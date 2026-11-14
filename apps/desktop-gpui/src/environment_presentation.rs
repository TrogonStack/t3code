//! How Settings > Environments words an environment: the status line, the transport label, the
//! routes list and the MCP URL (SavedEnvironmentRows.tsx, EnvironmentRow.tsx,
//! EnvironmentRoutesList.tsx, packages/client-runtime `connection/routes.ts`).

use crate::connection_diagnosis::Via;
use crate::environment_prefs::Sharing;
use crate::environment_state::{EnvState, Phase};
use std::net::{Ipv4Addr, Ipv6Addr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // `Relay` is named by `route_label` for T3 Connect hosts
pub enum RouteKind {
    Loopback,
    Lan,
    Tailnet,
    Vpn,
    Public,
    Relay,
}

fn normalize(host: &str) -> String {
    host.to_lowercase()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_owned()
}

fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host == "::1"
        || host
            .parse::<Ipv4Addr>()
            .is_ok_and(|ip| ip.octets()[0] == 127)
}

fn is_tailnet(host: &str) -> bool {
    host.ends_with(".ts.net")
        || host
            .parse::<Ipv6Addr>()
            .is_ok_and(|ip| ip.segments()[..3] == [0xfd7a, 0x115c, 0xa1e0])
}

fn is_shared_space(host: &str) -> bool {
    host.parse::<Ipv4Addr>()
        .is_ok_and(|ip| ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1]))
}

fn private_v4(o: [u8; 4]) -> bool {
    o[0] == 0
        || o[0] == 10
        || o[0] == 127
        || (o[0] == 100 && (64..=127).contains(&o[1]))
        || (o[0] == 172 && (16..=31).contains(&o[1]))
        || (o[0] == 192 && o[1] == 168)
        || (o[0] == 169 && o[1] == 254)
        || (o[0] == 198 && (18..=19).contains(&o[1]))
}

fn is_private(host: &str) -> bool {
    if host == "::"
        || is_loopback(host)
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host == "home.arpa"
        || host.ends_with(".home.arpa")
        || (!host.contains('.') && !host.contains(':'))
        || host.ends_with(".ts.net")
    {
        return true;
    }
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return private_v4(ip.octets());
    }
    if let Ok(ip) = host.parse::<Ipv6Addr>() {
        if let Some(mapped) = ip.to_ipv4_mapped() {
            return private_v4(mapped.octets());
        }
        let first = ip.segments()[0];
        return first & 0xfe00 == 0xfc00 || first & 0xffc0 == 0xfe80;
    }
    false
}

/// `directAddressKind`.
pub fn classify_host(host: &str) -> RouteKind {
    let host = normalize(host);
    if is_loopback(&host) {
        RouteKind::Loopback
    } else if is_tailnet(&host) {
        RouteKind::Tailnet
    } else if is_shared_space(&host) {
        RouteKind::Vpn
    } else if is_private(&host) {
        RouteKind::Lan
    } else {
        RouteKind::Public
    }
}

fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()?
        .host_str()
        .map(|host| host.to_owned())
}

/// `connectionRouteLabel`.
pub fn route_label(kind: RouteKind, url: &str) -> String {
    match kind {
        RouteKind::Relay => "T3 Connect".into(),
        RouteKind::Loopback => "This device".into(),
        RouteKind::Lan => "LAN".into(),
        RouteKind::Tailnet => "Tailscale".into(),
        RouteKind::Vpn => "VPN".into(),
        RouteKind::Public => host_of(url).unwrap_or_else(|| "Remote link".into()),
    }
}

