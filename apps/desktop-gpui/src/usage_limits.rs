//! `packages/shared/src/usageLimits.ts`: the accounts, pools and notices behind the usage page's
//! Limits tab. One desktop window talks to one environment, so the multi-environment branches
//! (environment-prefixed notices, the "Signed in" list) collapse to the single environment label,
//! and the usage limit sources, which only the config stream carries, are not read.

use crate::usage::{Kind, Limits, Pace, Reason, ResetCredits, Window, format_duration};
use std::collections::HashMap;

pub const CHATGPT_USAGE_URL: &str = "https://chatgpt.com/#settings/Usage";

/// `CURSOR_USAGE_WINDOWS`: id, label and description.
pub const CURSOR_USAGE_WINDOWS: [(&str, &str, &str); 3] = [
    (
        "totalPercentUsed",
        "Overall",
        "Combined usage across both allowances, not a third quota.",
    ),
    (
        "autoPercentUsed",
        "Cursor Models",
        "Grok and Composer use this first. Auto can use either pool.",
    ),
    (
        "apiPercentUsed",
        "Other Models",
        "Claude, GPT, and Gemini use this pool. Grok and Composer fall back here.",
    ),
];

pub fn cursor_window_details(id: &str) -> Option<(&'static str, &'static str)> {
    CURSOR_USAGE_WINDOWS
        .iter()
        .find(|(window, ..)| *window == id)
        .map(|(_, label, description)| (*label, *description))
}

fn cursor_rank(id: &str) -> usize {
    CURSOR_USAGE_WINDOWS
        .iter()
        .position(|(window, ..)| *window == id)
        .unwrap_or(CURSOR_USAGE_WINDOWS.len())
}

/// The slice of a `ServerProvider` the limits read.
#[derive(Clone, Debug, PartialEq)]
pub struct Native {
    pub instance_id: String,
    pub driver: String,
    pub display_name: Option<String>,
    pub accent_color: Option<String>,
    pub status: String,
    pub enabled: bool,
    pub installed: bool,
    pub available: bool,
    pub authenticated: bool,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub workspace_id: Option<String>,
    pub sharing: bool,
    pub limits: Option<Limits>,
}

impl Native {
    /// `providersWithLimits`.
    fn reports_limits(&self) -> bool {
        self.enabled && self.installed && self.available && self.limits.is_some()
    }

    /// `usesChatGptSharing`.
    pub fn uses_chatgpt_sharing(&self) -> bool {
        self.authenticated && self.sharing
    }
}

/// `limitsNotice`.
pub fn limits_notice(limits: &Limits) -> Option<String> {
    limits.notice()
}

fn normalized_email(email: Option<&str>) -> Option<String> {
    email
        .map(|email| email.trim().to_lowercase())
        .filter(|email| !email.is_empty())
}

/// `accountKey`.
fn account_key(native: &Native) -> Option<String> {
    if let Some(email) = normalized_email(native.email.as_deref()) {
        return Some(format!("{}:{email}", native.driver));
    }
    let fingerprint = native.limits.as_ref()?.fingerprint.as_ref()?;
    Some(format!("{}:credential:{fingerprint}", native.driver))
}

/// `accountKeyResolver` over the enabled logins, even the unreadable workspaces.
fn key_resolver(natives: &[Native]) -> impl Fn(&Native) -> Option<String> + '_ {
    let mut workspaces: HashMap<String, Vec<&str>> = HashMap::new();
    for native in natives.iter().filter(|native| native.enabled) {
        if let (Some(key), Some(workspace)) = (account_key(native), native.workspace_id.as_deref())
        {
            let known = workspaces.entry(key).or_default();
            if !known.contains(&workspace) {
                known.push(workspace);
            }
        }
    }
    move |native| {
        let key = account_key(native)?;
        if let Some(workspace) = &native.workspace_id {
            return Some(format!("{key}:{workspace}"));
        }
        match workspaces.get(&key) {
            None => Some(key),
            Some(known) if known.len() == 1 => Some(format!("{key}:{}", known[0])),
            Some(_) => None,
        }
    }
}

