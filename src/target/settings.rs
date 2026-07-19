//! Reads and writes `~/.claude/settings.json` as a whole. ccform treats this
//! file as fully owned: `write` replaces its entire contents rather than
//! patching individual keys.

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use serde_json::{Map, Value};

use crate::io::atomic;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to read {path}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse {path} as JSON")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error(transparent)]
    Write(#[from] atomic::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct Settings {
    path: PathBuf,
}

impl Settings {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Reads the file as JSON. A missing file is treated as an empty object
    /// rather than an error, since this file may not exist yet the first
    /// time ccform runs.
    pub fn read(&self) -> Result<Value> {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|source| Error::Parse {
                path: self.path.clone(),
                source,
            }),
            Err(source) if source.kind() == ErrorKind::NotFound => Ok(Value::Object(Map::new())),
            Err(source) => Err(Error::Read {
                path: self.path.clone(),
                source,
            }),
        }
    }

    /// Writes `value` atomically via [`atomic::write_json`], sorting object
    /// keys alphabetically at every nesting level first so the file's key
    /// order is stable across writes regardless of the caller's key order.
    pub fn write(&self, value: &Value) -> Result<()> {
        let mut sorted = value.clone();
        sort_keys(&mut sorted);
        atomic::write_json(&self.path, &sorted)?;
        Ok(())
    }
}

fn sort_keys(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for v in map.values_mut() {
                sort_keys(v);
            }
            map.sort_keys();
        }
        Value::Array(items) => {
            for v in items.iter_mut() {
                sort_keys(v);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use rstest::{fixture, rstest};
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;

    #[fixture]
    fn dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    fn mode_of(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    fn tmp_like_entries(dir: &Path) -> Vec<PathBuf> {
        let mut entries: Vec<PathBuf> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.contains(".tmp."))
            })
            .collect();
        entries.sort();
        entries
    }

    #[rstest]
    fn read_treats_missing_file_as_empty_object(dir: TempDir) {
        let settings = Settings::new(dir.path().join("settings.json"));

        assert_eq!(settings.read().unwrap(), json!({}));
    }

    #[rstest]
    fn read_parses_existing_file_contents(dir: TempDir) {
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            br#"{"model":"opus","permissions":{"allow":["Bash(ls:*)"]}}"#,
        )
        .unwrap();
        let settings = Settings::new(path);

        assert_eq!(
            settings.read().unwrap(),
            json!({
                "model": "opus",
                "permissions": {"allow": ["Bash(ls:*)"]},
            })
        );
    }

    #[rstest]
    fn write_creates_file_with_alphabetically_sorted_keys_and_0600_permissions(dir: TempDir) {
        let path = dir.path().join("settings.json");
        let settings = Settings::new(path.clone());

        settings
            .write(&json!({
                "permissions": {"deny": [], "allow": ["Bash(ls:*)"]},
                "model": "opus",
            }))
            .unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            indoc::indoc! {r#"
                {
                  "model": "opus",
                  "permissions": {
                    "allow": [
                      "Bash(ls:*)"
                    ],
                    "deny": []
                  }
                }"#}
        );
        assert_eq!(mode_of(&path), 0o600);
        assert_eq!(tmp_like_entries(dir.path()), Vec::<PathBuf>::new());
    }

    #[rstest]
    fn write_replaces_an_existing_file_via_atomic_rename(dir: TempDir) {
        let path = dir.path().join("settings.json");
        fs::write(&path, br#"{"old":true}"#).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let settings = Settings::new(path.clone());

        settings.write(&json!({"new": true})).unwrap();

        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(),
            json!({"new": true})
        );
        assert_eq!(mode_of(&path), 0o600);
        assert_eq!(tmp_like_entries(dir.path()), Vec::<PathBuf>::new());
    }

    #[rstest]
    fn write_leaves_existing_file_and_its_permissions_untouched_on_failure(dir: TempDir) {
        let path = dir.path().join("settings.json");
        fs::write(&path, br#"{"old":true}"#).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let settings = Settings::new(path.clone());
        // Remove write permission on the directory so creating the tmp file fails,
        // simulating a write that is interrupted before the rename happens.
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();

        let result = settings.write(&json!({"new": true}));

        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        assert!(matches!(
            result,
            Err(Error::Write(atomic::Error::CreateTempFile { .. }))
        ));
        assert_eq!(fs::read(&path).unwrap(), br#"{"old":true}"#);
        assert_eq!(mode_of(&path), 0o644);
    }
}