/// The address the app shows for an http base (`displayUrl`): host and port, no scheme.
pub fn display_url(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    Some(match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteRow {
    pub label: String,
    pub address: String,
    pub in_use: bool,
    pub relay: bool,
}

/// The routes this app can reach an environment by: its saved pairing first, then T3 Connect.
pub fn routes(state: &EnvState) -> Vec<RouteRow> {
    let connected = state.phase == Phase::Connected;
    let mut rows = Vec::new();
    if state.saved && !state.url.is_empty() {
        // `state.url` follows the live route, so a relay session would name the relay base.
        let direct =
            if state.via == Some(Via::Relay) && state.relay_url.as_deref() == Some(&state.url) {
                None
            } else {
                Some(state.url.clone())
            };
        if let Some(url) = direct {
            let kind = host_of(&url).map_or(RouteKind::Public, |host| classify_host(&host));
            rows.push(RouteRow {
                label: route_label(kind, &url),
                address: url,
                in_use: connected && state.via == Some(Via::Direct),
                relay: false,
            });
        }
    }
    if let Some(url) = &state.relay_url {
        rows.push(RouteRow {
            label: "T3 Connect".into(),
            address: url.clone(),
            in_use: connected && state.via == Some(Via::Relay),
            relay: true,
        });
    }
    rows
}

pub fn routes_button_label(count: usize) -> String {
    if count == 1 {
        "Routes".into()
    } else {
        format!("{count} routes")
    }
}

/// `environmentTransportLabel`.
pub fn transport_label(state: &EnvState) -> String {
    let routes = routes(state);
    if routes.len() > 1 {
        let active = routes.iter().find(|route| route.in_use);
        return match (state.phase == Phase::Connected, active) {
            (true, Some(route)) => format!("via {}", route.label),
            _ => routes[0].label.clone(),
        };
    }
    if state.relay_listed && !state.saved {
        return "T3 Connect".into();
    }
    display_url(&state.url).unwrap_or_else(|| "Remote link".into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Muted,
    Error,
}

/// `savedBackendStatus`'s text and tone.
pub fn status(state: &EnvState, enabled: bool) -> (String, Tone) {
    if !enabled {
        return ("Off".into(), Tone::Muted);
    }
    let error = state.failure.as_ref().map(|failure| failure.detail.clone());
    match state.phase {
        Phase::Connected => ("Connected".into(), Tone::Muted),
        Phase::Connecting => ("Connecting".into(), Tone::Muted),
        Phase::Reconnecting => {
            let waiting = state.failure.as_ref().is_some_and(|failure| {
                failure.reason == crate::connection_diagnosis::Reason::ApprovalPending
            });
            if waiting {
                (
                    error.unwrap_or_else(|| "Waiting for approval".into()),
                    Tone::Muted,
                )
            } else {
                (
                    match error {
                        Some(error) => format!("Reconnecting: {error}"),
                        None => "Reconnecting".into(),
                    },
                    Tone::Error,
                )
            }
        }
        Phase::Unsupported => ("Client not supported".into(), Tone::Error),
        Phase::Error => (
            match error {
                Some(error) => format!("Connection failed: {error}"),
                None => "Connection failed".into(),
            },
            Tone::Error,
        ),
        Phase::Offline => ("Offline".into(), Tone::Muted),
        Phase::Available => ("Not connected".into(), Tone::Muted),
    }
}

/// The row's subtitle: transport, then the status unless a diagnosis speaks for it, then the host
/// version when it differs.
pub fn subtitle(state: &EnvState, enabled: bool, has_diagnosis: bool) -> String {
    let status = status(state, enabled).0;
    [
        Some(transport_label(state)),
        (!has_diagnosis).then_some(status),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" \u{b7} ")
}

/// `environmentMcpUrl`: the relay base's `/mcp` if known, else the route's.
pub fn mcp_url(state: &EnvState) -> Option<String> {
    let base = state
        .relay_url
        .as_deref()
        .or((!state.url.is_empty()).then_some(state.url.as_str()))?;
    let mut url = url::Url::parse(base).ok()?;
    url.set_path("/mcp");
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string())
}

/// `summarizePullRequestRouting`.
pub fn summarize_sharing(entries: &[(String, Sharing)]) -> Option<String> {
    let groups: Vec<String> = [
        (Sharing::ReadWrite, "read and act"),
        (Sharing::Read, "read PRs"),
    ]
    .into_iter()
    .filter_map(|(sharing, text)| {
        let labels: Vec<&str> = entries
            .iter()
            .filter(|(_, s)| *s == sharing)
            .map(|(label, _)| label.as_str())
            .collect();
        (!labels.is_empty()).then(|| format!("{} {text}", labels.join(", ")))
    })
    .collect();
    (!groups.is_empty()).then(|| groups.join(" \u{b7} "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection_diagnosis::{Failure, Reason};

    fn state(url: &str) -> EnvState {
        let mut state = EnvState::new("e".into(), "Mini".into());
        state.url = url.into();
        state.saved = true;
        state
    }

    #[test]
    fn hosts_classify_like_the_web() {
        assert_eq!(classify_host("localhost"), RouteKind::Loopback);
        assert_eq!(classify_host("127.0.0.2"), RouteKind::Loopback);
        assert_eq!(classify_host("192.168.1.20"), RouteKind::Lan);
        assert_eq!(classify_host("mini.local"), RouteKind::Lan);
        assert_eq!(classify_host("box.tail1.ts.net"), RouteKind::Tailnet);
        assert_eq!(classify_host("100.100.1.1"), RouteKind::Vpn);
        assert_eq!(classify_host("devbox.example.com"), RouteKind::Public);
        assert_eq!(classify_host("[fd7a:115c:a1e0::1]"), RouteKind::Tailnet);
    }

    #[test]
    fn status_text_follows_the_phase() {
        let mut s = state("http://192.168.1.20:13774");
        s.phase = Phase::Reconnecting;
        s.failure = Some(Failure::new(Reason::Network, "no answer"));
        assert_eq!(
            status(&s, true),
            ("Reconnecting: no answer".into(), Tone::Error)
        );
        assert_eq!(status(&s, false), ("Off".into(), Tone::Muted));
        s.phase = Phase::Available;
        assert_eq!(status(&s, true).0, "Not connected");
        s.phase = Phase::Error;
        s.failure = None;
        assert_eq!(status(&s, true).0, "Connection failed");
    }

    #[test]
    fn the_subtitle_hides_the_status_behind_a_diagnosis() {
        let mut s = state("http://192.168.1.20:13774");
        s.phase = Phase::Connected;
        s.via = Some(Via::Direct);
        assert_eq!(
            subtitle(&s, true, false),
            "192.168.1.20:13774 \u{b7} Connected"
        );
        assert_eq!(subtitle(&s, true, true), "192.168.1.20:13774");
    }

    #[test]
    fn two_routes_say_which_one_is_in_use() {
        let mut s = state("http://192.168.1.20:13774");
        s.relay_listed = true;
        s.relay_url = Some("https://relay.example/env".into());
        assert_eq!(transport_label(&s), "LAN");
        s.phase = Phase::Connected;
        s.via = Some(Via::Relay);
        assert_eq!(routes(&s).len(), 2);
        assert_eq!(routes_button_label(2), "2 routes");
        assert_eq!(routes_button_label(1), "Routes");
        s.via = Some(Via::Direct);
        assert_eq!(transport_label(&s), "via LAN");
    }

    #[test]
    fn a_relay_only_environment_is_t3_connect() {
        let mut s = EnvState::new("e".into(), "Mini".into());
        s.relay_listed = true;
        s.relay_url = Some("https://relay.example/env?x=1#y".into());
        assert_eq!(transport_label(&s), "T3 Connect");
        assert_eq!(mcp_url(&s).as_deref(), Some("https://relay.example/mcp"));
    }

    #[test]
    fn the_mcp_url_takes_the_direct_base_without_a_query() {
        let s = state("http://192.168.1.20:13774/?a=1");
        assert_eq!(
            mcp_url(&s).as_deref(),
            Some("http://192.168.1.20:13774/mcp")
        );
    }

    #[test]
    fn sharing_summary_groups_read_and_act_first() {
        let entries = [
            ("A".to_owned(), Sharing::Read),
            ("B".to_owned(), Sharing::ReadWrite),
            ("C".to_owned(), Sharing::Read),
            ("D".to_owned(), Sharing::Off),
        ];
        assert_eq!(
            summarize_sharing(&entries).as_deref(),
            Some("B read and act \u{b7} A, C read PRs")
        );
        assert_eq!(summarize_sharing(&[("D".into(), Sharing::Off)]), None);
    }
}
