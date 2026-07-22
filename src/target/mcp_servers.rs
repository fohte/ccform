//! Partial-update target for the `mcpServers` key of `~/.claude.json`.
//!
//! Claude Code itself writes OAuth session state and per-project state into
//! `~/.claude.json` alongside `mcpServers`. Those other keys are not
//! ccform's to manage, so a write here must round-trip them byte-for-byte,
//! including their read order.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::io::atomic;

const KEY: &str = "mcpServers";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to read {path}")]
    Read { path: PathBuf, source: io::Error },

    #[error("failed to parse JSON from {path}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("expected {path} to contain a JSON object at the top level, found {found}")]
    NotAnObject { path: PathBuf, found: &'static str },

    #[error(transparent)]
    Write(#[from] atomic::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Reads and writes the `mcpServers` key of a `~/.claude.json`-shaped file,
/// leaving every other key — and their read order — untouched.
pub struct McpServers {
    path: PathBuf,
}

impl McpServers {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Targets the user's real `~/.claude.json`, resolved via
    /// `paths::claude_json_path()`.
    pub fn from_home() -> Self {
        Self::new(crate::paths::claude_json_path())
    }

    /// The current `mcpServers` value, or `Value::Null` if the file or the
    /// key does not exist.
    pub fn read(&self) -> Result<Value> {
        let root = read_object(&self.path)?;
        Ok(root.get(KEY).cloned().unwrap_or(Value::Null))
    }

    /// Same as [`read`](Self::read), but reports a missing file or key as an
    /// empty object instead of `Value::Null` — the shape callers that treat
    /// "absent" and "explicitly empty" the same way (`init`, `plan`, `apply`)
    /// actually want.
    pub fn read_or_empty(&self) -> Result<Value> {
        let value = self.read()?;
        Ok(if value.is_null() {
            Value::Object(Map::new())
        } else {
            value
        })
    }

    /// Replaces the `mcpServers` key with `desired`, preserving every other
    /// key and the file's existing key order. Creates the file with just
    /// `{"mcpServers": desired}` if it does not exist yet.
    ///
    /// This is a read-modify-write with no exclusive lock across the two
    /// steps: a concurrent write to `~/.claude.json` by Claude Code itself
    /// (e.g. an OAuth refresh) between the read and the rename here is not
    /// detected and its other-key changes are overwritten. Only the rename
    /// used to land this write is atomic, matching the crash-safety
    /// guarantee `io::atomic::write_json` provides; it is not mutual
    /// exclusion against other writers of the file.
    pub fn write(&self, desired: &Value) -> Result<()> {
        let mut root = read_object(&self.path)?;
        root.insert(KEY.to_string(), desired.clone());
        atomic::write_json(&self.path, &Value::Object(root))?;
        Ok(())
    }
}

/// Reads `path` as a JSON object, or an empty object if `path` does not
/// exist yet — the "bootstrap" case for both `read` and `write`.
fn read_object(path: &Path) -> Result<Map<String, Value>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(source) => {
            return Err(Error::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    };

    let value: Value = serde_json::from_slice(&bytes).map_err(|source| Error::Parse {
        path: path.to_path_buf(),
        source,
    })?;

    match value {
        Value::Object(map) => Ok(map),
        other => Err(Error::NotAnObject {
            path: path.to_path_buf(),
            found: type_name(&other),
        }),
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use rstest::{fixture, rstest};
    use serde_json::json;

    use super::*;

    #[fixture]
    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[rstest]
    fn write_creates_file_with_only_mcp_servers_when_missing(dir: tempfile::TempDir) {
        let path = dir.path().join(".claude.json");
        let target = McpServers::new(path.clone());

        target.write(&json!({"a": {"command": "foo"}})).unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            indoc! {r#"
                {
                  "mcpServers": {
                    "a": {
                      "command": "foo"
                    }
                  }
                }"#}
        );
    }

    #[rstest]
    fn write_replaces_mcp_servers_while_preserving_other_keys_and_order(dir: tempfile::TempDir) {
        let path = dir.path().join(".claude.json");
        fs::write(
            &path,
            indoc! {r#"
                {
                  "oauthAccount": {
                    "email": "user@example.com"
                  },
                  "mcpServers": {
                    "old": {
                      "command": "old"
                    }
                  },
                  "projects": {
                    "/foo": {}
                  }
                }"#},
        )
        .unwrap();
        let target = McpServers::new(path.clone());

        target.write(&json!({"new": {"command": "new"}})).unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            indoc! {r#"
                {
                  "oauthAccount": {
                    "email": "user@example.com"
                  },
                  "mcpServers": {
                    "new": {
                      "command": "new"
                    }
                  },
                  "projects": {
                    "/foo": {}
                  }
                }"#}
        );
    }

    #[rstest]
    #[case::file_missing(None, Value::Null)]
    #[case::key_missing(
        Some(r#"{"oauthAccount": {"email": "user@example.com"}}"#),
        Value::Null
    )]
    #[case::key_present(
        Some(r#"{"mcpServers": {"a": 1}, "other": true}"#),
        json!({"a": 1})
    )]
    fn read_returns_expected_value(
        dir: tempfile::TempDir,
        #[case] initial_content: Option<&str>,
        #[case] expected: Value,
    ) {
        let path = dir.path().join(".claude.json");
        if let Some(content) = initial_content {
            fs::write(&path, content).unwrap();
        }
        let target = McpServers::new(path);

        assert_eq!(target.read().unwrap(), expected);
    }

    #[rstest]
    #[case::file_missing(None, json!({}))]
    #[case::key_missing(
        Some(r#"{"oauthAccount": {"email": "user@example.com"}}"#),
        json!({})
    )]
    #[case::key_present(
        Some(r#"{"mcpServers": {"a": 1}, "other": true}"#),
        json!({"a": 1})
    )]
    fn read_or_empty_normalizes_absent_to_an_empty_object(
        dir: tempfile::TempDir,
        #[case] initial_content: Option<&str>,
        #[case] expected: Value,
    ) {
        let path = dir.path().join(".claude.json");
        if let Some(content) = initial_content {
            fs::write(&path, content).unwrap();
        }
        let target = McpServers::new(path);

        assert_eq!(target.read_or_empty().unwrap(), expected);
    }

    #[rstest]
    fn read_fails_on_malformed_json(dir: tempfile::TempDir) {
        let path = dir.path().join(".claude.json");
        fs::write(&path, "not json").unwrap();
        let target = McpServers::new(path);

        assert!(matches!(target.read().unwrap_err(), Error::Parse { .. }));
    }

    #[rstest]
    fn read_fails_when_top_level_is_not_an_object(dir: tempfile::TempDir) {
        let path = dir.path().join(".claude.json");
        fs::write(&path, "[1, 2, 3]").unwrap();
        let target = McpServers::new(path);

        assert!(matches!(
            target.read().unwrap_err(),
            Error::NotAnObject { found: "array", .. }
        ));
    }
}
