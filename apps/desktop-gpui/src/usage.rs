//! The usage limits row of the thread details card: `ThreadDetailsUsage.tsx` over
//! `packages/shared/src/usageLimits.ts`. A provider that cannot report limits shows one muted
//! "Limits  Unavailable" row whose tooltip says why.

use serde_json::Value;

const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

/// `ServerProviderUsageWindow.kind`; the declaration order is `WINDOW_KIND_ORDER`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Session,
    Weekly,
    Monthly,
    Other,
}

impl Kind {
    fn decode(text: &str) -> Option<Self> {
        Some(match text {
            "session" => Self::Session,
            "weekly" => Self::Weekly,
            "monthly" => Self::Monthly,
            "other" => Self::Other,
            _ => return None,
        })
    }

    pub fn wire(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Other => "other",
        }
    }
}

/// `ServerProviderUsageWindow`.
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub id: String,
    pub kind: Kind,
    pub label: String,
    pub used_percent: f64,
    pub resets_at: Option<i64>,
    pub duration_mins: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Unsupported,
    ProbeFailed,
}

/// `ServerProviderResetCredits`.
#[derive(Clone, Debug, PartialEq)]
pub struct ResetCredits {
    pub available: i64,
    pub next_expires_at: Option<i64>,
}

/// `ServerProviderUsageLimits.externalUsage`.
#[derive(Clone, Debug, PartialEq)]
pub struct External {
    pub label: String,
    pub url: String,
}

/// `ServerProviderUsageLimits`.
#[derive(Clone, Debug, PartialEq)]
pub struct Limits {
    pub windows: Vec<Window>,
    pub unavailable: Option<(Reason, Option<String>)>,
    /// `Date.parse(checkedAt)`: `None` stands for `NaN`, which compares false with everything.
    pub checked_at: Option<i64>,
    pub fingerprint: Option<String>,
    pub reset_credits: Option<ResetCredits>,
    pub external: Option<External>,
}

impl Limits {
    /// `None` when the provider reports no `usageLimits` at all.
    pub fn decode(value: &Value) -> Option<Self> {
        if !value.is_object() {
            return None;
        }
        let windows = value["windows"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|window| {
                Some(Window {
                    id: window["id"].as_str()?.to_owned(),
                    kind: Kind::decode(window["kind"].as_str()?)?,
                    label: window["label"].as_str()?.to_owned(),
                    used_percent: window["usedPercent"].as_f64()?,
                    resets_at: window["resetsAt"].as_str().and_then(parse_timestamp),
                    duration_mins: window["windowDurationMins"].as_f64(),
                })
            })
            .collect();
        let unavailable = value["unavailable"].as_object().and_then(|entry| {
            let reason = match entry.get("reason")?.as_str()? {
                "unsupported" => Reason::Unsupported,
                "probeFailed" => Reason::ProbeFailed,
                _ => return None,
            };
            let message = entry
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_owned);
            Some((reason, message))
        });
        let reset_credits = value["resetCredits"].as_object().and_then(|entry| {
            Some(ResetCredits {
                available: entry.get("availableCount")?.as_i64()?,
                next_expires_at: entry
                    .get("nextExpiresAt")
                    .and_then(Value::as_str)
                    .and_then(parse_timestamp),
            })
        });
        let external = value["externalUsage"].as_object().and_then(|entry| {
            Some(External {
                label: entry.get("label")?.as_str()?.to_owned(),
                url: entry.get("url")?.as_str()?.to_owned(),
            })
        });
        Some(Self {
            windows,
            unavailable,
            checked_at: value["checkedAt"].as_str().and_then(parse_timestamp),
            fingerprint: value["credentialFingerprint"].as_str().map(str::to_owned),
            reset_credits,
            external,
        })
    }

    /// `limitsNotice`: why the windows cannot be shown, or `None` when they can.
    pub fn notice(&self) -> Option<String> {
        match &self.unavailable {
            Some((Reason::Unsupported, message)) => Some(
                message
                    .clone()
                    .unwrap_or_else(|| "This account has no subscription limits.".to_owned()),
            ),
            Some((Reason::ProbeFailed, message)) => Some(
                message
                    .clone()
                    .unwrap_or_else(|| "Could not read limits.".to_owned()),
            ),
            None if self.windows.is_empty() => Some("No limits reported.".to_owned()),
            None => None,
        }
    }
}

/// `Math.round`: halves round toward positive infinity, so -0.5 is 0 and 2.5 is 3.
pub fn js_round(value: f64) -> i64 {
    (value + 0.5).floor() as i64
}

