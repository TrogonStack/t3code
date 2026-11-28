//! The logic of the web's "Add to another machine" dialog, without the dialog: clone URLs
//! (`packages/shared/src/cloneUrl.ts`), the folder a clone lands in (`getCloneDirectoryPath` and
//! friends of `packages/client-runtime/src/operations/projects.ts`), the ignored files to bring
//! along (`apps/web/src/lib/addProjectToMachine.ts`) and the request payloads of the RPCs it
//! calls.

use serde_json::{Value, json};
use std::collections::HashSet;

/// Matches the server's read cap: a larger file would arrive truncated.
pub const COPYABLE_FILE_MAX_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Ssh,
    Https,
}

struct Parsed<'a> {
    protocol: Protocol,
    host: &'a str,
    path: &'a str,
    custom_port: bool,
}

/// `(?:[^/\s]*@)?(\[[^\]@/\s]+\]|[^@:/\s]+)(:\d+)?/(.+)` after the scheme.
fn parse_authority<'a>(rest: &'a str, protocol: Protocol) -> Option<Parsed<'a>> {
    let slash = rest.find('/')?;
    let (authority, path) = (&rest[..slash], &rest[slash + 1..]);
    if authority.chars().any(char::is_whitespace) || path.is_empty() {
        return None;
    }
    let host_and_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, tail)| tail);
    let (host, custom_port) = match host_and_port.strip_prefix('[') {
        Some(inner) => {
            let close = inner.find(']')?;
            let after = &inner[close + 1..];
            let host = &host_and_port[..close + 2];
            if close == 0 || inner[..close].contains('@') {
                return None;
            }
            match after {
                "" => (host, false),
                port if is_port(port) => (host, true),
                _ => return None,
            }
        }
        None => match host_and_port.split_once(':') {
            Some((host, port)) if is_port(&format!(":{port}")) => (host, true),
            Some(_) => return None,
            None => (host_and_port, false),
        },
    };
    (!host.is_empty() && !host.contains('@')).then_some(Parsed {
        protocol,
        host,
        path,
        custom_port,
    })
}

fn is_port(text: &str) -> bool {
    text.strip_prefix(':')
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
}

/// `SCP_LIKE_URL`: `[user@]host:path`, where the path does not start with a slash.
fn parse_scp(text: &str) -> Option<Parsed<'_>> {
    let attempt = |rest: &'_ str| -> Option<(usize, usize)> {
        let (host_len, after) = if let Some(inner) = rest.strip_prefix('[') {
            let close = inner.find(']')?;
            let body = &inner[..close];
            if body.is_empty()
                || body
                    .chars()
                    .any(|c| c == '@' || c == '/' || c.is_whitespace())
            {
                return None;
            }
            (close + 2, &rest[close + 2..])
        } else {
            let end = rest.find(':')?;
            let host = &rest[..end];
            if host.is_empty()
                || host
                    .chars()
                    .any(|c| c == '/' || c == '[' || c == ']' || c.is_whitespace())
            {
                return None;
            }
            (end, &rest[end..])
        };
        let path = after.strip_prefix(':')?;
        if path.is_empty() || path.starts_with('/') {
            return None;
        }
        Some((host_len, rest.len() - path.len()))
    };
    let from_rest = |offset: usize| {
        let rest = &text[offset..];
        attempt(rest).map(|(host_len, path_start)| Parsed {
            protocol: Protocol::Ssh,
            host: &text[offset..offset + host_len],
            path: &text[offset + path_start..],
            custom_port: false,
        })
    };
    let with_user = text.find('@').and_then(|at| {
        let user = &text[..at];
        let valid = !user.is_empty() && !user.chars().any(|c| c == '/' || c.is_whitespace());
        if valid { from_rest(at + 1) } else { None }
    });
    with_user.or_else(|| from_rest(0))
}

fn parse(url: &str) -> Option<Parsed<'_>> {
    let text = url.trim();
    if let Some(rest) = text.strip_prefix("ssh://")
        && let Some(parsed) = parse_authority(rest, Protocol::Ssh)
    {
        return Some(parsed);
    }
    for scheme in ["https://", "http://"] {
        if let Some(rest) = text.strip_prefix(scheme)
            && let Some(parsed) = parse_authority(rest, Protocol::Https)
        {
            return Some(parsed);
        }
    }
    parse_scp(text)
}

