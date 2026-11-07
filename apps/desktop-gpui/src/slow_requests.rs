//! Requests the server has not answered for too long, and the toast that says so
//! (`rpc/requestLatencyState.ts`, `SlowRpcRequestToastCoordinator.tsx`).

use crate::toast::{Collapse, Kind, Timeout, Toast};
use gpui::{FontWeight, ParentElement, Styled, div, prelude::*, px};
use gpui_component::{ActiveTheme, v_flex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// `DEFAULT_SLOW_RPC_ACK_THRESHOLD_MS`.
const DEFAULT_THRESHOLD: Duration = Duration::from_secs(15);
/// `LONG_RUNNING_SLOW_RPC_ACK_THRESHOLD_MS`.
const LONG_RUNNING_THRESHOLD: Duration = Duration::from_secs(120);
const LONG_RUNNING: [&str; 4] = [
    "server.updateProvider",
    "server.refreshProviders",
    "server.updateServer",
    "vcs.refreshStatus",
];
const UNTRACKED: [&str; 3] = [
    "server.getUsageSummary",
    "server.reportClientActivity",
    "sourceControl.signInWait",
];
/// `MAX_TRACKED_REQUESTS`.
const MAX_TRACKED: usize = 256;

fn threshold(method: &str) -> Option<Duration> {
    if UNTRACKED.contains(&method)
        || method.contains("subscribe")
        || method.starts_with("pullRequests.")
    {
        return None;
    }
    Some(if LONG_RUNNING.contains(&method) {
        LONG_RUNNING_THRESHOLD
    } else {
        DEFAULT_THRESHOLD
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlowRequest {
    pub id: String,
    pub method: &'static str,
    /// Wall clock, for the "Started" line.
    pub started_at: SystemTime,
    pub threshold: Duration,
}

struct Tracked {
    request: SlowRequest,
    started: Instant,
}

impl Tracked {
    fn turns_slow(&self) -> Instant {
        self.started + self.request.threshold
    }
}

/// The unary requests sent and not yet answered. Streams are never started here.
#[derive(Default)]
pub struct Tracker {
    pending: Vec<Tracked>,
}

impl Tracker {
    pub fn start(&mut self, id: &str, method: &'static str, now: Instant, wall: SystemTime) {
        let Some(threshold) = threshold(method) else {
            return;
        };
        if self.pending.len() >= MAX_TRACKED {
            self.pending.remove(0);
        }
        self.pending.push(Tracked {
            request: SlowRequest {
                id: id.to_string(),
                method,
                started_at: wall,
                threshold,
            },
            started: now,
        });
    }

    pub fn finish(&mut self, id: &str) {
        self.pending.retain(|tracked| tracked.request.id != id);
    }

    /// Those past their threshold, in the order they turned slow.
    pub fn slow(&self, now: Instant) -> Vec<SlowRequest> {
        let mut slow: Vec<&Tracked> = self
            .pending
            .iter()
            .filter(|tracked| tracked.turns_slow() <= now)
            .collect();
        slow.sort_by_key(|tracked| tracked.turns_slow());
        slow.into_iter()
            .map(|tracked| tracked.request.clone())
            .collect()
    }

    /// How long until the next waiting request turns slow.
    pub fn next_slow(&self, now: Instant) -> Option<Duration> {
        self.pending
            .iter()
            .map(Tracked::turns_slow)
            .filter(|turns_slow| *turns_slow > now)
            .min()
            .map(|turns_slow| turns_slow - now)
    }
}

/// `${count} request${s} waiting longer than ${Math.round(min threshold / 1000)}s.`
fn describe(requests: &[SlowRequest]) -> String {
    let seconds = requests
        .iter()
        .map(|request| request.threshold.as_millis())
        .min()
        .map_or(0, |millis| (millis + 500) / 1000);
    format!(
        "{} request{} waiting longer than {seconds}s.",
        requests.len(),
        if requests.len() == 1 { "" } else { "s" }
    )
}

/// `Date#toLocaleTimeString` in en-US, as `3:07:09 PM`.
fn format_clock(hour: u32, minute: u32, second: u32) -> String {
    let twelve = match hour % 12 {
        0 => 12,
        hour => hour,
    };
    let meridiem = if hour < 12 { "AM" } else { "PM" };
    format!("{twelve}:{minute:02}:{second:02} {meridiem}")
}

fn local_clock(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs()) as libc::time_t;
    // SAFETY: `localtime_r` writes only into the zeroed `tm` it is given.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&seconds, &mut tm).is_null() {
            return String::new();
        }
        tm
    };
    format_clock(tm.tm_hour as u32, tm.tm_min as u32, tm.tm_sec as u32)
}

/// The one warning toast, with the waiting requests in its collapsible body.
pub fn toast(label: &str, requests: &[SlowRequest]) -> Toast {
    let items: Vec<(String, String)> = requests
        .iter()
        .map(|request| {
            (
                format!("{} \u{b7} {label}", request.method),
                format!("Started {}", local_clock(request.started_at)),
            )
        })
        .collect();
    Toast::new("Some requests are slow")
        .kind(Kind::Warning)
        .description(describe(requests))
        .timeout(Timeout::Never)
        .collapse(Collapse::new("Show requests", "Hide requests").on_description())
        .body(move |_, cx| {
            let border = cx.theme().border.opacity(0.5);
            let muted = cx.theme().muted_foreground;
            let foreground = cx.theme().foreground;
            let last = items.len().saturating_sub(1);
            v_flex()
                .gap(px(10.))
                .text_xs()
                .text_color(muted)
                .children(items.iter().enumerate().map(|(index, (tag, started))| {
                    v_flex()
                        .min_w_0()
                        .when(index != last, |item| {
                            item.border_b_1().border_color(border).pb(px(8.))
                        })
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(foreground)
                                .child(tag.clone()),
                        )
                        .child(
                            div()
                                .mt(px(2.))
                                .text_size(px(10.))
                                .line_height(px(14.))
                                .opacity(0.75)
                                .child(started.clone()),
                        )
                }))
                .into_any_element()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }

    fn started(tracker: &mut Tracker, id: &str, method: &'static str, at: Instant) {
        tracker.start(id, method, at, SystemTime::UNIX_EPOCH);
    }

    fn ids(requests: Vec<SlowRequest>) -> Vec<String> {
        requests.into_iter().map(|request| request.id).collect()
    }

    #[test]
    fn a_request_is_slow_after_its_threshold_until_it_finishes() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        started(&mut tracker, "1", "projects.mutate", start);
        assert!(tracker.slow(start + secs(14)).is_empty());
        assert_eq!(tracker.next_slow(start + secs(14)), Some(secs(1)));
        assert_eq!(ids(tracker.slow(start + secs(15))), ["1"]);
        assert_eq!(tracker.next_slow(start + secs(15)), None);
        tracker.finish("1");
        assert!(tracker.slow(start + secs(99)).is_empty());
    }

    #[test]
    fn long_running_methods_get_the_longer_threshold() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        started(&mut tracker, "1", "vcs.refreshStatus", start);
        assert!(tracker.slow(start + secs(119)).is_empty());
        assert_eq!(tracker.slow(start + secs(120))[0].threshold, secs(120));
    }

    #[test]
    fn exempt_methods_are_not_tracked() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        for method in [
            "orchestration.subscribeShell",
            "subscribeVcsStatus",
            "pullRequests.list",
            "server.getUsageSummary",
            "server.reportClientActivity",
            "sourceControl.signInWait",
        ] {
            started(&mut tracker, "1", method, start);
        }
        assert!(tracker.slow(start + secs(1000)).is_empty());
    }

    #[test]
    fn slow_requests_are_ordered_by_when_they_turned_slow() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        started(&mut tracker, "long", "vcs.refreshStatus", start);
        started(&mut tracker, "late", "projects.mutate", start + secs(10));
        started(&mut tracker, "early", "projects.mutate", start);
        assert_eq!(
            ids(tracker.slow(start + secs(200))),
            ["early", "late", "long"]
        );
    }

    #[test]
    fn the_oldest_request_is_dropped_past_the_cap() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        for id in 0..=MAX_TRACKED {
            started(&mut tracker, &id.to_string(), "projects.mutate", start);
        }
        let slow = ids(tracker.slow(start + secs(15)));
        assert_eq!(slow.len(), MAX_TRACKED);
        assert!(!slow.contains(&"0".to_string()));
    }

    #[test]
    fn the_description_counts_and_names_the_shortest_threshold() {
        let request = |threshold| SlowRequest {
            id: String::new(),
            method: "m",
            started_at: SystemTime::UNIX_EPOCH,
            threshold,
        };
        assert_eq!(
            describe(&[request(secs(15))]),
            "1 request waiting longer than 15s."
        );
        assert_eq!(
            describe(&[request(secs(120)), request(secs(15))]),
            "2 requests waiting longer than 15s."
        );
    }

    #[test]
    fn the_clock_reads_like_en_us() {
        assert_eq!(format_clock(0, 5, 9), "12:05:09 AM");
        assert_eq!(format_clock(12, 0, 0), "12:00:00 PM");
        assert_eq!(format_clock(15, 7, 9), "3:07:09 PM");
    }
}
