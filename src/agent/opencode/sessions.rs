use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags};

use super::super::SessionInfo;

/// Discovery only needs the newest session per directory, so keep the V2 scan
/// bounded: a long-lived store holds thousands of rows and a Mission Control
/// refresh must not turn into an unbounded read.
const SCAN_LIMIT: i64 = 500;

pub(in crate::agent) fn base() -> PathBuf {
    let home = super::super::home();
    let candidates = [
        std::env::var_os("XDG_DATA_HOME")
            .map(|directory| PathBuf::from(directory).join("opencode").join("storage")),
        Some(
            home.join(".local")
                .join("share")
                .join("opencode")
                .join("storage"),
        ),
        Some(home.join(".opencode").join("storage")),
    ];
    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return candidate.clone();
        }
    }
    home.join(".local")
        .join("share")
        .join("opencode")
        .join("storage")
}

/// OpenCode V2 keeps sessions in one SQLite database beside the V1 storage
/// tree. Usage reads the same file, so the path is resolved in one place.
pub(in crate::agent) fn database(base: &Path) -> PathBuf {
    base.parent().unwrap_or(base).join("opencode.db")
}

/// Open the store read-only. A missing, locked, or incompatible database is a
/// normal state (V1 installs have none), so every failure degrades to `None`.
pub(in crate::agent) fn open(database: &Path) -> Option<Connection> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_URI;
    let connection = Connection::open_with_flags(database, flags).ok()?;
    let _ = connection.busy_timeout(Duration::from_millis(25));
    Some(connection)
}

fn from_millis(value: i64) -> SystemTime {
    u64::try_from(value)
        .ok()
        .and_then(|millis| UNIX_EPOCH.checked_add(Duration::from_millis(millis)))
        .unwrap_or(UNIX_EPOCH)
}

/// Root sessions recorded by OpenCode V2. Subagent rows carry a `parent_id` and
/// are deliberately skipped: they are not independently resumable and would
/// otherwise mask the pane's own session for the same directory.
fn v2_sessions(database: &Path) -> Vec<SessionInfo> {
    let Some(connection) = open(database) else {
        return Vec::new();
    };
    let Ok(mut statement) = connection.prepare(
        "SELECT id, directory, time_updated FROM session_v2 \
         WHERE parent_id IS NULL AND time_archived IS NULL \
         ORDER BY time_updated DESC LIMIT ?1",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = statement.query_map([SCAN_LIMIT], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
        ))
    }) else {
        return Vec::new();
    };
    rows.flatten()
        .filter(|(id, directory, _)| !id.is_empty() && !directory.is_empty())
        .map(|(id, directory, updated)| SessionInfo {
            agent: "opencode".to_string(),
            session_id: id,
            cwd: PathBuf::from(directory),
            updated: from_millis(updated),
        })
        .collect()
}

fn session_files(base: &Path) -> Vec<(SystemTime, PathBuf)> {
    let mut output = Vec::new();
    for subdirectory in ["session", "session-metadata"] {
        let Ok(projects) = std::fs::read_dir(base.join(subdirectory)) else {
            continue;
        };
        for project in projects.flatten() {
            let Ok(files) = std::fs::read_dir(project.path()) else {
                continue;
            };
            for file in files.flatten() {
                let path = file.path();
                if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(modified) = file.metadata().and_then(|metadata| metadata.modified()) {
                    output.push((modified, path));
                }
            }
        }
    }
    output
}

fn read_session(path: &Path) -> Option<(String, PathBuf)> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let id = value.get("id").and_then(serde_json::Value::as_str)?;
    let directory = value.get("directory").and_then(serde_json::Value::as_str)?;
    Some((id.to_string(), PathBuf::from(directory)))
}

/// Sessions recorded by OpenCode V1 as one JSON file each.
fn v1_sessions(base: &Path) -> Vec<SessionInfo> {
    session_files(base)
        .into_iter()
        .filter_map(|(updated, path)| {
            read_session(&path).map(|(id, cwd)| SessionInfo {
                agent: "opencode".to_string(),
                session_id: id,
                cwd,
                updated,
            })
        })
        .collect()
}

/// Every session both stores know about, newest first. An upgraded install
/// keeps its V1 tree beside the V2 database, so neither store may hide the
/// other; the newest row for a directory wins.
fn sessions(base: &Path) -> Vec<SessionInfo> {
    let mut found = v2_sessions(&database(base));
    found.extend(v1_sessions(base));
    found.sort_by_key(|session| std::cmp::Reverse(session.updated));
    found
}

pub(in crate::agent) fn recent(base: &Path, limit: usize) -> Vec<SessionInfo> {
    let mut found = sessions(base);
    let mut seen = std::collections::HashSet::new();
    found.retain(|session| seen.insert(session.cwd.clone()));
    found.truncate(limit);
    found
}

pub(in crate::agent) fn latest(base: &Path, cwd: &Path) -> Option<String> {
    sessions(base)
        .into_iter()
        .find(|session| session.cwd == cwd)
        .map(|session| session.session_id)
}
