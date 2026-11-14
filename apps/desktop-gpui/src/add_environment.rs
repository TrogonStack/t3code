//! Add Environment (AddEnvironmentDialog.tsx): reading what the user types or pastes, and pairing.
//! The parsing mirrors `parsePairingUrlFields`, `parseRemotePairingFields`,
//! `parseManualDesktopSshTarget` and `filterDiscoveredSshHosts`
//! (apps/web/src/components/settings/ConnectionsSettings.logic.ts) and
//! `resolveRemotePairingTarget` (packages/shared/src/remote.ts).

use crate::auth::{self, AuthError, Credential};
use crate::credentials::{CredentialStore, StoredCredential};
use std::sync::Arc;
use url::Url;

fn pairing_token(url: &Url) -> Option<String> {
    let from_hash = url
        .fragment()
        .and_then(|fragment| {
            url::form_urlencoded::parse(fragment.as_bytes())
                .find(|(key, _)| key == "token")
                .map(|(_, value)| value.trim().to_owned())
        })
        .filter(|token| !token.is_empty());
    from_hash.or_else(|| {
        url.query_pairs()
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.trim().to_owned())
            .filter(|token| !token.is_empty())
    })
}

/// A full pairing URL, or a hosted `/pair?host=...#token=...` link, into host and code. `None`
/// when the text is not one.
pub fn parse_pairing_url_fields(input: &str) -> Option<(String, String)> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let has_scheme = trimmed
        .split_once("://")
        .is_some_and(|(scheme, _)| {
            scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
        });
    let like = if has_scheme {
        trimmed.to_owned()
    } else if trimmed.starts_with("//") {
        format!("https:{trimmed}")
    } else {
        format!("https://{trimmed}")
    };
    let url = Url::parse(&like).ok()?;
    let hosted_host = url
        .query_pairs()
        .find(|(key, _)| key == "host")
        .map(|(_, value)| value.trim().to_owned())
        .filter(|host| !host.is_empty());
    let token = pairing_token(&url)?;
    if let Some(host) = hosted_host {
        return Some((host, token));
    }
    Some((url.origin().ascii_serialization(), token))
}

/// What Add environment pairs with: a pasted pairing URL wins over the two fields.
pub fn parse_remote_fields(host: &str, code: &str) -> Result<(String, String), String> {
    if let Some(parsed) = parse_pairing_url_fields(host) {
        return Ok(parsed);
    }
    let (host, code) = (host.trim(), code.trim());
    if host.is_empty() {
        return Err("Enter a backend host.".into());
    }
    if code.is_empty() {
        return Err("Enter a pairing code.".into());
    }
    Ok((host.to_owned(), code.to_owned()))
}

/// `normalizeRemoteBaseUrl` then `toHttpBaseUrl`, without the trailing slash this app's http
/// bases omit.
pub fn http_base(host: &str) -> Result<String, String> {
    let trimmed = host.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        return Err("Enter a backend URL.".into());
    }
    let has_scheme = trimmed.split_once("://").is_some_and(|(scheme, _)| {
        scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-')
    });
    let normalized = if has_scheme {
        trimmed.to_owned()
    } else {
        format!("https://{trimmed}")
    };
    let invalid = || "Backend URL is invalid.".to_owned();
    let mut url = Url::parse(&normalized).map_err(|_| invalid())?;
    match url.scheme() {
        "http" | "https" => {}
        "ws" => url.set_scheme("http").map_err(|()| invalid())?,
        "wss" => url.set_scheme("https").map_err(|()| invalid())?,
        _ => return Err(invalid()),
    }
    url.host_str().ok_or_else(invalid)?;
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