pub fn url_protocol(url: &str) -> Option<Protocol> {
    parse(url).map(|parsed| parsed.protocol)
}

/// The path of an Azure URL as `[organization, project, repository]`, after the fixed marker.
fn azure_parts(path: &str, marker_index: usize, marker: &str) -> Option<[String; 3]> {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() != 4 || parts[marker_index] != marker {
        return None;
    }
    let mut names = parts
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != marker_index)
        .map(|(_, part)| *part);
    let mut next = || {
        names
            .next()
            .filter(|part| !part.is_empty() && !part.contains(['?', '#']))
            .map(str::to_owned)
    };
    Some([next()?, next()?, next()?])
}

/// Rewrites a clone URL to the other protocol, because the target machine may only have SSH keys
/// or only an HTTPS credential helper. `None` when it cannot be rewritten safely.
pub fn url_for_protocol(url: &str, protocol: Protocol) -> Option<String> {
    let parsed = parse(url)?;
    if parsed.protocol == protocol {
        return Some(url.trim().to_owned());
    }
    if parsed.custom_port {
        return None;
    }
    let host = parsed.host.to_lowercase();
    if parsed.protocol == Protocol::Ssh && host == "ssh.dev.azure.com" {
        let [org, project, repository] = azure_parts(parsed.path, 0, "v3")?;
        return Some(format!(
            "https://dev.azure.com/{org}/{project}/_git/{repository}"
        ));
    }
    if parsed.protocol == Protocol::Https && host == "dev.azure.com" {
        let [org, project, repository] = azure_parts(parsed.path, 2, "_git")?;
        return Some(format!(
            "git@ssh.dev.azure.com:v3/{org}/{project}/{repository}"
        ));
    }
    Some(match protocol {
        Protocol::Ssh => format!("git@{}:{}", parsed.host, parsed.path),
        Protocol::Https => format!("https://{}/{}", parsed.host, parsed.path),
    })
}

fn is_windows_drive_path(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(letter), Some(':'), None | Some('/' | '\\')) if letter.is_ascii_alphabetic()
    )
}

fn is_unc_path(value: &str) -> bool {
    value.starts_with("\\\\")
}

fn is_windows_absolute_path(value: &str) -> bool {
    is_unc_path(value) || is_windows_drive_path(value)
}

fn is_host_path(value: &str) -> bool {
    value.starts_with('/') || is_windows_absolute_path(value) || value.starts_with("~/")
}

