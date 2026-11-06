//! The Electron desktop app's saved connections: `connection-catalog.json` holds the catalog
//! encrypted with Electron safeStorage (DesktopConnectionCatalogStore.ts). The plaintext schema is
//! `ConnectionCatalogDocument` in packages/client-runtime/src/platform/storageDocument.ts.
//! Read-only: this module never writes the catalog and never logs a decrypted value.
//!
//! TODO(gpui-credentials): HACK. Reads Electron's private IndexedDB and non-extractable DPoP key. Replace with GPUI's own pairing + Keychain before shipping.
//! (`remoteDpopTokens` are only usable with Electron's DPoP key; see `electron_dpop`.)

use crate::auth::{self, Credential};
use crate::electron_dpop;
use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use hmac::{Hmac, KeyInit, Mac};
use p256::ecdsa::SigningKey;
use serde::Deserialize;
use sha1::Sha1;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

/// Chromium os_crypt_mac: key = PBKDF2-HMAC-SHA1(password, "saltysalt", 1003) of 16 bytes,
/// IV = 16 spaces, ciphertext prefixed with "v10".
const SAFE_STORAGE_PREFIX: &[u8] = b"v10";
const SAFE_STORAGE_SALT: &[u8] = b"saltysalt";
const SAFE_STORAGE_ITERATIONS: u32 = 1003;

/// Keychain items Electron's safeStorage uses as (service, account). The service is named after
/// the app at launch: a dev launcher that runs the stock Electron binary stores `Electron`.
#[cfg(target_os = "macos")]
const KEYCHAIN_ITEMS: [(&str, &str); 2] = [
    ("Electron Safe Storage", "Electron Key"),
    ("T3 Code (Dev) Safe Storage", "T3 Code (Dev) Key"),
];

pub const CATALOG_FILE: &str = "connection-catalog.json";

/// A directly reachable environment: a plain `http` base URL and the bearer token saved for it.
pub struct SavedEnvironment {
    pub environment_id: String,
    pub label: String,
    pub http_base: String,
    pub credential: Credential,
}

/// What `load` found: environments to connect to, and the `(label, kind)` of those it cannot.
#[derive(Default)]
pub struct Catalog {
    pub environments: Vec<SavedEnvironment>,
    pub skipped: Vec<(String, &'static str)>,
    /// One-line explanations for environments that are skipped for a specific reason.
    pub notices: Vec<String>,
}

#[derive(Deserialize)]
struct Envelope {
    version: u32,
    #[serde(rename = "encryptedCatalog")]
    encrypted_catalog: String,
}

// Variant names are the wire `_tag` values.
#[allow(clippy::enum_variant_names)]
#[derive(Deserialize)]
#[serde(tag = "_tag")]
enum Target {
    BearerConnectionTarget {
        #[serde(rename = "environmentId")]
        environment_id: String,
        label: String,
        #[serde(rename = "connectionId")]
        connection_id: String,
    },
    RelayConnectionTarget {
        label: String,
    },
    SshConnectionTarget {
        label: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize)]
#[serde(tag = "_tag")]
enum Profile {
    BearerConnectionProfile {
        #[serde(rename = "connectionId")]
        connection_id: String,
        #[serde(rename = "httpBaseUrl")]
        http_base_url: String,
        authorization: Option<String>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "_tag")]
enum StoredToken {
    BearerConnectionCredential {
        token: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct StoredCredential {
    #[serde(rename = "connectionId")]
    connection_id: String,
    credential: StoredToken,
}

/// `RemoteDpopAccessToken` (packages/client-runtime/src/authorization/tokenStore.ts).
#[derive(Deserialize)]
struct DpopToken {
    #[serde(rename = "environmentId")]
    environment_id: String,
    endpoint: DpopEndpoint,
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "expiresAtEpochMs")]
    expires_at_epoch_ms: f64,
    #[serde(rename = "dpopThumbprint")]
    dpop_thumbprint: String,
}

#[derive(Deserialize)]
struct DpopEndpoint {
    #[serde(rename = "httpBaseUrl")]
    http_base_url: String,
}

#[derive(Deserialize)]
struct Document {
    #[serde(default)]
    targets: Vec<Target>,
    #[serde(default)]
    profiles: Vec<Profile>,
    #[serde(default)]
    credentials: Vec<StoredCredential>,
    #[serde(rename = "remoteDpopTokens", default)]
    remote_dpop_tokens: Vec<DpopToken>,
    #[serde(rename = "disabledEnvironmentIds", default)]
    disabled_environment_ids: Vec<String>,
}

/// Why a bearer target cannot be dialed directly, as a short kind for the log.
fn bearer_kind(http_base: &str, authorization: Option<&str>) -> Option<&'static str> {
    if authorization == Some("t3-connect") {
        return Some("t3-connect");
    }
    // tokio-tungstenite is built without TLS, so a wss socket cannot open.
    (!http_base.starts_with("http://")).then_some("https")
}

