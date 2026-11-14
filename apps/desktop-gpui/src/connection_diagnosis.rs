//! What failed on the way to an environment, in order, and what to try: a port of
//! `presentConnectionDiagnosis` (packages/client-runtime/src/connection/diagnosis.ts). The
//! wording, hop order and fact order are the web's.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hop {
    Device,
    Relay,
    Tunnel,
    Server,
}

/// `ConnectionFailureCause` plus the causes `locate` derives itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    TunnelDown,
    OriginUnreachable,
    Dns,
    Tls,
    Refused,
    Timeout,
    InvalidResponse,
    Unknown,
    ClockSkew,
    NotLinked,
    RelayError,
    TunnelPending,
    TunnelNotEnabled,
    ApprovalPending,
    Offline,
    Outdated,
    Released,
    SessionPending,
}

impl Cause {
    /// The value of the "Cause" fact, as the web prints it.
    fn name(self) -> &'static str {
        match self {
            Self::TunnelDown => "tunnel_down",
            Self::OriginUnreachable => "origin_unreachable",
            Self::Dns => "dns",
            Self::Tls => "tls",
            Self::Refused => "refused",
            Self::Timeout => "timeout",
            Self::InvalidResponse => "invalid_response",
            Self::Unknown => "unknown",
            Self::ClockSkew => "clock_skew",
            Self::NotLinked => "not_linked",
            Self::RelayError => "relay_error",
            Self::TunnelPending => "tunnel_pending",
            Self::TunnelNotEnabled => "tunnel_not_enabled",
            Self::ApprovalPending => "approval_pending",
            Self::Offline => "offline",
            Self::Outdated => "outdated",
            Self::Released => "released",
            Self::SessionPending => "session_pending",
        }
    }
}

/// `ConnectionTransientReason` and `ConnectionBlockedReason`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Network,
    Timeout,
    Transport,
    EndpointUnavailable,
    RelayUnavailable,
    RemoteUnavailable,
    SessionPending,
    ApprovalPending,
    Authentication,
    Configuration,
    Permission,
    Unsupported,
}

impl Reason {
    fn name(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Timeout => "timeout",
            Self::Transport => "transport",
            Self::EndpointUnavailable => "endpoint-unavailable",
            Self::RelayUnavailable => "relay-unavailable",
            Self::RemoteUnavailable => "remote-unavailable",
            Self::SessionPending => "session-pending",
            Self::ApprovalPending => "approval-pending",
            Self::Authentication => "authentication",
            Self::Configuration => "configuration",
            Self::Permission => "permission",
            Self::Unsupported => "unsupported",
        }
    }

    fn unreachable(self) -> bool {
        matches!(self, Self::Network | Self::Timeout | Self::Transport)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    Relay,
    Direct,
}

/// The last failed attempt (`ConnectionAttemptError`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub reason: Reason,
    pub detail: String,
    pub trace_id: Option<String>,
    pub verification_code: Option<String>,
    pub hop: Option<Hop>,
    pub via: Option<Via>,
    pub cause: Option<Cause>,
    pub http_status: Option<u16>,
    /// A blocked connection whose server must be updated.
    pub server_update_required: bool,
}