/// `LimitAccount`, for the one environment.
#[derive(Clone, Debug, PartialEq)]
pub struct Account {
    pub key: String,
    pub driver: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub accent_color: Option<String>,
    /// The instance whose reset credit `provider.consumeResetCredit` spends.
    pub redeem: Option<String>,
    pub limits: Limits,
}

impl Account {
    fn sort_name(&self) -> String {
        self.display_name
            .as_deref()
            .or(self.email.as_deref())
            .unwrap_or(&self.key)
            .to_lowercase()
    }
}

/// `Date.parse(a) > Date.parse(b)`, false when either is `NaN`.
fn newer(next: Option<i64>, previous: Option<i64>) -> bool {
    matches!((next, previous), (Some(next), Some(previous)) if next > previous)
}

/// `collectLimitAccounts`: one account per login, however many instances share it.
pub fn collect_accounts(natives: &[Native]) -> Vec<Account> {
    struct Credit {
        checked_at: Option<i64>,
        credits: ResetCredits,
        redeem: Option<String>,
    }
    let resolve = key_resolver(natives);
    let mut accounts: Vec<(String, Account)> = Vec::new();
    let mut credits: HashMap<String, Credit> = HashMap::new();
    let mut plans: HashMap<String, (Option<i64>, String)> = HashMap::new();
    for native in natives.iter().filter(|native| native.reports_limits()) {
        let Some(limits) = native.limits.clone() else {
            continue;
        };
        if limits_notice(&limits).is_some() {
            continue;
        }
        let key = resolve(native).unwrap_or_else(|| format!("environment:{}", native.instance_id));
        let next = Account {
            key: format!("environment:{}", native.instance_id),
            driver: native.driver.clone(),
            display_name: native
                .display_name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            email: native.email.clone(),
            plan: native.plan.clone(),
            accent_color: native.accent_color.clone(),
            redeem: Some(native.instance_id.clone()),
            limits,
        };
        if let Some(next_credits) = &next.limits.reset_credits
            && credits
                .get(&key)
                .is_none_or(|previous| newer(next.limits.checked_at, previous.checked_at))
        {
            credits.insert(
                key.clone(),
                Credit {
                    checked_at: next.limits.checked_at,
                    credits: next_credits.clone(),
                    redeem: next.redeem.clone(),
                },
            );
        }
        if let Some(plan) = next.plan.as_ref().filter(|plan| !plan.is_empty())
            && plans
                .get(&key)
                .is_none_or(|(previous, _)| newer(next.limits.checked_at, *previous))
        {
            plans.insert(key.clone(), (next.limits.checked_at, plan.clone()));
        }
        let Some(slot) = accounts.iter().position(|(seen, _)| *seen == key) else {
            accounts.push((key, next));
            continue;
        };
        let previous = &accounts[slot].1;
        let fresher = newer(next.limits.checked_at, previous.limits.checked_at);
        let winner = if fresher { &next } else { previous };
        let credit = credits.get(&key);
        let merged = Account {
            display_name: previous
                .display_name
                .clone()
                .or_else(|| next.display_name.clone()),
            plan: plans
                .get(&key)
                .map(|(_, plan)| plan.clone())
                .or_else(|| previous.plan.clone())
                .or_else(|| next.plan.clone()),
            accent_color: previous
                .accent_color
                .clone()
                .or_else(|| next.accent_color.clone()),
            redeem: match credit {
                Some(credit) => credit.redeem.clone(),
                None => winner
                    .redeem
                    .clone()
                    .or_else(|| previous.redeem.clone())
                    .or_else(|| next.redeem.clone()),
            },
            limits: Limits {
                reset_credits: credit.map(|credit| credit.credits.clone()),
                ..winner.limits.clone()
            },
            ..previous.clone()
        };
        accounts[slot].1 = merged;
    }
    accounts.into_iter().map(|(_, account)| account).collect()
}