type KeyLoader<'a> = &'a mut dyn FnMut() -> Result<Arc<SigningKey>, String>;

/// Decodes the decrypted catalog JSON into the environments this client can reach.
/// `load_key` supplies Electron's DPoP key, and is called at most once, only when a saved
/// DPoP token is worth using.
pub fn parse(plaintext: &str, now_ms: u64, load_key: KeyLoader) -> Result<Catalog, String> {
    // serde messages can quote the offending value, and this document holds tokens.
    let document: Document = serde_json::from_str(plaintext).map_err(|error| {
        format!(
            "unreadable catalog document at {}:{}",
            error.line(),
            error.column()
        )
    })?;
    let profiles: HashMap<&str, (&str, Option<&str>)> = document
        .profiles
        .iter()
        .filter_map(|profile| match profile {
            Profile::BearerConnectionProfile {
                connection_id,
                http_base_url,
                authorization,
            } => Some((
                connection_id.as_str(),
                (http_base_url.as_str(), authorization.as_deref()),
            )),
            Profile::Other => None,
        })
        .collect();
    let tokens: HashMap<&str, &str> = document
        .credentials
        .iter()
        .filter_map(|stored| match &stored.credential {
            StoredToken::BearerConnectionCredential { token } => {
                Some((stored.connection_id.as_str(), token.as_str()))
            }
            StoredToken::Other => None,
        })
        .collect();
    let dpop_tokens: HashMap<&str, &DpopToken> = document
        .remote_dpop_tokens
        .iter()
        .map(|token| (token.environment_id.as_str(), token))
        .collect();
    let mut key: Option<Result<Arc<SigningKey>, String>> = None;
    let mut catalog = Catalog::default();
    for target in &document.targets {
        match target {
            Target::RelayConnectionTarget { label } => {
                catalog.skipped.push((label.clone(), "relay"));
            }
            Target::SshConnectionTarget { label } => catalog.skipped.push((label.clone(), "ssh")),
            Target::Unknown => {}
            Target::BearerConnectionTarget {
                environment_id,
                label,
                connection_id,
            } => {
                if document.disabled_environment_ids.contains(environment_id) {
                    continue;
                }
                let Some((http_base, authorization)) = profiles.get(connection_id.as_str()) else {
                    catalog.skipped.push((label.clone(), "no-profile"));
                    continue;
                };
                let bearer = tokens.get(connection_id.as_str());
                // Electron signs in with the DPoP token for T3 Connect profiles, and for any
                // profile that has no bearer of its own.
                let dpop = dpop_tokens
                    .get(environment_id.as_str())
                    .filter(|_| *authorization == Some("t3-connect") || bearer.is_none());
                if let Some(saved) = dpop {
                    match dpop_environment(
                        saved,
                        environment_id,
                        label,
                        http_base,
                        now_ms,
                        &mut key,
                        load_key,
                    ) {
                        Ok(environment) => catalog.environments.push(environment),
                        Err(DpopSkip::Notice(notice)) => catalog.notices.push(notice),
                        Err(DpopSkip::Kind(kind)) => catalog.skipped.push((label.clone(), kind)),
                    }
                    continue;
                }
                if let Some(kind) = bearer_kind(http_base, *authorization) {
                    catalog.skipped.push((label.clone(), kind));
                    continue;
                }
                let Some(token) = bearer else {
                    catalog.skipped.push((label.clone(), "no-credential"));
                    continue;
                };
                catalog.environments.push(SavedEnvironment {
                    environment_id: environment_id.clone(),
                    label: label.clone(),
                    http_base: http_base.trim_end_matches('/').to_owned(),
                    credential: Credential::Bearer((*token).to_owned()),
                });
            }
        }
    }
    // The same environment can be saved more than once (one target per pairing). Keep the first
    // connectable entry, and do not report siblings of a connectable environment as skipped.
    let mut seen = std::collections::HashSet::new();
    catalog
        .environments
        .retain(|saved| seen.insert(saved.environment_id.clone()));
    catalog.skipped.retain(|(skipped, _)| {
        !catalog
            .environments
            .iter()
            .any(|saved| &saved.label == skipped)
    });
    Ok(catalog)
}

