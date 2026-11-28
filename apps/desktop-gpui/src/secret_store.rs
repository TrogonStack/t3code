//! Small secrets (sessions, the T3 Connect account, the device key), keyed by service and account.
//!
//! Release builds use the login Keychain. Debug builds use an owner-only file in the dev state dir:
//! macOS keys a Keychain grant to a build without an Apple Team ID by its code hash, so every
//! recompile would ask again no matter how often "Always Allow" is clicked.
//!
//! TODO: the dev file is plaintext, weaker than Electron dev's safeStorage. Fix by signing dev
//! builds with an Apple Development identity (stable Team ID) and moving both profiles to one
//! Keychain-held key that encrypts a secrets file, like `apps/desktop`'s `ElectronSafeStorage`.

pub fn get(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
    imp::get(service, account)
}

pub fn set(service: &str, account: &str, secret: &[u8]) -> Result<(), String> {
    imp::set(service, account, secret)
}

/// Deleting what is not there succeeds.
pub fn delete(service: &str, account: &str) -> Result<(), String> {
    imp::delete(service, account)
}

pub fn accounts(service: &str) -> Result<Vec<String>, String> {
    imp::accounts(service)
}

#[cfg(all(target_os = "macos", not(debug_assertions)))]
mod imp {
    use security_framework::item::{ItemClass, ItemSearchOptions, Limit};
    use security_framework::passwords::{
        delete_generic_password, get_generic_password, set_generic_password,
    };

    /// `errSecItemNotFound`.
    const NOT_FOUND: i32 = -25300;

    fn describe(error: security_framework::base::Error) -> String {
        format!("Keychain: {error}")
    }

    pub fn get(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
        match get_generic_password(service, account) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.code() == NOT_FOUND => Ok(None),
            Err(error) => Err(describe(error)),
        }
    }

    pub fn set(service: &str, account: &str, secret: &[u8]) -> Result<(), String> {
        set_generic_password(service, account, secret).map_err(describe)
    }

    pub fn delete(service: &str, account: &str) -> Result<(), String> {
        match delete_generic_password(service, account) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == NOT_FOUND => Ok(()),
            Err(error) => Err(describe(error)),
        }
    }

    pub fn accounts(service: &str) -> Result<Vec<String>, String> {
        let found = ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(service)
            .load_attributes(true)
            .limit(Limit::All)
            .search();
        match found {
            Ok(results) => Ok(results
                .into_iter()
                .filter_map(|result| result.simplify_dict()?.get("acct").cloned())
                .collect()),
            Err(error) if error.code() == NOT_FOUND => Ok(Vec::new()),
            Err(error) => Err(describe(error)),
        }
    }
}

#[cfg(all(not(target_os = "macos"), not(debug_assertions)))]
mod imp {
    const UNSUPPORTED: &str = "saving secrets needs the macOS Keychain";

    pub fn get(_: &str, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
    pub fn set(_: &str, _: &str, _: &[u8]) -> Result<(), String> {
        Err(UNSUPPORTED.to_owned())
    }
    pub fn delete(_: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    pub fn accounts(_: &str) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }
}

#[cfg(debug_assertions)]
mod imp {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use std::path::PathBuf;
    use std::sync::Mutex;

    type Items = std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>;

    static LOCK: Mutex<()> = Mutex::new(());

    fn path() -> Result<PathBuf, String> {
        let home = std::env::var(crate::config::ENV_HOME)
            .ok()
            .filter(|home| !home.trim().is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|home| PathBuf::from(home).join(".t3/dev"))
            })
            .ok_or("no home directory for dev secrets")?;
        Ok(home.join("gpui-dev-secrets.json"))
    }

    fn read() -> Result<Items, String> {
        match std::fs::read(path()?) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| "the dev secrets file is unreadable".to_owned()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Items::new()),
            Err(error) => Err(format!("dev secrets: {error}")),
        }
    }

    fn write(items: &Items) -> Result<(), String> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let path = path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|error| format!("dev secrets: {error}"))?;
        }
        let bytes = serde_json::to_vec(items).map_err(|error| error.to_string())?;
        let temp = path.with_extension("json.tmp");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temp)
            .map_err(|error| format!("dev secrets: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("dev secrets: {error}"))?;
        std::fs::rename(&temp, &path).map_err(|error| format!("dev secrets: {error}"))
    }

    pub fn get(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
        let _guard = LOCK.lock().expect("dev secrets lock");
        read()?
            .get(service)
            .and_then(|accounts| accounts.get(account))
            .map(|encoded| {
                STANDARD
                    .decode(encoded)
                    .map_err(|_| "a dev secret is unreadable".to_owned())
            })
            .transpose()
    }

    pub fn set(service: &str, account: &str, secret: &[u8]) -> Result<(), String> {
        let _guard = LOCK.lock().expect("dev secrets lock");
        let mut items = read()?;
        items
            .entry(service.to_owned())
            .or_default()
            .insert(account.to_owned(), STANDARD.encode(secret));
        write(&items)
    }

    pub fn delete(service: &str, account: &str) -> Result<(), String> {
        let _guard = LOCK.lock().expect("dev secrets lock");
        let mut items = read()?;
        let removed = items
            .get_mut(service)
            .is_some_and(|accounts| accounts.remove(account).is_some());
        if removed { write(&items) } else { Ok(()) }
    }

    pub fn accounts(service: &str) -> Result<Vec<String>, String> {
        let _guard = LOCK.lock().expect("dev secrets lock");
        Ok(read()?
            .get(service)
            .map(|accounts| accounts.keys().cloned().collect())
            .unwrap_or_default())
    }
}