/// `collectLimitNotices` for one environment.
pub fn collect_notices(natives: &[Native]) -> Vec<String> {
    natives
        .iter()
        .filter(|native| native.reports_limits())
        .filter_map(|native| {
            let limits = native.limits.as_ref()?;
            if matches!(limits.unavailable, Some((Reason::Unsupported, _))) {
                return None;
            }
            let notice = limits_notice(limits)?;
            let name = native
                .display_name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or(&native.driver);
            Some(format!("{name}: {notice}"))
        })
        .collect()
}

/// One `collectExternalUsageLinks` entry.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub label: String,
    pub url: String,
    pub message: Option<String>,
    pub accounts: Vec<String>,
}

pub fn collect_external_links(natives: &[Native], environment: &str) -> Vec<Link> {
    let mut links: Vec<Link> = Vec::new();
    for native in natives.iter().filter(|native| native.reports_limits()) {
        let Some(limits) = &native.limits else {
            continue;
        };
        let Some(external) = &limits.external else {
            continue;
        };
        if !native.authenticated {
            continue;
        }
        let account = format!(
            "{} on {environment}",
            native
                .display_name
                .as_deref()
                .unwrap_or(&native.instance_id)
        );
        let message = limits
            .unavailable
            .as_ref()
            .and_then(|(_, message)| message.clone());
        match links.iter_mut().find(|link| link.url == external.url) {
            Some(link) => {
                link.label = external.label.clone();
                link.message = message;
                if !link.accounts.contains(&account) {
                    link.accounts.push(account);
                }
            }
            None => links.push(Link {
                label: external.label.clone(),
                url: external.url.clone(),
                message,
                accounts: vec![account],
            }),
        }
    }
    links
}

/// `LimitPoolMember`: an account's window in a pool, as an index into `Pool::accounts`.
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub account: usize,
    pub window: Window,
    pub refill_percent: f64,
}

