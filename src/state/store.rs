//! Reads and writes `state.json`, ccform's record of the desired state
//! applied on the last successful `ccform apply`. This lets `plan`/`apply`
//! compute a 3-way diff (state vs actual vs desired) and detect drift made
//! outside of ccform.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::io::atomic;
use crate::paths;

const CURRENT_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to read {path}")]
    Read { path: PathBuf, source: io::Error },

    #[error("failed to parse {path} as JSON")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("{path} has version {found}, but this ccform only supports version {expected}")]
    VersionMismatch {
        path: PathBuf,
        found: u32,
        expected: u32,
    },

    #[error("failed to back up {path} to {backup_path}")]
    Backup {
        path: PathBuf,
        backup_path: PathBuf,
        source: io::Error,
    },

    #[error(transparent)]
    Write(#[from] atomic::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// The last desired state applied to `~/.claude/settings.json` and the
/// `mcpServers` key of `~/.claude.json`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct State {
    pub version: u32,
    pub settings: Value,
    #[serde(rename = "mcpServers")]
    pub mcp_servers: Value,
}

impl State {
    fn to_value(&self) -> Value {
        serde_json::json!({
            "version": self.version,
            "settings": self.settings,
            "mcpServers": self.mcp_servers,
        })
    }
}

/// Reads state.json from the XDG state directory. Returns `None` if it does
/// not exist yet, i.e. before the first `ccform init`.
pub fn load() -> Result<Option<State>> {
    load_from(&paths::state_path())
}

/// Writes `state` to state.json, first renaming any existing state.json to
/// state.json.backup so exactly one prior generation is kept. Creates the
/// state directory (mode 0700) if it does not exist yet.
pub fn save_with_backup(state: &State) -> Result<()> {
    save_with_backup_to(&paths::state_path(), &paths::state_backup_path(), state)
}

/// Bootstraps state.json for `ccform init`. Unlike `save_with_backup`, this
/// does not rename an existing state.json to a backup first: init is not
/// expected to run against an already-populated state.json.
pub fn initialize(initial: &State) -> Result<()> {
    initialize_at(&paths::state_path(), initial)
}

fn load_from(path: &Path) -> Result<Option<State>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(Error::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    };

    let state: State = serde_json::from_slice(&bytes).map_err(|source| Error::Parse {
        path: path.to_path_buf(),
        source,
    })?;

    if state.version != CURRENT_VERSION {
        return Err(Error::VersionMismatch {
            path: path.to_path_buf(),
            found: state.version,
            expected: CURRENT_VERSION,
        });
    }

    Ok(Some(state))
}

fn save_with_backup_to(path: &Path, backup_path: &Path, state: &State) -> Result<()> {
    ensure_state_dir(path)?;

    match fs::rename(path, backup_path) {
        Ok(()) => {}
        Err(source) if source.kind() == io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(Error::Backup {
                path: path.to_path_buf(),
                backup_path: backup_path.to_path_buf(),
                source,
            });
        }
    }

    atomic::write_json(path, &state.to_value())?;
    Ok(())
}

fn initialize_at(path: &Path, initial: &State) -> Result<()> {
    ensure_state_dir(path)?;
    atomic::write_json(path, &initial.to_value())?;
    Ok(())
}