impl Window {
    /// `remainingPercent`.
    pub fn remaining_percent(&self) -> i64 {
        js_round(100. - self.used_percent.clamp(0., 100.))
    }

    /// `elapsedShare`: `None` without a reset time and window length.
    pub fn elapsed_share(&self, now: i64) -> Option<f64> {
        let resets_at = self.resets_at?;
        let length = self.duration_mins? * MINUTE as f64;
        if length <= 0. {
            return None;
        }
        Some(((length - (resets_at - now) as f64) / length).clamp(0., 1.))
    }

    /// `paceRemainingPercent`: the share of the window still to come, where even spending would sit.
    pub fn pace_remaining_percent(&self, now: i64) -> Option<i64> {
        Some(js_round((1. - self.elapsed_share(now)?) * 100.))
    }

    /// `paceOf`.
    pub fn pace(&self, now: i64) -> Option<Pace> {
        Some(Pace::of_shares(self.used_percent, self.elapsed_share(now)?))
    }

    /// `formatResetsIn`.
    pub fn resets_in(&self, now: i64) -> Option<String> {
        let at = self.resets_at?;
        Some(if at <= now {
            "resets now".to_owned()
        } else if at - now < MINUTE {
            "Resets momentarily".to_owned()
        } else {
            format!("resets in {}", format_duration(at - now))
        })
    }

    /// The row's tooltip lines: room left, when it resets, and the pace.
    pub fn tooltip(&self, now: i64) -> Vec<String> {
        let mut lines = vec![format!(
            "{}: {}% left",
            self.label,
            self.remaining_percent()
        )];
        if let Some(resets_at) = self.resets_at.filter(|at| *at > now) {
            lines.push(format!("Resets in {}", format_duration(resets_at - now)));
        }
        if let Some(pace) = self.pace(now) {
            lines.push(pace.label().to_owned());
        }
        lines
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    Ahead,
    On,
    Under,
}

impl Pace {
    /// `paceOfShares`.
    pub fn of_shares(used_percent: f64, elapsed: f64) -> Self {
        let gap = used_percent - elapsed * 100.;
        if gap > 5. {
            Self::Ahead
        } else if gap < -5. {
            Self::Under
        } else {
            Self::On
        }
    }