/// One entry of `LimitPoolWindow.resets`: `member` indexes `PoolWindow::members`.
#[derive(Clone, Debug, PartialEq)]
pub struct Reset {
    pub member: usize,
    pub at: i64,
    pub restores_percent: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PoolWindow {
    pub id: String,
    pub kind: Kind,
    pub label: String,
    pub members: Vec<Member>,
    /// One slot per account of the pool, holding the index of its member when it reports the window.
    pub columns: Vec<Option<usize>>,
    pub remaining_percent: i64,
    pub used_percent: i64,
    pub pace: Option<Pace>,
    pub resets: Vec<Reset>,
}

impl PoolWindow {
    /// The reset that restores something for the member at `member`.
    pub fn reset_of(&self, member: usize) -> Option<&Reset> {
        self.resets.iter().find(|reset| reset.member == member)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pool {
    pub driver: String,
    pub accounts: Vec<Account>,
    pub windows: Vec<PoolWindow>,
}

impl Pool {
    /// `displayLimitWindows`.
    pub fn display_windows(&self) -> Vec<&PoolWindow> {
        let mut windows: Vec<&PoolWindow> = self.windows.iter().collect();
        match self.driver.as_str() {
            "antigravity" => {
                let family = |id: &str| {
                    ["-5h", "-weekly", "-quota"]
                        .iter()
                        .find_map(|suffix| id.strip_suffix(suffix))
                        .unwrap_or(id)
                        .to_lowercase()
                };
                windows.sort_by(|left, right| {
                    (!left.id.starts_with("gemini-"))
                        .cmp(&!right.id.starts_with("gemini-"))
                        .then_with(|| family(&left.id).cmp(&family(&right.id)))
                        .then_with(|| left.kind.cmp(&right.kind))
                });
            }
            "cursor" => {
                let has = |id: &str| self.windows.iter().any(|window| window.id == id);
                let both = has("autoPercentUsed") && has("apiPercentUsed");
                windows.retain(|window| !both || window.id != "totalPercentUsed");
                windows.sort_by_key(|window| cursor_rank(&window.id));
            }
            _ => {}
        }
        windows
    }
}

fn reset_millis(window: &Window) -> Option<i64> {
    window.resets_at
}

/// Codex ids name slots, so pool by kind; other drivers' ids identify distinct allowances.
fn pool_key(driver: &str, window: &Window) -> String {
    if driver == "codex" {
        window.kind.wire().to_owned()
    } else {
        format!("{}:{}", window.kind.wire(), window.id)
    }
}

/// `poolMember`: a spent weekly allowance blocks the session window, whatever it has left.
fn pool_member(account: &Account, reported: &Window, now: i64) -> (Window, f64) {
    let spent_weekly = (reported.kind == Kind::Session)
        .then(|| {
            account.limits.windows.iter().find(|window| {
                window.used_percent >= 100.
                    && match account.driver.as_str() {
                        "claudeAgent" => window.id == "seven_day",
                        "codex" => window.kind == Kind::Weekly,
                        _ => false,
                    }
            })
        })
        .flatten();
    let Some(spent) = spent_weekly else {
        return (reported.clone(), reported.used_percent);
    };
    let mut resets_at = None;
    let mut refill = 0.;
    // A stale or missing clock cannot tell us when blocked quota becomes usable.
    if let (Some(weekly), Some(session), Some(checked)) = (
        reset_millis(spent),
        reset_millis(reported),
        account.limits.checked_at,
    ) {
        let cutoff = now.max(checked);
        if weekly > cutoff && session > cutoff {
            if session <= weekly {
                resets_at = spent.resets_at;
                refill = 100.;
            } else if reported.used_percent >= 100. {
                resets_at = reported.resets_at;
                refill = 100.;
            } else {
                resets_at = spent.resets_at;
                refill = 100. - reported.used_percent.clamp(0., 100.);
            }
        }
    }
    (
        Window {
            used_percent: 100.,
            resets_at,
            duration_mins: None,
            ..reported.clone()
        },
        refill,
    )
}

/// `poolWindows`.
fn pool_windows(accounts: &[Account], now: i64) -> Vec<PoolWindow> {
    let mut by_key: Vec<(String, Vec<Member>)> = Vec::new();
    for (index, account) in accounts.iter().enumerate() {
        for reported in &account.limits.windows {
            let (window, refill_percent) = pool_member(account, reported, now);
            let pooled = pool_key(&account.driver, &window);
            let taken = by_key
                .iter()
                .find(|(key, _)| *key == pooled)
                .is_some_and(|(_, members)| members.iter().any(|member| member.account == index));
            let key = if taken {
                format!("{}:{}", window.kind.wire(), window.id)
            } else {
                pooled
            };
            let member = Member {
                account: index,
                window,
                refill_percent,
            };
            match by_key.iter_mut().find(|(seen, _)| *seen == key) {
                Some((_, members)) => members.push(member),
                None => by_key.push((key, vec![member])),
            }
        }
    }
    let mut windows: Vec<PoolWindow> = by_key
        .into_iter()
        .map(|(key, members)| {
            let count = members.len() as f64;
            let first = &members[0].window;
            let codex = accounts[members[0].account].driver == "codex";
            let used = members.iter().map(|m| m.window.used_percent).sum::<f64>() / count;
            // Judge pace only for windows with reset clocks; others add spend without elapsed time.
            let timed: Vec<(f64, f64)> = members
                .iter()
                .filter_map(|m| Some((m.window.used_percent, m.window.elapsed_share(now)?)))
                .collect();
            let pace = (!timed.is_empty()).then(|| {
                let total = timed.len() as f64;
                Pace::of_shares(
                    timed.iter().map(|(used, _)| used).sum::<f64>() / total,
                    timed.iter().map(|(_, elapsed)| elapsed).sum::<f64>() / total,
                )
            });
            let mut resets: Vec<Reset> = members
                .iter()
                .enumerate()
                .filter_map(|(member, entry)| {
                    Some(Reset {
                        member,
                        at: reset_millis(&entry.window)?,
                        restores_percent: (entry.refill_percent / count).round() as i64,
                    })
                })
                .collect();
            resets.sort_by_key(|reset| reset.at);
            let mut slots: HashMap<usize, usize> = HashMap::new();
            for (index, member) in members.iter().enumerate() {
                slots.insert(member.account, index);
            }
            PoolWindow {
                id: if codex { key } else { first.id.clone() },
                kind: first.kind,
                label: first.label.clone(),
                columns: (0..accounts.len())
                    .map(|account| slots.get(&account).copied())
                    .collect(),
                used_percent: used.round() as i64,
                remaining_percent: (100. - used).round() as i64,
                pace,
                resets,
                members,
            }
        })
        .collect();
    windows.sort_by_key(|window| window.kind);
    windows
}

/// `collectLimitPools`: accounts grouped by driver, soonest reset first.
pub fn collect_pools(accounts: &[Account], now: i64) -> Vec<Pool> {
    let mut drivers: Vec<(&str, Vec<&Account>)> = Vec::new();
    for account in accounts {
        match drivers
            .iter_mut()
            .find(|(driver, _)| *driver == account.driver)
        {
            Some((_, members)) => members.push(account),
            None => drivers.push((&account.driver, vec![account])),
        }
    }
    drivers
        .into_iter()
        .map(|(driver, members)| {
            let order_window = members
                .iter()
                .flat_map(|account| &account.limits.windows)
                .min_by_key(|window| window.kind);
            let order_reset = |account: &Account| -> f64 {
                let reset = order_window
                    .and_then(|order| {
                        account
                            .limits
                            .windows
                            .iter()
                            .find(|window| pool_key(driver, window) == pool_key(driver, order))
                    })
                    .and_then(|window| reset_millis(&pool_member(account, window, now).0));
                reset.map_or(f64::INFINITY, |at| at as f64)
            };
            let mut sorted: Vec<Account> = members.into_iter().cloned().collect();
            sorted.sort_by(|left, right| {
                order_reset(left)
                    .partial_cmp(&order_reset(right))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.sort_name().cmp(&right.sort_name()))
                    .then_with(|| left.key.cmp(&right.key))
            });
            let windows = pool_windows(&sorted, now);
            Pool {
                driver: driver.to_owned(),
                accounts: sorted,
                windows,
            }
        })
        .collect()
}

/// `accountInitials`.
pub fn account_initials(email: &str) -> String {
    let mut parts = email.split('@');
    let first = |part: Option<&str>| part.and_then(|part| part.chars().next());
    let initials: String = [first(parts.next()), first(parts.next())]
        .into_iter()
        .flatten()
        .collect::<String>()
        .to_uppercase();
    if initials.is_empty() {
        "?".to_owned()
    } else {
        initials
    }
}

/// `accountHue`.
pub fn account_hue(email: &str) -> u32 {
    let mut hash: i32 = 0;
    for unit in email.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
    }
    hash.unsigned_abs() % 360
}

/// `resetCreditsSummary`.
pub fn reset_credits_summary(credits: &ResetCredits, now: i64, compact: bool) -> String {
    let expires_in = credits.next_expires_at.map(|at| format_duration(at - now));
    if credits.available == 0 {
        return "No reset credits banked".to_owned();
    }
    if compact {
        return match expires_in {
            Some(expires) => format!("{} banked · expires in {expires}", credits.available),
            None => format!("{} banked", credits.available),
        };
    }
    let noun = if credits.available == 1 {
        "reset credit"
    } else {
        "reset credits"
    };
    match expires_in {
        Some(expires) => format!(
            "{} {noun} banked · next expires in {expires}",
            credits.available
        ),
        None => format!("{} {noun} banked", credits.available),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const HOUR: i64 = 3_600_000;
    const NOW: i64 = 1_000 * HOUR;

    fn iso(at: i64) -> String {
        let secs = at / 1000;
        let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
        // Civil date from days since the epoch (Hinnant).
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        format!(
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.000Z",
            rest / 3600,
            rest % 3600 / 60,
            rest % 60
        )
    }

    fn window(id: &str, kind: &str, used: f64, resets: Option<i64>, mins: Option<f64>) -> Value {
        let mut value = json!({"id": id, "kind": kind, "label": id, "usedPercent": used});
        if let Some(at) = resets {
            value["resetsAt"] = json!(iso(at));
        }
        if let Some(mins) = mins {
            value["windowDurationMins"] = json!(mins);
        }
        value
    }

    use serde_json::Value;

    fn limits(checked: i64, windows: Vec<Value>) -> Limits {
        Limits::decode(&json!({"checkedAt": iso(checked), "windows": windows})).expect("limits")
    }

    fn native(instance: &str, driver: &str, email: Option<&str>, limits: Limits) -> Native {
        Native {
            instance_id: instance.to_owned(),
            driver: driver.to_owned(),
            display_name: Some(instance.to_owned()),
            accent_color: None,
            status: "ready".to_owned(),
            enabled: true,
            installed: true,
            available: true,
            authenticated: true,
            email: email.map(str::to_owned),
            plan: None,
            workspace_id: None,
            sharing: false,
            limits: Some(limits),
        }
    }

    fn session(used: f64, resets: i64) -> Value {
        window("five_hour", "session", used, Some(resets), Some(300.))
    }

    #[test]
    fn instances_of_one_login_merge_into_one_account() {
        let older = limits(NOW - HOUR, vec![session(10., NOW + HOUR)]);
        let mut newer = limits(NOW, vec![session(40., NOW + HOUR)]);
        newer.reset_credits = Some(ResetCredits {
            available: 2,
            next_expires_at: None,
        });
        let mut first = native("a", "codex", Some("Me@x.io"), older);
        first.plan = Some("Plus".to_owned());
        let second = native("b", "codex", Some("me@x.io"), newer);
        let accounts = collect_accounts(&[first, second]);
        assert_eq!(accounts.len(), 1);
        let account = &accounts[0];
        assert_eq!(account.display_name.as_deref(), Some("a"));
        assert_eq!(account.plan.as_deref(), Some("Plus"));
        assert_eq!(account.limits.windows[0].used_percent, 40.);
        assert_eq!(account.redeem.as_deref(), Some("b"));
        assert_eq!(
            account.limits.reset_credits.as_ref().map(|c| c.available),
            Some(2)
        );
    }

    #[test]
    fn accounts_skip_providers_without_usable_limits() {
        let usable = limits(NOW, vec![session(10., NOW + HOUR)]);
        let empty = limits(NOW, vec![]);
        let mut disabled = native("c", "codex", Some("c@x.io"), usable.clone());
        disabled.enabled = false;
        let mut unavailable = native("d", "codex", Some("d@x.io"), usable.clone());
        unavailable.available = false;
        let natives = [
            native("a", "codex", Some("a@x.io"), usable),
            native("b", "codex", Some("b@x.io"), empty),
            disabled,
            unavailable,
        ];
        let accounts = collect_accounts(&natives);
        assert_eq!(accounts.len(), 1);
        assert_eq!(collect_notices(&natives), ["b: No limits reported."]);
    }

    #[test]
    fn an_unreadable_workspace_names_the_login_s_other_account() {
        let usable = limits(NOW, vec![session(10., NOW + HOUR)]);
        let mut work = native("work", "codex", Some("me@x.io"), usable.clone());
        work.workspace_id = Some("w1".to_owned());
        let personal = native("personal", "codex", Some("me@x.io"), usable);
        let pair = [work.clone(), personal.clone()];
        let resolve = key_resolver(&pair);
        assert_eq!(resolve(&work).as_deref(), Some("codex:me@x.io:w1"));
        assert_eq!(resolve(&personal).as_deref(), Some("codex:me@x.io:w1"));
        let mut other = personal.clone();
        other.workspace_id = Some("w2".to_owned());
        let trio = [work, other, personal.clone()];
        let resolve = key_resolver(&trio);
        assert_eq!(resolve(&personal), None);
    }

    #[test]
    fn pools_sort_accounts_by_the_soonest_reset() {
        let later = native(
            "a",
            "claudeAgent",
            Some("a@x.io"),
            limits(NOW, vec![session(10., NOW + 3 * HOUR)]),
        );
        let sooner = native(
            "b",
            "claudeAgent",
            Some("b@x.io"),
            limits(NOW, vec![session(30., NOW + HOUR)]),
        );
        let accounts = collect_accounts(&[later, sooner]);
        let pools = collect_pools(&accounts, NOW);
        assert_eq!(pools.len(), 1);
        let pool = &pools[0];
        let names: Vec<_> = pool
            .accounts
            .iter()
            .map(|a| a.display_name.as_deref())
            .collect();
        assert_eq!(names, [Some("b"), Some("a")]);
        let window = &pool.windows[0];
        assert_eq!(window.remaining_percent, 80);
        assert_eq!(window.columns, [Some(0), Some(1)]);
        assert_eq!(window.resets.len(), 2);
        assert_eq!(window.resets[0].at, NOW + HOUR);
    }

    #[test]
    fn a_spent_weekly_window_blocks_the_session_window() {
        let weekly = window(
            "seven_day",
            "weekly",
            100.,
            Some(NOW + 20 * HOUR),
            Some(10_080.),
        );
        let spent = |session_used, session_resets| {
            let account = Account {
                key: "k".to_owned(),
                driver: "claudeAgent".to_owned(),
                display_name: None,
                email: None,
                plan: None,
                accent_color: None,
                redeem: None,
                limits: limits(
                    NOW,
                    vec![
                        window(
                            "five_hour",
                            "session",
                            session_used,
                            Some(session_resets),
                            Some(300.),
                        ),
                        weekly.clone(),
                    ],
                ),
            };
            let reported = account.limits.windows[0].clone();
            pool_member(&account, &reported, NOW)
        };
        // The session refills first, so the weekly reset is when it becomes usable.
        let (blocked, refill) = spent(20., NOW + 2 * HOUR);
        assert_eq!(blocked.used_percent, 100.);
        assert_eq!(blocked.duration_mins, None);
        assert_eq!(blocked.resets_at, Some(NOW + 20 * HOUR));
        assert_eq!(refill, 100.);
        // The session outlasts the weekly window and is itself spent.
        let (blocked, refill) = spent(100., NOW + 30 * HOUR);
        assert_eq!(blocked.resets_at, Some(NOW + 30 * HOUR));
        assert_eq!(refill, 100.);
        // The session outlasts the weekly window with room left: only that room comes back.
        let (blocked, refill) = spent(25., NOW + 30 * HOUR);
        assert_eq!(blocked.resets_at, Some(NOW + 20 * HOUR));
        assert_eq!(refill, 75.);
        // A session without a clock has no reset to wait for.
        let account = Account {
            limits: limits(
                NOW,
                vec![
                    window("five_hour", "session", 5., None, None),
                    weekly.clone(),
                ],
            ),
            key: "k".to_owned(),
            driver: "claudeAgent".to_owned(),
            display_name: None,
            email: None,
            plan: None,
            accent_color: None,
            redeem: None,
        };
        let reported = account.limits.windows[0].clone();
        let (blocked, refill) = pool_member(&account, &reported, NOW);
        assert_eq!((blocked.resets_at, refill), (None, 0.));
    }

    #[test]
    fn pooled_windows_average_use_and_judge_pace_over_the_timed_ones() {
        let half = |used| {
            window(
                "five_hour",
                "session",
                used,
                Some(NOW + 150 * 60_000),
                Some(300.),
            )
        };
        let accounts = collect_accounts(&[
            native("a", "codex", Some("a@x.io"), limits(NOW, vec![half(20.)])),
            native("b", "codex", Some("b@x.io"), limits(NOW, vec![half(80.)])),
        ]);
        let pools = collect_pools(&accounts, NOW);
        let window = &pools[0].windows[0];
        assert_eq!(window.used_percent, 50);
        assert_eq!(window.remaining_percent, 50);
        assert_eq!(window.pace, Some(Pace::On));
        assert_eq!(window.id, "session");
    }

    #[test]
    fn windows_order_by_kind_then_reporting_order() {
        let accounts = collect_accounts(&[native(
            "a",
            "claudeAgent",
            Some("a@x.io"),
            limits(
                NOW,
                vec![
                    window("seven_day", "weekly", 10., None, None),
                    window("five_hour", "session", 10., None, None),
                ],
            ),
        )]);
        let pools = collect_pools(&accounts, NOW);
        let ids: Vec<_> = pools[0].windows.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["five_hour", "seven_day"]);
    }

    fn pool_of(driver: &str, ids: &[(&str, &str)]) -> Pool {
        let windows = ids
            .iter()
            .map(|(id, kind)| window(id, kind, 10., None, None))
            .collect();
        let accounts =
            collect_accounts(&[native("a", driver, Some("a@x.io"), limits(NOW, windows))]);
        collect_pools(&accounts, NOW).remove(0)
    }

    #[test]
    fn cursor_hides_overall_when_both_allowances_exist() {
        let pool = pool_of(
            "cursor",
            &[
                ("totalPercentUsed", "monthly"),
                ("apiPercentUsed", "monthly"),
                ("autoPercentUsed", "monthly"),
            ],
        );
        let ids: Vec<_> = pool
            .display_windows()
            .iter()
            .map(|w| w.id.clone())
            .collect();
        assert_eq!(ids, ["autoPercentUsed", "apiPercentUsed"]);
        let lone = pool_of("cursor", &[("totalPercentUsed", "monthly")]);
        assert_eq!(lone.display_windows().len(), 1);
    }

    #[test]
    fn antigravity_lists_gemini_first_and_pairs_a_family() {
        let pool = pool_of(
            "antigravity",
            &[
                ("claude-weekly", "weekly"),
                ("gemini-weekly", "weekly"),
                ("claude-5h", "session"),
                ("gemini-5h", "session"),
            ],
        );
        let ids: Vec<_> = pool
            .display_windows()
            .iter()
            .map(|w| w.id.clone())
            .collect();
        assert_eq!(
            ids,
            ["gemini-5h", "gemini-weekly", "claude-5h", "claude-weekly"]
        );
    }

    #[test]
    fn external_links_group_accounts_by_url() {
        let mut shared = limits(NOW, vec![session(10., NOW + HOUR)]);
        shared.external = Some(crate::usage::External {
            label: "Usage".to_owned(),
            url: CHATGPT_USAGE_URL.to_owned(),
        });
        let natives = [
            native("a", "codex", Some("a@x.io"), shared.clone()),
            native("b", "codex", Some("b@x.io"), shared),
        ];
        let links = collect_external_links(&natives, "Local");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].accounts, ["a on Local", "b on Local"]);
    }

    #[test]
    fn avatars_follow_the_web_hash() {
        assert_eq!(account_initials("jo@x.io"), "JX");
        assert_eq!(account_initials("@"), "?");
        assert_eq!(account_initials("solo"), "S");
        assert_eq!(account_hue(""), 0);
        // 97 * 31 + 98 = 3105; 3105 % 360 = 225.
        assert_eq!(account_hue("ab"), 225);
        let long = "a-very-long-address@example-domain.com";
        assert!(account_hue(long) < 360);
    }

    #[test]
    fn reset_credit_summaries_name_the_next_expiry() {
        let credits = |available, expires| ResetCredits {
            available,
            next_expires_at: expires,
        };
        assert_eq!(
            reset_credits_summary(&credits(0, None), NOW, false),
            "No reset credits banked"
        );
        assert_eq!(
            reset_credits_summary(&credits(1, None), NOW, false),
            "1 reset credit banked"
        );
        assert_eq!(
            reset_credits_summary(&credits(3, Some(NOW + 26 * HOUR)), NOW, false),
            "3 reset credits banked · next expires in 1d 2h"
        );
        assert_eq!(
            reset_credits_summary(&credits(3, Some(NOW + 2 * HOUR)), NOW, true),
            "3 banked · expires in 2h 0m"
        );
    }

    #[test]
    fn chatgpt_sharing_needs_an_authenticated_login() {
        let mut shared = native("a", "codex", None, limits(NOW, vec![]));
        shared.sharing = true;
        assert!(shared.uses_chatgpt_sharing());
        shared.authenticated = false;
        assert!(!shared.uses_chatgpt_sharing());
    }
}