fn unsafe_segment(segment: &str) -> bool {
    matches!(segment, "." | "..")
        || segment
            .chars()
            .any(|c| matches!(c, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control())
        || segment.ends_with([' ', '.'])
}

fn windows_device_name(segment: &str) -> bool {
    let lower = segment.to_lowercase();
    let digits = [
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '\u{b9}', '\u{b2}', '\u{b3}',
    ];
    let mut names: Vec<String> = ["con", "prn", "aux", "nul", "conin$", "conout$"]
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    for prefix in ["com", "lpt"] {
        names.extend(digits.iter().map(|digit| format!("{prefix}{digit}")));
    }
    names.iter().any(|name| {
        lower.strip_prefix(name.as_str()).is_some_and(|rest| {
            let rest = rest.trim_start_matches(' ');
            rest.is_empty() || rest.starts_with('.')
        })
    })
}

/// The `<host>/<owner>/<repo>` folders ghq clones a URL into, or `None` for local paths and
/// anything with a folder that could leave the root.
pub fn repository_path(url: &str) -> Option<String> {
    if is_windows_drive_path(url.trim()) {
        return None;
    }
    let parsed = parse(url)?;
    let path = parsed.path.split(['?', '#']).next().unwrap_or_default();
    let mut segments: Vec<String> = std::iter::once(parsed.host.to_owned())
        .chain(path.split('/').filter(|s| !s.is_empty()).map(str::to_owned))
        .collect();
    if let Some(last) = segments.last_mut()
        && let Some(stripped) = last.strip_suffix(".git")
    {
        *last = stripped.to_owned();
    }
    let safe = segments.len() >= 2
        && segments
            .iter()
            .all(|s| !s.is_empty() && !unsafe_segment(s) && !windows_device_name(s));
    safe.then(|| segments.join("/"))
}

/// `getCloneDirectoryName`.
pub fn directory_name(repository_or_url: &str) -> String {
    let without_query = repository_or_url
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    let scheme = without_query.find("://");
    let has_host = scheme.is_some() || has_scp_prefix(without_query);
    let path = scheme.map_or(without_query, |at| &without_query[at + 3..]);
    let segments: Vec<&str> = path
        .split(['/', '\\', ':'])
        .filter(|segment| !segment.trim().is_empty())
        .collect();
    if has_host && segments.len() < 2 {
        return String::new();
    }
    let last = segments.last().map_or("", |segment| segment.trim());
    if has_host
        && segments.len() == 2
        && !last.is_empty()
        && last.chars().all(|c| c.is_ascii_digit())
    {
        return String::new();
    }
    last.strip_suffix(".git").unwrap_or(last).to_owned()
}

/// `^[^/\\:]+@[^/\\:]+:`
fn has_scp_prefix(text: &str) -> bool {
    let run_end = text.find(['/', '\\', ':']).unwrap_or(text.len());
    if !text[run_end..].starts_with(':') {
        return false;
    }
    let run = &text[..run_end];
    run.char_indices()
        .any(|(index, c)| c == '@' && index >= 1 && index + 1 < run.len())
}

/// The folder a clone gets below its base: `<repo>`, or `<host>/<owner>/<repo>` like ghq.
pub fn directory_path(remote_url: &str, ghq_layout: bool) -> String {
    ghq_layout
        .then(|| repository_path(remote_url))
        .flatten()
        .unwrap_or_else(|| directory_name(remote_url))
}

fn preferred_separator(value: &str) -> char {
    if is_windows_absolute_path(value) {
        '\\'
    } else if value.starts_with('/') {
        '/'
    } else if value.contains('\\') {
        '\\'
    } else {
        '/'
    }
}

fn has_trailing_separator(value: &str) -> bool {
    if !is_windows_absolute_path(value) && value.starts_with('/') {
        value.ends_with('/')
    } else {
        value.ends_with(['/', '\\'])
    }
}

fn ensure_directory_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || has_trailing_separator(trimmed) {
        return trimmed.to_owned();
    }
    format!("{trimmed}{}", preferred_separator(trimmed))
}

/// `getAddProjectInitialQuery`: the base folder clones land in, home when none is set.
pub fn initial_query(base_directory: Option<&str>) -> String {
    let trimmed = base_directory.map_or("", str::trim);
    if trimmed.is_empty() {
        "~/".to_owned()
    } else {
        ensure_directory_path(trimmed)
    }
}

/// `getCloneDestinationPath`.
pub fn destination_path(directory: &str, name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        return directory.to_owned();
    }
    let base = ensure_directory_path(directory);
    if base.ends_with('\\') {
        format!("{base}{}", name.replace('/', "\\"))
    } else {
        format!("{base}{name}")
    }
}