enum DpopSkip {
    Notice(String),
    Kind(&'static str),
}

/// Maps a saved DPoP token to a connectable environment, or says why it cannot be used. The
/// direct profile address wins over the token's own endpoint; both must be plain `http`.
fn dpop_environment(
    saved: &DpopToken,
    environment_id: &str,
    label: &str,
    profile_base: &str,
    now_ms: u64,
    key: &mut Option<Result<Arc<SigningKey>, String>>,
    load_key: KeyLoader,
) -> Result<SavedEnvironment, DpopSkip> {
    // The token is minted through the T3 Connect relay, which this client cannot reach, so an
    // expired one cannot be renewed here.
    if saved.expires_at_epoch_ms <= now_ms as f64 {
        return Err(DpopSkip::Notice(format!(
            "{label}: saved token expired, open the Electron app to refresh"
        )));
    }
    let http_base = [profile_base, saved.endpoint.http_base_url.as_str()]
        .into_iter()
        .find(|base| base.starts_with("http://"))
        .ok_or(DpopSkip::Kind("https"))?;
    let key = key.get_or_insert_with(load_key);
    let key = key
        .as_ref()
        .map_err(|error| DpopSkip::Notice(format!("{label}: no Electron DPoP key ({error})")))?;
    if electron_dpop::thumbprint(key) != saved.dpop_thumbprint {
        return Err(DpopSkip::Notice(format!(
            "{label}: Electron's DPoP key does not match the saved token"
        )));
    }
    Ok(SavedEnvironment {
        environment_id: environment_id.to_owned(),
        label: label.to_owned(),
        http_base: http_base.trim_end_matches('/').to_owned(),
        credential: Credential::Dpop {
            token: saved.access_token.clone(),
            key: key.clone(),
        },
    })
}

/// PBKDF2-HMAC-SHA1 for one 16-byte block (SHA-1 yields 20 bytes, so a single block suffices).
fn pbkdf2_sha1_block(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 16] {
    let prf = |data: &[u8]| {
        let mut mac = Hmac::<Sha1>::new_from_slice(password).expect("HMAC takes any key length");
        mac.update(data);
        mac.finalize().into_bytes()
    };
    let mut first = salt.to_vec();
    first.extend_from_slice(&1u32.to_be_bytes());
    let mut u = prf(&first);
    let mut t = u;
    for _ in 1..iterations {
        u = prf(&u);
        for (acc, byte) in t.iter_mut().zip(u.iter()) {
            *acc ^= byte;
        }
    }
    let mut key = [0u8; 16];
    key.copy_from_slice(&t[..16]);
    key
}

/// Decrypts Electron safeStorage ciphertext (`v10` + AES-128-CBC) with the Keychain password.
pub fn decrypt(ciphertext: &[u8], password: &[u8]) -> Result<Vec<u8>, String> {
    let body = ciphertext
        .strip_prefix(SAFE_STORAGE_PREFIX)
        .ok_or("ciphertext is not v10 safeStorage data")?;
    let key = pbkdf2_sha1_block(password, SAFE_STORAGE_SALT, SAFE_STORAGE_ITERATIONS);
    Aes128CbcDec::new(&key.into(), &[b' '; 16].into())
        .decrypt_padded_vec_mut::<Pkcs7>(body)
        .map_err(|_| "could not decrypt the catalog (wrong Keychain key?)".to_owned())
}

#[cfg(target_os = "macos")]
fn keychain_password() -> Result<Vec<u8>, String> {
    use security_framework::passwords::get_generic_password;
    let mut last = String::new();
    for (service, account) in KEYCHAIN_ITEMS {
        match get_generic_password(service, account) {
            Ok(password) => return Ok(password),
            Err(error) => last = format!("{service}: {error}"),
        }
    }
    Err(format!("could not read the safeStorage key ({last})"))
}

#[cfg(not(target_os = "macos"))]
fn keychain_password() -> Result<Vec<u8>, String> {
    Err("reading the safeStorage key is only supported on macOS".to_owned())
}

/// Reads, decrypts and decodes the catalog at `path`. Blocking: the Keychain may prompt.
pub fn load(path: &Path) -> Result<Catalog, String> {
    let raw = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let envelope: Envelope =
        serde_json::from_str(&raw).map_err(|error| format!("unreadable catalog file: {error}"))?;
    if envelope.version != 1 {
        return Err(format!("unsupported catalog version {}", envelope.version));
    }
    let ciphertext = b64::decode(&envelope.encrypted_catalog)?;
    let plaintext = decrypt(&ciphertext, &keychain_password()?)?;
    let plaintext = String::from_utf8(plaintext).map_err(|_| "catalog is not UTF-8".to_owned())?;
    // TODO(gpui-credentials): HACK. Reads Electron's private IndexedDB and non-extractable DPoP key. Replace with GPUI's own pairing + Keychain before shipping.
    parse(&plaintext, auth::now_ms(), &mut || {
        let dir = electron_dpop::default_idb_dir().ok_or("no IndexedDB location")?;
        electron_dpop::load_key(&dir).map(Arc::new)
    })
}

/// Standard base64 (RFC 4648, padded), enough for the envelope; no extra crate.
pub(crate) mod b64 {
    pub fn decode(input: &str) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(input.len() * 3 / 4);
        let (mut buffer, mut bits) = (0u32, 0u32);
        for byte in input.trim().bytes() {
            let value = match byte {
                b'A'..=b'Z' => byte - b'A',
                b'a'..=b'z' => byte - b'a' + 26,
                b'0'..=b'9' => byte - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                b'=' => break,
                _ => return Err("catalog is not valid base64".to_owned()),
            };
            buffer = (buffer << 6) | u32::from(value);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((buffer >> bits) as u8);
                buffer &= (1 << bits) - 1;
            }
        }
        Ok(out)
    }

    #[cfg(test)]
    pub fn encode(input: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in input.chunks(3) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
            for i in 0..=chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            }
            out.extend(std::iter::repeat_n('=', 3 - chunk.len()));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::BlockEncryptMut;

    type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

    /// Encrypts like safeStorage, with a synthetic password.
    fn seal(plaintext: &[u8], password: &[u8]) -> Vec<u8> {
        let key = pbkdf2_sha1_block(password, SAFE_STORAGE_SALT, SAFE_STORAGE_ITERATIONS);
        let mut sealed = SAFE_STORAGE_PREFIX.to_vec();
        sealed.extend(
            Aes128CbcEnc::new(&key.into(), &[b' '; 16].into())
                .encrypt_padded_vec_mut::<Pkcs7>(plaintext),
        );
        sealed
    }

    #[test]
    fn pbkdf2_matches_the_rfc_6070_vector() {
        // RFC 6070, P="password", S="salt", c=2, first 16 bytes of the derived key.
        let key = pbkdf2_sha1_block(b"password", b"salt", 2);
        assert_eq!(hex::encode(key), "ea6c014dc72d6f8ccd1ed92ace1d41f0");
    }

    #[test]
    fn base64_round_trips() {
        for sample in [&b""[..], b"a", b"ab", b"abc", b"abcd", &[0, 255, 128, 7, 9]] {
            assert_eq!(b64::decode(&b64::encode(sample)).unwrap(), sample);
        }
        assert!(b64::decode("a*b").is_err());
    }

    #[test]
    fn decrypts_what_safe_storage_would_produce() {
        let sealed = seal(b"synthetic plaintext", b"synthetic-password");
        assert_eq!(
            decrypt(&sealed, b"synthetic-password").unwrap(),
            b"synthetic plaintext"
        );
        assert!(decrypt(&sealed, b"another-password").is_err());
        assert!(decrypt(b"v11whatever", b"synthetic-password").is_err());
        assert!(decrypt(&sealed[..sealed.len() - 1], b"synthetic-password").is_err());
    }

    fn no_key() -> Result<Arc<SigningKey>, String> {
        Err("no key".to_owned())
    }

    const DOCUMENT: &str = r#"{
      "schemaVersion": 1,
      "targets": [
        {"_tag":"BearerConnectionTarget","environmentId":"env-direct","label":"Direct","connectionId":"bearer:env-direct"},
        {"_tag":"BearerConnectionTarget","environmentId":"env-off","label":"Off","connectionId":"bearer:env-off"},
        {"_tag":"BearerConnectionTarget","environmentId":"env-connect","label":"Tunnel","connectionId":"bearer:env-connect"},
        {"_tag":"BearerConnectionTarget","environmentId":"env-tls","label":"Secure","connectionId":"bearer:env-tls"},
        {"_tag":"BearerConnectionTarget","environmentId":"env-bare","label":"Bare","connectionId":"bearer:env-bare"},
        {"_tag":"RelayConnectionTarget","environmentId":"env-relay","label":"Relayed"},
        {"_tag":"SshConnectionTarget","environmentId":"env-ssh","label":"Shell","connectionId":"ssh:env-ssh"},
        {"_tag":"FutureTarget","environmentId":"x","label":"Future"}
      ],
      "profiles": [
        {"_tag":"BearerConnectionProfile","connectionId":"bearer:env-direct","environmentId":"env-direct","label":"Direct","httpBaseUrl":"http://10.0.0.2:3773/","wsBaseUrl":"ws://10.0.0.2:3773/"},
        {"_tag":"BearerConnectionProfile","connectionId":"bearer:env-off","environmentId":"env-off","label":"Off","httpBaseUrl":"http://10.0.0.3:3773","wsBaseUrl":"ws://10.0.0.3:3773"},
        {"_tag":"BearerConnectionProfile","connectionId":"bearer:env-connect","environmentId":"env-connect","label":"Tunnel","httpBaseUrl":"http://tunnel.invalid","wsBaseUrl":"ws://tunnel.invalid","authorization":"t3-connect"},
        {"_tag":"BearerConnectionProfile","connectionId":"bearer:env-tls","environmentId":"env-tls","label":"Secure","httpBaseUrl":"https://host.invalid","wsBaseUrl":"wss://host.invalid"},
        {"_tag":"BearerConnectionProfile","connectionId":"bearer:env-bare","environmentId":"env-bare","label":"Bare","httpBaseUrl":"http://10.0.0.4:3773","wsBaseUrl":"ws://10.0.0.4:3773"},
        {"_tag":"SshConnectionProfile","connectionId":"ssh:env-ssh","environmentId":"env-ssh","label":"Shell","target":{}}
      ],
      "credentials": [
        {"connectionId":"bearer:env-direct","credential":{"_tag":"BearerConnectionCredential","token":"synthetic-token"}},
        {"connectionId":"bearer:env-off","credential":{"_tag":"BearerConnectionCredential","token":"synthetic-token-2"}}
      ],
      "remoteDpopTokens": [],
      "disabledEnvironmentIds": ["env-off"]
    }"#;

    #[test]
    fn parse_keeps_direct_bearer_environments_and_names_the_rest() {
        let catalog = parse(DOCUMENT, 0, &mut no_key).unwrap();
        assert_eq!(catalog.environments.len(), 1);
        let direct = &catalog.environments[0];
        assert_eq!(direct.environment_id, "env-direct");
        assert_eq!(direct.label, "Direct");
        assert_eq!(direct.http_base, "http://10.0.0.2:3773");
        assert!(matches!(&direct.credential, Credential::Bearer(t) if t == "synthetic-token"));
        assert_eq!(
            catalog.skipped,
            [
                ("Tunnel".to_owned(), "t3-connect"),
                ("Secure".to_owned(), "https"),
                ("Bare".to_owned(), "no-credential"),
                ("Relayed".to_owned(), "relay"),
                ("Shell".to_owned(), "ssh"),
            ]
        );
        assert!(parse("{", 0, &mut no_key).is_err());
    }

    fn dpop_document(thumbprint: &str, expires_at: u64, authorization: &str, base: &str) -> String {
        format!(
            r#"{{"targets":[{{"_tag":"BearerConnectionTarget","environmentId":"env-a","label":"Devbox","connectionId":"bearer:env-a"}}],
            "profiles":[{{"_tag":"BearerConnectionProfile","connectionId":"bearer:env-a","httpBaseUrl":"{base}"{authorization}}}],
            "credentials":[],
            "remoteDpopTokens":[{{"environmentId":"env-a","label":"Devbox","accessToken":"synthetic-access",
              "endpoint":{{"httpBaseUrl":"https://relay.invalid","wsBaseUrl":"wss://relay.invalid","providerKind":"t3_relay"}},
              "expiresAtEpochMs":{expires_at},"dpopThumbprint":"{thumbprint}"}}]}}"#
        )
    }

    #[test]
    fn dpop_tokens_become_credentials_when_fresh_and_bound_to_the_electron_key() {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).unwrap();
        let key = Arc::new(SigningKey::from_slice(&seed).unwrap());
        let bound = electron_dpop::thumbprint(&key);
        let mut loads = 0;
        let mut load = || {
            loads += 1;
            Ok(key.clone())
        };

        let fresh = dpop_document(&bound, 2_000, "", "http://10.0.0.5:3773/");
        let catalog = parse(&fresh, 1_000, &mut load).unwrap();
        assert_eq!(catalog.environments.len(), 1);
        let saved = &catalog.environments[0];
        assert_eq!(saved.http_base, "http://10.0.0.5:3773");
        assert!(matches!(
            &saved.credential,
            Credential::Dpop { token, .. } if token == "synthetic-access"
        ));

        // The token's own relay endpoint is https, so a t3-connect profile still dials its direct address.
        let connect = dpop_document(
            &bound,
            2_000,
            r#","authorization":"t3-connect""#,
            "http://10.0.0.5:3773",
        );
        assert_eq!(
            parse(&connect, 1_000, &mut load)
                .unwrap()
                .environments
                .len(),
            1
        );

        let expired = parse(&fresh, 2_000, &mut load).unwrap();
        assert!(expired.environments.is_empty());
        assert_eq!(
            expired.notices,
            ["Devbox: saved token expired, open the Electron app to refresh"]
        );

        let other = dpop_document("someone-else", 2_000, "", "http://10.0.0.5:3773");
        let mismatched = parse(&other, 1_000, &mut load).unwrap();
        assert!(mismatched.environments.is_empty());
        assert!(mismatched.notices[0].contains("does not match"));

        let tls = dpop_document(&bound, 2_000, "", "https://host.invalid");
        assert_eq!(
            parse(&tls, 1_000, &mut load).unwrap().skipped,
            [("Devbox".to_owned(), "https")]
        );

        let keyless = parse(&fresh, 1_000, &mut no_key).unwrap();
        assert!(keyless.environments.is_empty());
        assert!(keyless.notices[0].contains("no Electron DPoP key"));
        // Expired tokens never touch the IndexedDB.
        assert_eq!(loads, 3);
    }

    #[test]
    fn load_reads_the_envelope_and_rejects_other_versions() {
        let dir = std::env::temp_dir().join(format!("t3-gpui-catalog-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(CATALOG_FILE);
        std::fs::write(&path, r#"{"version":2,"encryptedCatalog":""}"#).unwrap();
        assert!(load(&path).err().unwrap().contains("version"));
        std::fs::write(&path, "nope").unwrap();
        assert!(load(&path).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(load(&path).is_err());
    }

    #[test]
    fn duplicate_saves_of_one_environment_collapse_to_the_connectable_one() {
        let document = r#"{"targets":[
          {"_tag":"BearerConnectionTarget","environmentId":"e","label":"Box","connectionId":"c1"},
          {"_tag":"BearerConnectionTarget","environmentId":"e","label":"Box","connectionId":"c2"}],
          "profiles":[
          {"_tag":"BearerConnectionProfile","connectionId":"c1","httpBaseUrl":"http://10.0.0.9:1"},
          {"_tag":"BearerConnectionProfile","connectionId":"c2","httpBaseUrl":"http://10.0.0.9:1"}],
          "credentials":[{"connectionId":"c2","credential":{"_tag":"BearerConnectionCredential","token":"t"}}]}"#;
        let catalog = parse(document, 0, &mut no_key).unwrap();
        assert_eq!(catalog.environments.len(), 1);
        assert!(catalog.skipped.is_empty());
    }
}