fn ensure_state_dir(path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        atomic::ensure_dir(dir, 0o700)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::test_support::{dir, mode_of};

    fn paths_in(dir: &TempDir) -> (PathBuf, PathBuf) {
        (
            dir.path().join("state.json"),
            dir.path().join("state.json.backup"),
        )
    }

    #[rstest]
    fn load_from_returns_none_when_file_is_missing(dir: TempDir) {
        let (path, _) = paths_in(&dir);

        assert_eq!(load_from(&path).unwrap(), None);
    }

    #[rstest]
    fn load_from_returns_state_when_version_matches(dir: TempDir) {
        let (path, _) = paths_in(&dir);
        fs::write(
            &path,
            r#"{"version":1,"settings":{"model":"opus"},"mcpServers":{"a":{"command":"foo"}}}"#,
        )
        .unwrap();

        assert_eq!(
            load_from(&path).unwrap(),
            Some(State {
                version: 1,
                settings: json!({"model": "opus"}),
                mcp_servers: json!({"a": {"command": "foo"}}),
            })
        );
    }

    #[rstest]
    fn load_from_fails_when_version_does_not_match(dir: TempDir) {
        let (path, _) = paths_in(&dir);
        fs::write(&path, r#"{"version":2,"settings":{},"mcpServers":{}}"#).unwrap();

        let err = load_from(&path).unwrap_err();

        assert!(matches!(
            err,
            Error::VersionMismatch {
                found: 2,
                expected: 1,
                ..
            }
        ));
    }

    #[rstest]
    fn load_from_fails_on_malformed_json(dir: TempDir) {
        let (path, _) = paths_in(&dir);
        fs::write(&path, "not json").unwrap();

        assert!(matches!(load_from(&path).unwrap_err(), Error::Parse { .. }));
    }

    #[rstest]
    fn save_with_backup_to_creates_dir_and_file_when_nothing_exists_yet(dir: TempDir) {
        let path = dir.path().join("ccform").join("state.json");
        let backup_path = dir.path().join("ccform").join("state.json.backup");
        let state = State {
            version: 1,
            settings: json!({"model": "opus"}),
            mcp_servers: json!({}),
        };

        save_with_backup_to(&path, &backup_path, &state).unwrap();

        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(),
            json!({"version": 1, "settings": {"model": "opus"}, "mcpServers": {}})
        );
        assert!(!backup_path.exists());
        assert_eq!(mode_of(path.parent().unwrap()), 0o700);
        assert_eq!(mode_of(&path), 0o600);
    }

    #[rstest]
    fn save_with_backup_to_moves_existing_state_to_backup(dir: TempDir) {
        let (path, backup_path) = paths_in(&dir);
        fs::write(&path, r#"{"version":1,"settings":{},"mcpServers":{}}"#).unwrap();
        let new_state = State {
            version: 1,
            settings: json!({"model": "opus"}),
            mcp_servers: json!({}),
        };

        save_with_backup_to(&path, &backup_path, &new_state).unwrap();

        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(),
            json!({"version": 1, "settings": {"model": "opus"}, "mcpServers": {}})
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&backup_path).unwrap()).unwrap(),
            json!({"version": 1, "settings": {}, "mcpServers": {}})
        );
    }

    #[rstest]
    fn save_with_backup_to_overwrites_a_previous_backup(dir: TempDir) {
        let (path, backup_path) = paths_in(&dir);
        fs::write(
            &backup_path,
            r#"{"version":1,"settings":{"stale":true},"mcpServers":{}}"#,
        )
        .unwrap();
        fs::write(
            &path,
            r#"{"version":1,"settings":{"current":true},"mcpServers":{}}"#,
        )
        .unwrap();
        let new_state = State {
            version: 1,
            settings: json!({"new": true}),
            mcp_servers: json!({}),
        };

        save_with_backup_to(&path, &backup_path, &new_state).unwrap();

        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&backup_path).unwrap()).unwrap(),
            json!({"version": 1, "settings": {"current": true}, "mcpServers": {}})
        );
    }

    #[rstest]
    fn initialize_at_creates_dir_and_file(dir: TempDir) {
        let path = dir.path().join("ccform").join("state.json");
        let initial = State {
            version: 1,
            settings: json!({}),
            mcp_servers: json!({}),
        };

        initialize_at(&path, &initial).unwrap();

        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(),
            json!({"version": 1, "settings": {}, "mcpServers": {}})
        );
        assert_eq!(mode_of(path.parent().unwrap()), 0o700);
        assert_eq!(mode_of(&path), 0o600);
    }
}
