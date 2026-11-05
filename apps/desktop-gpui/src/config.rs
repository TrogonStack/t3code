//! Launch settings handed over by the dev runner (or, later, a packaged launcher).

pub const ENV_PORT: &str = "T3CODE_GPUI_SERVER_PORT";
pub const ENV_HOME: &str = "T3CODE_GPUI_HOME";
pub const ENV_COMMAND: &str = "T3CODE_GPUI_SERVER_COMMAND";
pub const ENV_ENTRY: &str = "T3CODE_GPUI_SERVER_ENTRY";
pub const ENV_ATTACH_URL: &str = "T3CODE_GPUI_SERVER_URL";
pub const ENV_PAIRING_TOKEN: &str = "T3CODE_GPUI_PAIRING_TOKEN";
pub const ENV_CONNECTION_CATALOG: &str = "T3CODE_GPUI_CONNECTION_CATALOG";

/// The Electron app's saved connections: the override, else the catalog inside the T3 home.
pub fn connection_catalog_path() -> Option<std::path::PathBuf> {
    catalog_path_from(|name| std::env::var(name).ok())
}

fn catalog_path_from(get: impl Fn(&str) -> Option<String>) -> Option<std::path::PathBuf> {
    let non_empty = |name| get(name).filter(|value| !value.trim().is_empty());
    non_empty(ENV_CONNECTION_CATALOG)
        .map(std::path::PathBuf::from)
        .or_else(|| {
            non_empty(ENV_HOME)
                .map(|home| std::path::Path::new(&home).join(crate::catalog::CATALOG_FILE))
        })
}

/// A server someone else runs (a dev server, `npx t3`, a remote host), reached
/// with a pairing token instead of being spawned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attach {
    pub http_base: String,
    pub pairing_token: String,
}

impl Attach {
    pub fn from_env() -> Option<Self> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let non_empty = |name| {
            get(name)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        Some(Self {
            http_base: non_empty(ENV_ATTACH_URL)?.trim_end_matches('/').to_owned(),
            pairing_token: non_empty(ENV_PAIRING_TOKEN)?,
        })
    }
}

/// How to start the server. The command is data: dev runs `node <entry>`, a
/// packaged build runs `t3` from PATH with no entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub port: u16,
    pub t3_home: String,
    pub command: ServerCommand,
}

impl Config {
    pub fn http_base(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// `None` when the launcher did not ask this app to manage a server.
    pub fn from_env() -> Option<Self> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let non_empty = |name| get(name).filter(|value| !value.trim().is_empty());
        let port = non_empty(ENV_PORT)?.trim().parse().ok()?;
        let t3_home = non_empty(ENV_HOME)?;
        let program = non_empty(ENV_COMMAND)?;
        let args = non_empty(ENV_ENTRY).into_iter().collect();
        Some(Self {
            port,
            t3_home,
            command: ServerCommand { program, args },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        }
    }

    #[test]
    fn reads_all_settings() {
        let config = Config::from_lookup(lookup(&[
            (ENV_PORT, "13773"),
            (ENV_HOME, "/work/.t3"),
            (ENV_COMMAND, "node"),
            (ENV_ENTRY, "/work/apps/server/src/bin.ts"),
        ]))
        .expect("config");
        assert_eq!(config.port, 13773);
        assert_eq!(config.command.args, vec!["/work/apps/server/src/bin.ts"]);
        assert_eq!(config.http_base(), "http://127.0.0.1:13773");
    }

    #[test]
    fn entry_is_optional_and_missing_values_disable_the_server() {
        let config = Config::from_lookup(lookup(&[
            (ENV_PORT, "1"),
            (ENV_HOME, "/h"),
            (ENV_COMMAND, "t3"),
        ]))
        .expect("config");
        assert!(config.command.args.is_empty());
        assert_eq!(Config::from_lookup(lookup(&[(ENV_PORT, "1")])), None);
        assert_eq!(Config::from_lookup(lookup(&[])), None);
    }

    #[test]
    fn attach_needs_url_and_token() {
        let attach = Attach::from_lookup(lookup(&[
            (ENV_ATTACH_URL, "http://127.0.0.1:13773/"),
            (ENV_PAIRING_TOKEN, "tok"),
        ]))
        .expect("attach");
        assert_eq!(attach.http_base, "http://127.0.0.1:13773");
        assert_eq!(
            Attach::from_lookup(lookup(&[(ENV_ATTACH_URL, "http://h")])),
            None
        );
    }

    #[test]
    fn catalog_path_prefers_the_override_then_the_t3_home() {
        let path = |pairs| catalog_path_from(lookup(pairs));
        assert_eq!(
            path(&[
                (ENV_CONNECTION_CATALOG, "/c/catalog.json"),
                (ENV_HOME, "/h")
            ]),
            Some("/c/catalog.json".into())
        );
        assert_eq!(
            path(&[(ENV_HOME, "/h")]),
            Some("/h/connection-catalog.json".into())
        );
        assert_eq!(path(&[]), None);
    }
}