/// Pairs with a remote environment and saves the pairing, as `connectPairing` does.
pub async fn pair_remote(
    store: Arc<dyn CredentialStore>,
    host: String,
    code: String,
) -> Result<StoredCredential, String> {
    let http_base = http_base(&host)?;
    let client = auth::http_client();
    let environment = crate::server::fetch_environment(&client, &http_base)
        .await
        .map_err(|message| format!("Could not reach {http_base}: {message}"))?;
    let session = auth::exchange_session(
        &client,
        &http_base,
        &Credential::Pairing(auth::pairing_token(&code)),
    )
    .await
    .map_err(|error| match error {
        AuthError::Rejected(_) => {
            "The pairing code was rejected. It works only once and expires.".to_owned()
        }
        error => format!("Pairing failed: {error}"),
    })?;
    let now = auth::now_ms();
    let credential = StoredCredential {
        environment_id: environment.id,
        label: environment.label,
        http_base,
        token: session.token,
        saved_at_ms: now,
        expires_at_ms: session.expires_in_secs.map(|secs| now + secs * 1000),
    };
    let to_save = credential.clone();
    tokio::task::spawn_blocking(move || store.save(&to_save))
        .await
        .map_err(|error| error.to_string())??;
    Ok(credential)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshTarget {
    pub alias: String,
    pub hostname: String,
    pub username: Option<String>,
    pub port: Option<u16>,
}

/// `parseManualDesktopSshTarget`.
pub fn parse_ssh_target(host: &str, username: &str, port: &str) -> Result<SshTarget, String> {
    let raw = host.trim();
    if raw.is_empty() {
        return Err("SSH host or alias is required.".into());
    }
    let mut hostname = raw.to_owned();
    let mut user = Some(username.trim().to_owned()).filter(|u| !u.is_empty());
    let mut parsed_port: Option<i64> = None;
    if let Some(at) = hostname.rfind('@').filter(|at| *at > 0) {
        let inline = hostname[..at].trim().to_owned();
        hostname = hostname[at + 1..].trim().to_owned();
        if user.is_none() && !inline.is_empty() {
            user = Some(inline);
        }
    }
    let bracketed = hostname.strip_prefix('[').and_then(|rest| {
        let (inside, tail) = rest.split_once(']')?;
        match tail {
            "" => Some((inside.to_owned(), None)),
            _ => {
                let digits = tail.strip_prefix(':')?;
                (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
                    .then(|| (inside.to_owned(), digits.parse::<i64>().ok()))
            }
        }
    });
    if let Some((inside, bracket_port)) = bracketed {
        hostname = inside.trim().to_owned();
        parsed_port = bracket_port;
    } else {
        let segments: Vec<&str> = hostname.split(':').collect();
        if segments.len() == 2
            && !segments[1].is_empty()
            && segments[1].bytes().all(|b| b.is_ascii_digit())
        {
            parsed_port = segments[1].parse().ok().or(Some(i64::MAX));
            hostname = segments[0].trim().to_owned();
        }
    }
    let raw_port = port.trim();
    if !raw_port.is_empty() {
        parsed_port = Some(
            raw_port
                .bytes()
                .take_while(u8::is_ascii_digit)
                .fold(Some(0i64), |acc: Option<i64>, b| {
                    acc?.checked_mul(10)?.checked_add(i64::from(b - b'0'))
                })
                .filter(|_| raw_port.as_bytes()[0].is_ascii_digit())
                .unwrap_or(0),
        );
    }
    if hostname.is_empty() {
        return Err("SSH host or alias is required.".into());
    }
    let port = match parsed_port {
        Some(p) if (1..=65_535).contains(&p) => Some(p as u16),
        Some(_) => return Err("SSH port must be between 1 and 65535.".into()),
        None => None,
    };
    Ok(SshTarget {
        alias: hostname.clone(),
        hostname,
        username: user,
        port,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshHost {
    pub alias: String,
    pub hostname: String,
}

/// `filterDiscoveredSshHosts`: alias prefix matches first, then alias or hostname substrings.
pub fn filter_ssh_hosts(hosts: &[SshHost], query: &str) -> Vec<SshHost> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return hosts.to_vec();
    }
    let mut prefix = Vec::new();
    let mut substring = Vec::new();
    for host in hosts {
        let alias = host.alias.to_lowercase();
        if alias.starts_with(&query) {
            prefix.push(host.clone());
        } else if alias.contains(&query) || host.hostname.to_lowercase().contains(&query) {
            substring.push(host.clone());
        }
    }
    prefix.extend(substring);
    prefix
}

/// The hosts `~/.ssh/config` names (wildcards skipped, `HostName` as the hostname) and the ones
/// `known_hosts` has seen (hashed entries skipped).
pub fn parse_ssh_hosts(config: &str, known_hosts: &str) -> Vec<SshHost> {
    let mut hosts: Vec<SshHost> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    for line in config.lines() {
        let line = line.trim();
        let Some((key, value)) = line
            .split_once(char::is_whitespace)
            .or_else(|| line.split_once('='))
        else {
            continue;
        };
        let value = value.trim().trim_start_matches('=').trim();
        match key.to_lowercase().as_str() {
            "host" => {
                current.clear();
                for alias in value.split_whitespace() {
                    if alias.contains(['*', '?', '!']) || hosts.iter().any(|h| h.alias == alias) {
                        continue;
                    }
                    hosts.push(SshHost {
                        alias: alias.to_owned(),
                        hostname: alias.to_owned(),
                    });
                    current.push(hosts.len() - 1);
                }
            }
            "hostname" => {
                for index in &current {
                    hosts[*index].hostname = value.to_owned();
                }
            }
            _ => {}
        }
    }
    for line in known_hosts.lines() {
        let Some(first) = line.split_whitespace().next() else {
            continue;
        };
        if first.starts_with('|') || first.starts_with('#') || first.starts_with('@') {
            continue;
        }
        for entry in first.split(',') {
            let name = entry
                .strip_prefix('[')
                .and_then(|rest| rest.split_once(']'))
                .map_or(entry, |(host, _)| host);
            if name.is_empty() || name.contains(['*', '?', '!']) {
                continue;
            }
            if !hosts.iter().any(|h| h.alias == name) {
                hosts.push(SshHost {
                    alias: name.to_owned(),
                    hostname: name.to_owned(),
                });
            }
        }
    }
    hosts
}

/// Reads the user's SSH files; empty when they are missing.
pub fn discover_ssh_hosts() -> Vec<SshHost> {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return Vec::new();
    };
    let read = |name: &str| std::fs::read_to_string(home.join(".ssh").join(name)).unwrap_or_default();
    parse_ssh_hosts(&read("config"), &read("known_hosts"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pairing_url_fills_host_and_code() {
        assert_eq!(
            parse_pairing_url_fields("https://box.example.com:3773/pair#token=ABC123"),
            Some(("https://box.example.com:3773".into(), "ABC123".into()))
        );
        assert_eq!(
            parse_pairing_url_fields("box.example.com/pair?token=ZZ"),
            Some(("https://box.example.com".into(), "ZZ".into()))
        );
        assert_eq!(
            parse_pairing_url_fields("https://app.t3.codes/pair?host=https%3A%2F%2Fbox.example.com#token=T"),
            Some(("https://box.example.com".into(), "T".into()))
        );
        assert_eq!(parse_pairing_url_fields("backend.example.com"), None);
        assert_eq!(parse_pairing_url_fields("  "), None);
    }

    #[test]
    fn the_fields_need_a_host_and_a_code() {
        assert_eq!(
            parse_remote_fields(" ", "x"),
            Err("Enter a backend host.".into())
        );
        assert_eq!(
            parse_remote_fields("box", " "),
            Err("Enter a pairing code.".into())
        );
        assert_eq!(
            parse_remote_fields("box", " CODE "),
            Ok(("box".into(), "CODE".into()))
        );
    }

    #[test]
    fn hosts_become_http_bases() {
        assert_eq!(http_base("box.example.com").as_deref(), Ok("https://box.example.com"));
        assert_eq!(http_base("//box:3773/x?y#z").as_deref(), Ok("https://box:3773"));
        assert_eq!(http_base("ws://10.0.0.2:3773").as_deref(), Ok("http://10.0.0.2:3773"));
        assert_eq!(http_base("wss://box").as_deref(), Ok("https://box"));
        assert_eq!(http_base("ftp://box"), Err("Backend URL is invalid.".into()));
        assert_eq!(http_base(""), Err("Enter a backend URL.".into()));
    }

    #[test]
    fn ssh_targets_parse_inline_user_and_port() {
        let target = parse_ssh_target("root@devbox:2222", "", "").unwrap();
        assert_eq!(target.hostname, "devbox");
        assert_eq!(target.username.as_deref(), Some("root"));
        assert_eq!(target.port, Some(2222));
        let target = parse_ssh_target("[::1]:22", "me", "2200").unwrap();
        assert_eq!(target.hostname, "::1");
        assert_eq!(target.username.as_deref(), Some("me"));
        assert_eq!(target.port, Some(2200));
        assert_eq!(
            parse_ssh_target("", "", ""),
            Err("SSH host or alias is required.".into())
        );
        assert_eq!(
            parse_ssh_target("box", "", "70000"),
            Err("SSH port must be between 1 and 65535.".into())
        );
        assert_eq!(
            parse_ssh_target("box", "", "abc"),
            Err("SSH port must be between 1 and 65535.".into())
        );
    }

    #[test]
    fn discovery_reads_config_aliases_and_known_hosts() {
        let hosts = parse_ssh_hosts(
            "Host devbox staging\n  HostName 10.0.0.5\nHost *\n  User x\nHost wild-*\n",
            "|1|hashed ssh-rsa AAA\nmini.local,192.168.0.9 ssh-ed25519 AAA\n[nas.lan]:2222 ssh-rsa AAA\n",
        );
        let names: Vec<_> = hosts.iter().map(|h| h.alias.as_str()).collect();
        assert_eq!(names, ["devbox", "staging", "mini.local", "192.168.0.9", "nas.lan"]);
        assert_eq!(hosts[0].hostname, "10.0.0.5");
    }

    #[test]
    fn the_filter_puts_alias_prefixes_first() {
        let hosts = [
            SshHost { alias: "my-dev".into(), hostname: "a".into() },
            SshHost { alias: "dev".into(), hostname: "b".into() },
            SshHost { alias: "x".into(), hostname: "devhost".into() },
            SshHost { alias: "y".into(), hostname: "zzz".into() },
        ];
        let aliases: Vec<_> = filter_ssh_hosts(&hosts, " DEV ")
            .into_iter()
            .map(|h| h.alias)
            .collect();
        assert_eq!(aliases, ["dev", "my-dev", "x"]);
        assert_eq!(filter_ssh_hosts(&hosts, "").len(), 4);
    }
}