/// Env files are the ignored files people almost always want on a new machine.
pub fn is_default_copied(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name == ".env"
        || name
            .strip_prefix(".env.")
            .is_some_and(|rest| !rest.is_empty())
        || name == ".envrc"
        || name == ".dev.vars"
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IgnoredFile {
    pub path: String,
    pub byte_length: u64,
}

impl IgnoredFile {
    pub fn copyable(&self) -> bool {
        self.byte_length <= COPYABLE_FILE_MAX_BYTES
    }
}

/// What the dialog checks before the user touches the list.
pub fn default_selection(files: &[IgnoredFile]) -> HashSet<String> {
    files
        .iter()
        .filter(|file| file.copyable() && is_default_copied(&file.path))
        .map(|file| file.path.clone())
        .collect()
}

/// The set the All/None button leaves behind.
pub fn toggled_all(selected: &HashSet<String>, files: &[IgnoredFile]) -> HashSet<String> {
    let copyable: HashSet<String> = files
        .iter()
        .filter(|file| file.copyable())
        .map(|file| file.path.clone())
        .collect();
    if !copyable.is_empty() && selected.len() == copyable.len() {
        HashSet::new()
    } else {
        copyable
    }
}

/// Whether the button reads "None" (everything copyable is selected) rather than "All".
pub fn all_selected(selected: &HashSet<String>, files: &[IgnoredFile]) -> bool {
    let copyable = files.iter().filter(|file| file.copyable()).count();
    copyable > 0 && selected.len() == copyable
}

fn format_scaled(value: f64) -> String {
    let digits = if value >= 100. {
        0
    } else if value >= 10. {
        1
    } else {
        2
    };
    format!("{value:.digits$}")
}

/// `formatBytes`.
pub fn format_bytes(value: u64) -> String {
    let value = value as f64;
    if value.round() < 1024. {
        return format!("{} B", value.round());
    }
    let units = ["KB", "MB", "GB", "TB"];
    let mut next = value / 1024.;
    let mut index = 0;
    // Pick the unit after rounding, so 1023.6 KB reads "1.00 MB", not "1024 KB".
    while format_scaled(next).parse::<f64>().unwrap_or(0.) >= 1024. && index < units.len() - 1 {
        next /= 1024.;
        index += 1;
    }
    format!("{} {}", format_scaled(next), units[index])
}

/// The size column of a file row.
pub fn size_label(file: &IgnoredFile) -> String {
    if file.copyable() {
        format_bytes(file.byte_length)
    } else {
        "Over 1 MB".to_owned()
    }
}

/// A project can be rooted below its repository's top folder. The clone lands at the top, so
/// copied files keep the same path below it.
pub fn project_subdirectory(repository_root: Option<&str>, workspace_root: &str) -> String {
    let Some(root) = repository_root.filter(|root| !root.is_empty()) else {
        return String::new();
    };
    let prefix = root.trim_end_matches(['/', '\\']);
    match workspace_root.strip_prefix(prefix) {
        Some(rest) if rest.starts_with(['/', '\\']) => rest[1..].replace('\\', "/"),
        _ => String::new(),
    }
}

/// Where a copied file lands relative to the clone.
pub fn copy_target(subdirectory: &str, relative_path: &str) -> String {
    if subdirectory.is_empty() {
        relative_path.to_owned()
    } else {
        format!("{subdirectory}/{relative_path}")
    }
}

/// The text that opens a quoted path at its end: `'/a b'` or `"C:\a b"`, then only whitespace.
fn quoted_path_at_end(text: &str) -> Option<(usize, String)> {
    fn single(text: &str, from: usize) -> Option<String> {
        // `(?:[^']|'\\'')+` then the closing quote, then only whitespace.
        fn walk(text: &str, at: usize, start: usize) -> Option<String> {
            let rest = &text[at..];
            if let Some(tail) = rest.strip_prefix("'\\''")
                && let Some(found) = walk(text, text.len() - tail.len(), start)
            {
                return Some(found);
            }
            if let Some(tail) = rest.strip_prefix('\'') {
                return (at > start && tail.trim().is_empty()).then(|| text[start..at].to_owned());
            }
            let next = rest.chars().next()?;
            walk(text, at + next.len_utf8(), start)
        }
        walk(text, from, from)
    }
    for (index, c) in text.char_indices() {
        if c == '\'' {
            if let Some(content) = single(text, index + 1) {
                return Some((index, content.replace("'\\''", "'")));
            }
        } else if c == '"' {
            let body = &text[index + 1..];
            if let Some(close) = body.find('"')
                && close > 0
                && body[close + 1..].trim().is_empty()
            {
                return Some((index, body[..close].to_owned()));
            }
        }
    }
    None
}

fn shell_unescape(path: &str) -> String {
    const ESCAPED: &str = "'\"\\()[]{}<>|&;$`!*?#~^=";
    let mut out = String::with_capacity(path.len());
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        match chars.peek() {
            Some(&next) if c == '\\' && (next.is_whitespace() || ESCAPED.contains(next)) => {
                out.push(next);
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}

/// `resolvePastedPath`: what a folder box holds after a change. A full path pasted into it
/// replaces the box, even after the text already there, because `~/'/a b'` would create that
/// nested folder.
pub fn resolve_pasted_path(previous: &str, next: &str) -> String {
    let (previous_len, next_len) = (previous.chars().count(), next.chars().count());
    let typing = if next.starts_with(previous) {
        next_len == previous_len + 1
    } else {
        previous.starts_with(next)
    };
    if typing {
        return next.to_owned();
    }
    if let Some((index, quoted)) = quoted_path_at_end(next)
        && previous.starts_with(next[..index].trim_end())
        && is_host_path(&quoted)
    {
        return quoted;
    }
    let after_folder = previous.ends_with(['/', '\\']) && next.starts_with(previous);
    let path = if after_folder {
        &next[previous.len()..]
    } else {
        next
    };
    if !is_host_path(path) {
        return next.to_owned();
    }
    // `\` separates folders in Windows paths, so it only escapes in `/` and `~/` paths.
    if is_windows_absolute_path(path) {
        path.to_owned()
    } else {
        shell_unescape(path)
    }
}

/// `projects.listIgnoredFiles`, answered by the source environment.
pub fn list_payload(cwd: &str) -> Value {
    json!({ "cwd": cwd })
}

/// `projects.readFile`, answered by the source environment.
pub fn read_payload(cwd: &str, relative_path: &str) -> Value {
    json!({ "cwd": cwd, "relativePath": relative_path })
}

/// `projects.writeFile`, answered by the target environment.
pub fn write_payload(cwd: &str, relative_path: &str, contents: &str) -> Value {
    json!({ "cwd": cwd, "relativePath": relative_path, "contents": contents })
}

/// `projectClone.start`, answered by the target environment.
pub fn start_payload(
    project_id: &str,
    title: &str,
    created_at: &str,
    remote_url: &str,
    destination_path: &str,
) -> Value {
    json!({
        "projectId": project_id,
        "title": title,
        "createdAt": created_at,
        "remoteUrl": remote_url,
        "destinationPath": destination_path,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct IgnoredFiles {
    pub files: Vec<IgnoredFile>,
    pub truncated: bool,
}

pub fn decode_ignored_files(value: &Value) -> Option<IgnoredFiles> {
    let files = value
        .get("files")?
        .as_array()?
        .iter()
        .map(|file| {
            Some(IgnoredFile {
                path: file.get("path")?.as_str()?.to_owned(),
                byte_length: file.get("byteLength")?.as_u64()?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(IgnoredFiles {
        files,
        truncated: value
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// What the dialog reads from `server.getConfig`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ServerSettings {
    /// `environment.capabilities.ignoredFileListing`.
    pub ignored_file_listing: bool,
    /// `settings.addProjectBaseDirectory`.
    pub base_directory: Option<String>,
    /// `settings.ghqCloneLayout`.
    pub ghq_layout: bool,
}

pub fn decode_config(value: &Value) -> ServerSettings {
    let flag = |value: Option<&Value>| value.and_then(Value::as_bool) == Some(true);
    let capabilities = value.pointer("/environment/capabilities");
    ServerSettings {
        ignored_file_listing: flag(capabilities.and_then(|c| c.get("ignoredFileListing"))),
        base_directory: value
            .pointer("/settings/addProjectBaseDirectory")
            .and_then(Value::as_str)
            .map(str::to_owned),
        ghq_layout: flag(value.pointer("/settings/ghqCloneLayout")),
    }
}

/// What reading one file yields: its text, or why it cannot be copied.
pub fn decode_read(value: &Value) -> Result<String, String> {
    if value.get("truncated").and_then(Value::as_bool) == Some(true) {
        return Err("Too large to copy.".to_owned());
    }
    value
        .get("contents")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "The server sent no contents.".to_owned())
}

/// The clone's folder on the target, from `projectClone.start`.
pub fn decode_started(value: &Value) -> Option<String> {
    value.get("cwd")?.as_str().map(str::to_owned)
}

/// Where a clone stands, as `waitForClone` reads the clone list: it is over once the clone is
/// done, or gone after it was seen. A failed or cancelled clone keeps waiting, since the web
/// composer offers a retry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloneWait {
    Pending,
    Done,
    Gone,
}

pub fn clone_wait(list: &Value, project_id: &str, seen: &mut bool) -> CloneWait {
    let clone = list.as_array().and_then(|clones| {
        clones
            .iter()
            .find(|c| c.get("projectId").and_then(Value::as_str) == Some(project_id))
    });
    if clone.is_some() {
        *seen = true;
    }
    match clone {
        Some(clone) if clone.get("phase").and_then(Value::as_str) == Some("done") => {
            CloneWait::Done
        }
        None if *seen => CloneWait::Gone,
        _ => CloneWait::Pending,
    }
}

/// The notice for files that did not make it across.
pub fn failure_title(count: usize, target: &str) -> String {
    format!(
        "Could not copy {count} {} to {target}",
        if count == 1 { "file" } else { "files" }
    )
}

/// The clone button's label.
pub fn submit_label(submitting: bool, target: Option<&str>) -> String {
    if submitting {
        "Starting\u{2026}".to_owned()
    } else {
        format!("Clone to {}", target.unwrap_or("machine"))
    }
}

/// The count under the arrow: "no files", "+ 1 file", "+ 3 files".
pub fn file_count_label(count: usize) -> String {
    match count {
        0 => "no files".to_owned(),
        1 => "+ 1 file".to_owned(),
        count => format!("+ {count} files"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, byte_length: u64) -> IgnoredFile {
        IgnoredFile {
            path: path.to_owned(),
            byte_length,
        }
    }

    #[test]
    fn config_yields_the_capability_and_clone_settings() {
        let config = json!({
            "environment": { "capabilities": { "ignoredFileListing": true } },
            "settings": { "addProjectBaseDirectory": "~/code", "ghqCloneLayout": true },
        });
        assert_eq!(
            decode_config(&config),
            ServerSettings {
                ignored_file_listing: true,
                base_directory: Some("~/code".to_owned()),
                ghq_layout: true,
            }
        );
        assert_eq!(decode_config(&json!({})), ServerSettings::default());
    }

    #[test]
    fn protocols_are_read_from_each_url_shape() {
        assert_eq!(
            url_protocol("git@host.test:acme/app.git"),
            Some(Protocol::Ssh)
        );
        assert_eq!(
            url_protocol("ssh://git@host.test/acme/app.git"),
            Some(Protocol::Ssh)
        );
        assert_eq!(
            url_protocol("https://host.test/acme/app.git"),
            Some(Protocol::Https)
        );
        assert_eq!(
            url_protocol("http://user:pw@host.test/acme/app"),
            Some(Protocol::Https)
        );
        assert_eq!(url_protocol("host.test:acme/app"), Some(Protocol::Ssh));
        assert_eq!(url_protocol("file:///srv/app"), None);
        assert_eq!(url_protocol("not a url"), None);
    }

    #[test]
    fn urls_rewrite_between_protocols() {
        let https = "https://host.test/acme/app.git";
        let ssh = "git@host.test:acme/app.git";
        assert_eq!(url_for_protocol(https, Protocol::Ssh).as_deref(), Some(ssh));
        assert_eq!(
            url_for_protocol(ssh, Protocol::Https).as_deref(),
            Some(https)
        );
        assert_eq!(
            url_for_protocol(&format!(" {https} "), Protocol::Https).as_deref(),
            Some(https)
        );
        assert_eq!(
            url_for_protocol("ssh://git@host.test/acme/app", Protocol::Https).as_deref(),
            Some("https://host.test/acme/app")
        );
    }

    #[test]
    fn a_custom_port_cannot_change_protocol() {
        assert_eq!(
            url_for_protocol("ssh://git@host.test:2222/acme/app", Protocol::Https),
            None
        );
        assert_eq!(
            url_for_protocol("https://host.test:8443/acme/app", Protocol::Ssh),
            None
        );
        assert_eq!(
            url_for_protocol("ssh://git@host.test:2222/acme/app", Protocol::Ssh).as_deref(),
            Some("ssh://git@host.test:2222/acme/app")
        );
    }

    #[test]
    fn azure_urls_move_their_repository_marker() {
        assert_eq!(
            url_for_protocol("git@ssh.dev.azure.com:v3/org/proj/repo", Protocol::Https).as_deref(),
            Some("https://dev.azure.com/org/proj/_git/repo")
        );
        assert_eq!(
            url_for_protocol("https://dev.azure.com/org/proj/_git/repo", Protocol::Ssh).as_deref(),
            Some("git@ssh.dev.azure.com:v3/org/proj/repo")
        );
        assert_eq!(
            url_for_protocol("https://dev.azure.com/org/proj", Protocol::Ssh),
            None
        );
    }

    #[test]
    fn ghq_layout_uses_host_owner_and_repository() {
        assert_eq!(
            repository_path("git@host.test:acme/app.git").as_deref(),
            Some("host.test/acme/app")
        );
        assert_eq!(
            repository_path("https://host.test/acme/app?x=1").as_deref(),
            Some("host.test/acme/app")
        );
        assert_eq!(repository_path("C:\\work\\app"), None);
        assert_eq!(repository_path("https://host.test/acme/../app"), None);
        assert_eq!(repository_path("https://host.test/acme/nul"), None);
        assert_eq!(repository_path("https://host.test/acme/app."), None);
    }

    #[test]
    fn the_clone_folder_is_the_repository_name() {
        assert_eq!(directory_name("https://host.test/acme/app.git"), "app");
        assert_eq!(directory_name("git@host.test:acme/app.git"), "app");
        assert_eq!(directory_name("acme/app"), "app");
        assert_eq!(directory_name("https://host.test/app"), "app");
        assert_eq!(directory_name("https://host.test"), "");
        assert_eq!(directory_name("git@host.test:22"), "");
        assert_eq!(directory_name(""), "");
    }

    #[test]
    fn the_clone_folder_follows_the_layout() {
        let url = "https://host.test/acme/app.git";
        assert_eq!(directory_path(url, false), "app");
        assert_eq!(directory_path(url, true), "host.test/acme/app");
        assert_eq!(directory_path("acme/app", true), "app");
    }

    #[test]
    fn the_destination_joins_base_and_folder() {
        assert_eq!(initial_query(None), "~/");
        assert_eq!(initial_query(Some("  ")), "~/");
        assert_eq!(initial_query(Some("/srv/code")), "/srv/code/");
        assert_eq!(initial_query(Some("C:\\code")), "C:\\code\\");
        assert_eq!(destination_path("~/", "app"), "~/app");
        assert_eq!(
            destination_path("/srv/code", "host/acme/app"),
            "/srv/code/host/acme/app"
        );
        assert_eq!(
            destination_path("C:\\code", "host/app"),
            "C:\\code\\host\\app"
        );
        assert_eq!(destination_path("/srv/code", " "), "/srv/code");
    }

    #[test]
    fn env_files_are_copied_by_default() {
        for path in [".env", "apps/web/.env.local", ".envrc", "pkg/.dev.vars"] {
            assert!(is_default_copied(path), "{path}");
        }
        for path in [
            ".env.",
            "env",
            ".environment",
            "node/.envrc.bak",
            "a/.dev.vars.x",
        ] {
            assert!(!is_default_copied(path), "{path}");
        }
    }

    #[test]
    fn selection_defaults_skip_files_over_the_cap() {
        let files = [
            file(".env", 10),
            file(".env.big", COPYABLE_FILE_MAX_BYTES + 1),
            file("notes.txt", 5),
        ];
        let selected = default_selection(&files);
        assert_eq!(selected, HashSet::from([".env".to_owned()]));
        let all = toggled_all(&selected, &files);
        assert_eq!(all.len(), 2);
        assert!(all_selected(&all, &files));
        assert!(toggled_all(&all, &files).is_empty());
        assert!(!all_selected(&selected, &files));
    }

    #[test]
    fn sizes_read_like_the_web() {
        assert_eq!(format_bytes(599), "599 B");
        assert_eq!(format_bytes(8192), "8.00 KB");
        assert_eq!(format_bytes(1023 * 1024 + 700), "1.00 MB");
        assert_eq!(format_bytes(150 * 1024), "150 KB");
        assert_eq!(
            size_label(&file("a", COPYABLE_FILE_MAX_BYTES + 1)),
            "Over 1 MB"
        );
        assert_eq!(size_label(&file("a", 599)), "599 B");
    }

    #[test]
    fn a_project_below_the_repository_root_keeps_its_subdirectory() {
        assert_eq!(
            project_subdirectory(Some("/repo/"), "/repo/apps/web"),
            "apps/web"
        );
        assert_eq!(project_subdirectory(Some("/repo"), "/repo"), "");
        assert_eq!(project_subdirectory(Some("/repo"), "/repository/x"), "");
        assert_eq!(project_subdirectory(None, "/repo/x"), "");
        assert_eq!(
            project_subdirectory(Some("C:\\repo"), "C:\\repo\\apps\\web"),
            "apps/web"
        );
        assert_eq!(copy_target("apps/web", ".env"), "apps/web/.env");
        assert_eq!(copy_target("", ".env"), ".env");
    }

    #[test]
    fn typing_keeps_the_text_and_a_paste_replaces_it() {
        assert_eq!(resolve_pasted_path("~/co", "~/cod"), "~/cod");
        assert_eq!(resolve_pasted_path("~/cod", "~/co"), "~/co");
        assert_eq!(
            resolve_pasted_path("~/code/", "~/code/'/srv/a b'"),
            "/srv/a b"
        );
        assert_eq!(resolve_pasted_path("~/code", "/srv/a\\ b"), "/srv/a b");
        assert_eq!(resolve_pasted_path("~/code/", "~/code//srv/x"), "/srv/x");
        assert_eq!(resolve_pasted_path("~/code", "plain text"), "plain text");
        assert_eq!(resolve_pasted_path("x", "C:\\a\\ b"), "C:\\a\\ b");
    }

    #[test]
    fn requests_carry_the_web_payloads() {
        assert_eq!(
            read_payload("/r", "a/.env"),
            json!({"cwd": "/r", "relativePath": "a/.env"})
        );
        assert_eq!(
            write_payload("/t", ".env", "A=1"),
            json!({"cwd": "/t", "relativePath": ".env", "contents": "A=1"})
        );
        assert_eq!(
            start_payload("p", "App", "2026-01-01T00:00:00.000Z", "u", "/t"),
            json!({"projectId": "p", "title": "App", "createdAt": "2026-01-01T00:00:00.000Z", "remoteUrl": "u", "destinationPath": "/t"})
        );
    }

    #[test]
    fn answers_decode_or_say_why_not() {
        let listed = decode_ignored_files(&json!({
            "files": [{"path": ".env", "byteLength": 4}],
            "truncated": true,
        }))
        .unwrap();
        assert_eq!(listed.files, vec![file(".env", 4)]);
        assert!(listed.truncated);
        assert_eq!(decode_ignored_files(&json!({"files": [{"path": 1}]})), None);
        assert_eq!(
            decode_read(&json!({"contents": "A=1", "truncated": false})),
            Ok("A=1".to_owned())
        );
        assert_eq!(
            decode_read(&json!({"contents": "A", "truncated": true})),
            Err("Too large to copy.".to_owned())
        );
        assert_eq!(decode_started(&json!({"cwd": "/t"})).as_deref(), Some("/t"));
    }

    #[test]
    fn the_wait_ends_when_the_clone_is_done_or_gone() {
        let mut seen = false;
        assert_eq!(clone_wait(&json!([]), "p", &mut seen), CloneWait::Pending);
        let running = json!([{"projectId": "p", "phase": "running"}]);
        assert_eq!(clone_wait(&running, "p", &mut seen), CloneWait::Pending);
        let failed = json!([{"projectId": "p", "phase": "failed"}]);
        assert_eq!(clone_wait(&failed, "p", &mut seen), CloneWait::Pending);
        assert_eq!(clone_wait(&json!([]), "p", &mut seen), CloneWait::Gone);
        let done = json!([{"projectId": "p", "phase": "done"}]);
        assert_eq!(clone_wait(&done, "p", &mut false), CloneWait::Done);
    }

    #[test]
    fn labels_follow_the_counts() {
        assert_eq!(file_count_label(0), "no files");
        assert_eq!(file_count_label(1), "+ 1 file");
        assert_eq!(file_count_label(4), "+ 4 files");
        assert_eq!(failure_title(1, "Box"), "Could not copy 1 file to Box");
        assert_eq!(failure_title(2, "Box"), "Could not copy 2 files to Box");
        assert_eq!(submit_label(true, Some("Box")), "Starting\u{2026}");
        assert_eq!(submit_label(false, Some("Box")), "Clone to Box");
        assert_eq!(submit_label(false, None), "Clone to machine");
    }
}