    /// `PACE[pace].label` of the usage page.
    pub fn description(self) -> &'static str {
        match self {
            Self::Ahead => "Ahead of pace: spending faster than the window elapses",
            Self::On => "On pace with the window",
            Self::Under => "Under pace: headroom left for the rest of the window",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ahead => "Ahead of pace",
            Self::On => "On pace",
            Self::Under => "Under pace",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Error,
    Warning,
}

/// The tone `LimitRow` gives a bar: red at 10% left, amber at 20%.
pub fn tone(left: i64) -> Option<Tone> {
    if left <= 10 {
        Some(Tone::Error)
    } else if left <= 20 {
        Some(Tone::Warning)
    } else {
        None
    }
}

/// `formatDuration`.
pub fn format_duration(ms: i64) -> String {
    let remaining = ms.max(0);
    let days = remaining / DAY;
    let hours = remaining % DAY / HOUR;
    let minutes = remaining % HOUR / MINUTE;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// Milliseconds since the epoch of an ISO 8601 timestamp (`Date.parse` for the forms the server writes).
pub fn parse_timestamp(text: &str) -> Option<i64> {
    let (date, rest) = text.split_once('T')?;
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let split = rest.find(['Z', '+', '-']).unwrap_or(rest.len());
    let (clock, zone) = rest.split_at(split);
    let mut fields = clock.split(':');
    let hour: i64 = fields.next()?.parse().ok()?;
    let minute: i64 = fields.next()?.parse().ok()?;
    let seconds: f64 = fields.next().unwrap_or("0").parse().ok()?;
    let offset = match zone {
        "" | "Z" => 0,
        zone => {
            let sign = if zone.starts_with('-') { -1 } else { 1 };
            let (h, m) = zone[1..].split_once(':')?;
            sign * (h.parse::<i64>().ok()? * HOUR + m.parse::<i64>().ok()? * MINUTE)
        }
    };
    // Howard Hinnant's days_from_civil.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * DAY + hour * HOUR + minute * MINUTE + (seconds * 1000.).round() as i64 - offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn js_round_rounds_halves_toward_positive_infinity() {
        assert_eq!(js_round(-0.5), 0);
        assert_eq!(js_round(2.5), 3);
        assert_eq!(js_round(-1.5), -1);
        assert_eq!(js_round(2.4), 2);
        assert_eq!(js_round(-2.6), -3);
    }

    fn window(used: f64, resets: Option<i64>, mins: Option<f64>) -> Window {
        Window {
            id: "five_hour".to_owned(),
            kind: Kind::Session,
            label: "5h".to_owned(),
            used_percent: used,
            resets_at: resets,
            duration_mins: mins,
        }
    }

    #[test]
    fn a_failed_probe_names_the_error() {
        let limits = Limits::decode(&json!({
            "windows": [],
            "unavailable": {
                "reason": "probeFailed",
                "message": "Codex could not read usage (JSON-RPC -32600)."
            },
            "checkedAt": "2026-01-01T00:00:00.000Z",
        }))
        .expect("limits");
        assert_eq!(
            limits.notice().as_deref(),
            Some("Codex could not read usage (JSON-RPC -32600).")
        );
    }

    #[test]
    fn notices_fall_back_to_the_stock_wording() {
        let notice = |value: Value| Limits::decode(&value).expect("limits").notice();
        let unsupported = json!({"windows": [], "unavailable": {"reason": "unsupported"}});
        assert_eq!(
            notice(unsupported).as_deref(),
            Some("This account has no subscription limits.")
        );
        let failed = json!({"windows": [], "unavailable": {"reason": "probeFailed"}});
        assert_eq!(notice(failed).as_deref(), Some("Could not read limits."));
        assert_eq!(
            notice(json!({"windows": []})).as_deref(),
            Some("No limits reported.")
        );
        let windows =
            json!({"windows": [{"id": "a", "kind": "session", "label": "5h", "usedPercent": 10}]});
        assert_eq!(notice(windows), None);
        assert!(Limits::decode(&Value::Null).is_none());
    }

    #[test]
    fn remaining_percent_clamps_and_rounds() {
        assert_eq!(window(0., None, None).remaining_percent(), 100);
        assert_eq!(window(33.4, None, None).remaining_percent(), 67);
        assert_eq!(window(140., None, None).remaining_percent(), 0);
        assert_eq!(window(-5., None, None).remaining_percent(), 100);
    }

    #[test]
    fn tones_flag_the_last_fifth() {
        assert_eq!(tone(10), Some(Tone::Error));
        assert_eq!(tone(11), Some(Tone::Warning));
        assert_eq!(tone(20), Some(Tone::Warning));
        assert_eq!(tone(21), None);
    }

    #[test]
    fn durations_read_in_the_two_largest_units() {
        assert_eq!(format_duration(-5), "0m");
        assert_eq!(format_duration(59 * MINUTE), "59m");
        assert_eq!(format_duration(HOUR + 5 * MINUTE), "1h 5m");
        assert_eq!(format_duration(2 * DAY + 3 * HOUR + 59 * MINUTE), "2d 3h");
    }

    #[test]
    fn pace_compares_use_with_time_elapsed() {
        let now = 1_000 * HOUR;
        // A 10 hour window with 5 hours left is half elapsed.
        let at = |used| window(used, Some(now + 5 * HOUR), Some(600.)).pace(now);
        assert_eq!(at(60.), Some(Pace::Ahead));
        assert_eq!(at(55.), Some(Pace::On));
        assert_eq!(at(45.), Some(Pace::On));
        assert_eq!(at(44.), Some(Pace::Under));
        assert_eq!(window(50., None, Some(600.)).pace(now), None);
        assert_eq!(window(50., Some(now), None).pace(now), None);
    }

    #[test]
    fn the_tooltip_lists_room_reset_and_pace() {
        let now = 1_000 * HOUR;
        let shown = window(50., Some(now + 5 * HOUR), Some(600.));
        assert_eq!(
            shown.tooltip(now),
            ["5h: 50% left", "Resets in 5h 0m", "On pace"]
        );
        let past = window(50., Some(now - 1), None);
        assert_eq!(past.tooltip(now), ["5h: 50% left"]);
    }

    #[test]
    fn timestamps_parse_like_date_parse() {
        assert_eq!(parse_timestamp("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_timestamp("2026-01-01T00:00:00.000Z"),
            Some(1_767_225_600_000)
        );
        assert_eq!(
            parse_timestamp("2026-01-01T02:00:00+02:00"),
            Some(1_767_225_600_000)
        );
        assert_eq!(
            parse_timestamp("2024-02-29T12:30:15.500Z"),
            Some(1_709_209_815_500)
        );
        assert_eq!(parse_timestamp("soon"), None);
    }
}