impl Failure {
    pub fn new(reason: Reason, detail: impl Into<String>) -> Self {
        Self {
            reason,
            detail: detail.into(),
            trace_id: None,
            verification_code: None,
            hop: None,
            via: None,
            cause: None,
            http_status: None,
            server_update_required: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Input {
    pub host: String,
    pub offline: bool,
    pub failure: Option<Failure>,
    pub attempt: u32,
    pub retry_at_ms: Option<u64>,
    pub via: Via,
    pub client_version: Option<String>,
    pub server_version: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HopResult {
    Ok,
    Failed,
    Unchecked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosisHop {
    pub label: String,
    pub result: HopResult,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnosis {
    pub headline: String,
    pub hops: Vec<DiagnosisHop>,
    pub steps: Vec<String>,
    pub facts: Vec<(String, String)>,
    pub more_info_url: Option<String>,
}

struct Located {
    hop: Hop,
    cause: Option<Cause>,
    guessed: bool,
}

fn located(hop: Hop, cause: Option<Cause>) -> Located {
    Located {
        hop,
        cause,
        guessed: false,
    }
}

fn locate(input: &Input) -> Option<Located> {
    let failure = input.failure.as_ref();
    if input.offline {
        return Some(located(Hop::Device, Some(Cause::Offline)));
    }
    if failure.is_some_and(|f| f.server_update_required) {
        return Some(located(Hop::Server, Some(Cause::Outdated)));
    }
    if failure.is_some_and(|f| f.reason == Reason::SessionPending) {
        return Some(located(Hop::Device, Some(Cause::SessionPending)));
    }
    if let Some(hop) = failure.and_then(|f| f.hop) {
        return Some(located(hop, failure.and_then(|f| f.cause)));
    }
    let failure = failure?;
    let via = failure.via.unwrap_or(input.via);
    Some(Located {
        hop: if failure.reason.unreachable() && via == Via::Relay {
            Hop::Tunnel
        } else {
            Hop::Server
        },
        cause: None,
        guessed: true,
    })
}

fn headline(host: &str, at: &Located, failure: Option<&Failure>) -> String {
    match at.cause {
        Some(Cause::Offline) => return "This device is offline".into(),
        Some(Cause::SessionPending) => {
            return "This device is still loading its T3 Connect sign-in".into();
        }
        Some(Cause::ApprovalPending) => {
            return format!("{host} is waiting for someone to approve this device");
        }
        Some(Cause::Outdated) => return format!("{host} runs an older T3 Code"),
        Some(Cause::ClockSkew) => return "This device's clock is off".into(),
        Some(Cause::NotLinked) => {
            return format!("{host} is not linked to your T3 Connect account");
        }
        Some(Cause::RelayError) => return "T3 Connect returned an error".into(),
        Some(Cause::TunnelPending) => {
            return format!("{host}'s tunnel is still being set up");
        }
        Some(Cause::TunnelNotEnabled) => {
            return format!("T3 Connect is not turned on for {host}");
        }
        Some(Cause::Released | Cause::TunnelDown) => {
            return format!("{host}'s tunnel is offline");
        }
        Some(Cause::OriginUnreachable) => {
            return format!("Tunnel works, but T3 Code on {host} did not answer");
        }
        Some(Cause::InvalidResponse) => {
            return format!("T3 Code on {host} sent an unexpected answer");
        }
        Some(Cause::Dns) => return format!("{host}'s tunnel address did not resolve"),
        Some(Cause::Tls) => return format!("Secure connection to {host}'s tunnel failed"),
        Some(Cause::Refused) => return format!("{host}'s tunnel refused the connection"),
        Some(Cause::Timeout) => return format!("{host} did not answer in time"),
        Some(Cause::Unknown) | None => {}
    }
    let reason = failure.map(|f| f.reason);
    match at.hop {
        Hop::Device => "This device could not prepare the connection".into(),
        Hop::Relay => if reason == Some(Reason::Authentication) {
            "T3 Connect did not accept this device's sign-in"
        } else {
            "This device can't reach T3 Connect"
        }
        .into(),
        Hop::Tunnel => if reason.is_some_and(Reason::unreachable) {
            format!("This device can't reach {host}'s tunnel")
        } else {
            format!("T3 Connect could not reach {host}'s tunnel")
        },
        Hop::Server => {
            if matches!(reason, Some(Reason::Authentication | Reason::Permission)) {
                format!("T3 Code on {host} did not accept this device")
            } else {
                format!("T3 Code on {host} did not answer")
            }
        }
    }
}

fn steps(host: &str, at: &Located, failure: Option<&Failure>) -> Vec<String> {
    let check_host =
        format!("On {host}, check Settings > Sharing & access or run `t3 connect status`.");
    let turn_on = format!("On {host}, open Settings > Sharing & access and turn T3 Connect on.");
    let awake = format!("Make sure {host} is awake and online.");
    match at.cause {
        Some(Cause::Offline) => {
            return vec!["Connect this device to the internet. T3 Code reconnects on its own.".into()];
        }
        Some(Cause::SessionPending) => {
            return vec![
                "Wait a moment. T3 Code retries on its own.".into(),
                "If it keeps waiting, reload or restart T3 Code on this device.".into(),
            ];
        }
        Some(Cause::ApprovalPending) => {
            let code = failure.and_then(|f| f.verification_code.as_deref());
            let approve = match code {
                Some(code) if !code.is_empty() => format!("approve the device showing {code}"),
                _ => "approve this device".into(),
            };
            return vec![
                format!(
                    "On a device already connected to {host}, open Settings > Sharing & access and {approve}."
                ),
                format!(
                    "Or run `t3 auth connect-device approve {}` on {host}.",
                    code.unwrap_or("<code>")
                ),
            ];
        }
        Some(Cause::Outdated) => return vec![format!("Update T3 Code on {host}.")],
        Some(Cause::ClockSkew) => {
            return vec![format!(
                "Turn on automatic date and time on this device and on {host}, then retry."
            )];
        }
        Some(Cause::NotLinked) => {
            return vec![
                "Check that this device is signed in to the same T3 Connect account as the host."
                    .into(),
                turn_on,
            ];
        }
        Some(Cause::TunnelPending) => {
            return vec!["Wait a minute. T3 Code retries on its own.".into(), check_host];
        }
        Some(Cause::RelayError) => {
            return vec![
                "Wait a few minutes. T3 Code retries on its own.".into(),
                "If it keeps failing, copy the diagnostics and report it.".into(),
            ];
        }
        Some(Cause::TunnelNotEnabled) => return vec![turn_on],
        Some(Cause::Released | Cause::TunnelDown) => {
            return vec![
                format!("Make sure {host} is awake and T3 Code is running on it."),
                check_host,
                format!(
                    "A VPN or Tailscale exit node on {host} can block the tunnel (port 7844). Turn it off or allow the tunnel."
                ),
            ];
        }
        Some(Cause::OriginUnreachable | Cause::InvalidResponse) => {
            return vec![format!("Open or restart T3 Code on {host}."), check_host];
        }
        Some(Cause::Dns | Cause::Tls | Cause::Refused | Cause::Timeout | Cause::Unknown) => {
            return vec![awake, check_host];
        }
        None => {}
    }
    let reason = failure.map(|f| f.reason);
    match at.hop {
        Hop::Device => vec![
            "Restart T3 Code on this device.".into(),
            format!("If it keeps failing, remove {host} from this device and add it again."),
        ],
        Hop::Relay => {
            if reason == Some(Reason::Authentication) {
                vec!["Sign out of T3 Connect and sign in again.".into()]
            } else {
                vec![
                    "Check this device's internet connection.".into(),
                    "A VPN, firewall, or ad blocker on this device can block T3 Connect.".into(),
                ]
            }
        }
        Hop::Tunnel => {
            if reason.is_some_and(Reason::unreachable) {
                vec![
                    "A VPN, firewall, or DNS filter on this device can block the tunnel.".into(),
                    "Try another network, or add a direct route such as a Tailscale address."
                        .into(),
                ]
            } else {
                vec![awake, check_host]
            }
        }
        Hop::Server => vec![
            format!("Make sure T3 Code is running on {host}."),
            "Retry. If it keeps failing, copy the diagnostics.".into(),
        ],
    }
}

fn help_section(at: &Located, failure: Option<&Failure>) -> &'static str {
    const THIS_DEVICE: &str = "this-device-cant-reach-t3-connect";
    const SIGN_IN: &str = "sign-in-account-or-link-problems";
    const TUNNEL_OFFLINE: &str = "the-hosts-tunnel-is-offline";
    const NO_ANSWER: &str = "the-tunnel-works-but-t3-code-does-not-answer";
    match at.cause {
        Some(Cause::Offline) => return THIS_DEVICE,
        Some(Cause::Outdated) => return "the-host-runs-an-older-t3-code",
        Some(Cause::ClockSkew) => return "the-clock-is-off",
        Some(Cause::SessionPending | Cause::ApprovalPending | Cause::NotLinked)
        | Some(Cause::TunnelNotEnabled) => return SIGN_IN,
        Some(Cause::RelayError) => return "t3-connect-returned-an-error",
        Some(Cause::Released | Cause::TunnelDown | Cause::TunnelPending) => return TUNNEL_OFFLINE,
        Some(Cause::OriginUnreachable | Cause::InvalidResponse) => return NO_ANSWER,
        Some(Cause::Dns | Cause::Tls | Cause::Refused | Cause::Timeout | Cause::Unknown) => {
            return "the-t3-connect-address-does-not-resolve-or-answer";
        }
        None => {}
    }
    let reason = failure.map(|f| f.reason);
    match at.hop {
        Hop::Device => THIS_DEVICE,
        Hop::Relay => {
            if reason == Some(Reason::Authentication) {
                SIGN_IN
            } else {
                THIS_DEVICE
            }
        }
        Hop::Tunnel => {
            if reason.is_some_and(Reason::unreachable) {
                THIS_DEVICE
            } else {
                TUNNEL_OFFLINE
            }
        }
        Hop::Server => {
            if matches!(reason, Some(Reason::Authentication | Reason::Permission)) {
                SIGN_IN
            } else {
                NO_ANSWER
            }
        }
    }
}

fn hop_label(hop: Hop, host: &str) -> String {
    match hop {
        Hop::Device => "This device".into(),
        Hop::Relay => "T3 Connect".into(),
        Hop::Tunnel => format!("{host} tunnel"),
        Hop::Server => format!("T3 Code on {host}"),
    }
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ` of a Unix time in milliseconds (`DateTime.formatIso`).
pub fn iso(ms: u64) -> String {
    let days = (ms / 86_400_000) as i64;
    let rest = ms % 86_400_000;
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3_600_000,
        rest / 60_000 % 60,
        rest / 1000 % 60,
        rest % 1000
    )
}

/// Null when nothing has failed.
pub fn present(input: &Input) -> Option<Diagnosis> {
    let at = locate(input)?;
    let host = input.host.as_str();
    let failure = input.failure.as_ref();
    let relay_path = at.hop == Hop::Relay
        || at.hop == Hop::Tunnel
        || failure.and_then(|f| f.via).unwrap_or(input.via) == Via::Relay;
    let order: &[Hop] = if relay_path {
        &[Hop::Device, Hop::Relay, Hop::Tunnel, Hop::Server]
    } else {
        &[Hop::Device, Hop::Server]
    };
    let failed = order.iter().position(|hop| *hop == at.hop);
    let hops = order
        .iter()
        .enumerate()
        .map(|(index, hop)| DiagnosisHop {
            label: hop_label(*hop, host),
            result: if Some(index) == failed {
                HopResult::Failed
            } else if failed.is_some_and(|failed| index < failed) && !at.guessed {
                HopResult::Ok
            } else {
                HopResult::Unchecked
            },
        })
        .collect();

    let mut facts: Vec<(String, String)> = Vec::new();
    let mut add = |label: &str, value: Option<String>| {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            facts.push((label.to_owned(), value));
        }
    };
    add(
        "Cause",
        at.cause
            .map(|cause| cause.name().to_owned())
            .or_else(|| failure.map(|f| f.reason.name().to_owned())),
    );
    add(
        "HTTP status",
        failure.and_then(|f| f.http_status).map(|s| s.to_string()),
    );
    add(
        "Attempts",
        (input.attempt > 0).then(|| input.attempt.to_string()),
    );
    add("Next retry", input.retry_at_ms.map(iso));
    add(
        "Route",
        Some(if relay_path { "T3 Connect" } else { "Direct" }.into()),
    );
    add("App version", input.client_version.clone());
    add("Host version", input.server_version.clone());
    add("Error", failure.map(|f| f.detail.clone()));
    add("Trace ID", failure.and_then(|f| f.trace_id.clone()));

    Some(Diagnosis {
        headline: headline(host, &at, failure),
        hops,
        steps: steps(host, &at, failure),
        facts,
        more_info_url: (relay_path || at.cause == Some(Cause::SessionPending)).then(|| {
            format!(
                "https://github.com/pingdotgg/t3code/blob/main/docs/user/t3-connect-troubleshooting.md#{}",
                help_section(&at, failure)
            )
        }),
    })
}

/// One plain-text block for bug reports (`connectionDiagnosisText`).
pub fn text(host: &str, diagnosis: &Diagnosis) -> String {
    let mut lines = vec![
        format!("T3 Code connection to {host}: {}", diagnosis.headline),
        String::new(),
    ];
    for hop in &diagnosis.hops {
        let mark = match hop.result {
            HopResult::Ok => "ok",
            HopResult::Failed => "FAILED",
            HopResult::Unchecked => "not checked",
        };
        lines.push(format!("{}: {mark}", hop.label));
    }
    lines.push(String::new());
    for (label, value) in &diagnosis.facts {
        lines.push(format!("{label}: {value}"));
    }
    if let Some(url) = &diagnosis.more_info_url {
        lines.push(String::new());
        lines.push(format!("More info: {url}"));
    }
    lines.join("\n")
}

/// The summary's retry clause: `retrying in Ns` (rounded up) until the deadline, then
/// `retrying`.
pub fn retry_text(retry_at_ms: Option<u64>, now_ms: u64) -> Option<String> {
    let at = retry_at_ms?;
    Some(if at > now_ms {
        format!("retrying in {}s", (at - now_ms).div_ceil(1000))
    } else {
        "retrying".into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(failure: Option<Failure>) -> Input {
        Input {
            host: "devbox".into(),
            offline: false,
            failure,
            attempt: 141,
            retry_at_ms: Some(1_700_000_000_123),
            via: Via::Direct,
            client_version: Some("0.1.0".into()),
            server_version: None,
        }
    }

    #[test]
    fn nothing_failed_is_no_diagnosis() {
        assert_eq!(present(&input(None)), None);
    }

    #[test]
    fn a_direct_network_failure_blames_the_server_with_the_device_checked() {
        let mut failure = Failure::new(Reason::Network, "connect refused");
        failure.hop = Some(Hop::Server);
        let diagnosis = present(&input(Some(failure))).unwrap();
        assert_eq!(diagnosis.headline, "T3 Code on devbox did not answer");
        assert_eq!(
            diagnosis.steps,
            [
                "Make sure T3 Code is running on devbox.",
                "Retry. If it keeps failing, copy the diagnostics."
            ]
        );
        let results: Vec<_> = diagnosis.hops.iter().map(|h| h.result).collect();
        assert_eq!(results, [HopResult::Ok, HopResult::Failed]);
        assert_eq!(diagnosis.more_info_url, None);
        let labels: Vec<_> = diagnosis.facts.iter().map(|f| f.0.as_str()).collect();
        assert_eq!(
            labels,
            ["Cause", "Attempts", "Next retry", "Route", "App version", "Error"]
        );
        assert_eq!(diagnosis.facts[2].1, "2023-11-14T22:13:20.123Z");
    }

    #[test]
    fn a_guessed_hop_leaves_the_earlier_hops_unchecked() {
        let diagnosis = present(&input(Some(Failure::new(Reason::Network, "x")))).unwrap();
        let results: Vec<_> = diagnosis.hops.iter().map(|h| h.result).collect();
        assert_eq!(results, [HopResult::Unchecked, HopResult::Failed]);
    }

    #[test]
    fn an_offline_device_wins() {
        let mut offline = input(Some(Failure::new(Reason::Network, "x")));
        offline.offline = true;
        let diagnosis = present(&offline).unwrap();
        assert_eq!(diagnosis.headline, "This device is offline");
        assert_eq!(diagnosis.facts[0], ("Cause".into(), "offline".into()));
    }

    #[test]
    fn a_relay_tunnel_failure_lists_four_hops_and_links_help() {
        let mut offline = input(Some(Failure::new(Reason::Timeout, "x")));
        offline.via = Via::Relay;
        let diagnosis = present(&offline).unwrap();
        assert_eq!(diagnosis.headline, "This device can't reach devbox's tunnel");
        assert_eq!(diagnosis.hops.len(), 4);
        assert!(
            diagnosis
                .more_info_url
                .unwrap()
                .ends_with("#this-device-cant-reach-t3-connect")
        );
    }

    #[test]
    fn approval_names_the_code() {
        let mut failure = Failure::new(Reason::ApprovalPending, "x");
        failure.hop = Some(Hop::Relay);
        failure.cause = Some(Cause::ApprovalPending);
        failure.verification_code = Some("AB12".into());
        let diagnosis = present(&input(Some(failure))).unwrap();
        assert!(diagnosis.steps[0].ends_with("approve the device showing AB12."));
        assert!(diagnosis.steps[1].contains("approve AB12"));
    }

    #[test]
    fn text_matches_the_web_block() {
        let mut failure = Failure::new(Reason::Network, "boom");
        failure.hop = Some(Hop::Server);
        let diagnosis = present(&input(Some(failure))).unwrap();
        let text = text("devbox", &diagnosis);
        assert!(text.starts_with(
            "T3 Code connection to devbox: T3 Code on devbox did not answer\n\nThis device: ok\nT3 Code on devbox: FAILED\n\nCause: network"
        ));
    }

    #[test]
    fn retry_counts_down_then_says_retrying() {
        assert_eq!(retry_text(Some(10_500), 10_000).as_deref(), Some("retrying in 1s"));
        assert_eq!(retry_text(Some(12_001), 10_000).as_deref(), Some("retrying in 3s"));
        assert_eq!(retry_text(Some(10_000), 10_000).as_deref(), Some("retrying"));
        assert_eq!(retry_text(None, 10_000), None);
    }
}
