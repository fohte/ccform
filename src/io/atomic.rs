//! Atomic file writes (tmp + fsync + rename) and stale-tmp-file detection.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "not wired into the CLI yet; consumed by target::* / state::store in a later task"
    )
)]

use std::fs::{self, File};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to create temporary file {path}")]
    CreateTempFile { path: PathBuf, source: io::Error },

    #[error("failed to write temporary file {path}")]
    WriteTempFile { path: PathBuf, source: io::Error },

    #[error("failed to sync temporary file {path}")]
    SyncTempFile { path: PathBuf, source: io::Error },

    #[error("failed to set permissions on {path}")]
    SetPermissions { path: PathBuf, source: io::Error },

    #[error("failed to rename {from} to {to}")]
    Rename {
        from: PathBuf,
        to: PathBuf,
        source: io::Error,
    },

    #[error("failed to create directory {path}")]
    CreateDir { path: PathBuf, source: io::Error },

    #[error("failed to read directory {path}")]
    ReadDir { path: PathBuf, source: io::Error },

    #[error("failed to sync directory {path}")]
    SyncDir { path: PathBuf, source: io::Error },

    #[error("failed to serialize JSON for {path}")]
    SerializeJson {
        path: PathBuf,
        source: serde_json::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

static NONCE: AtomicU64 = AtomicU64::new(0);

/// Builds a tmp path in the same directory as `path`, named `{file_name}.tmp.{pid}.{nonce}`.
fn tmp_path_for(path: &Path) -> PathBuf {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let pid = std::process::id();
    let nonce = NONCE.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!("{file_name}.tmp.{pid}.{nonce}"))
}

fn write_tmp_file(tmp_path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    // `.mode()` narrows the window where the file is readable with looser,
    // umask-derived permissions, but the kernel still masks it by umask
    // (unlike chmod), so the explicit set_permissions below remains the
    // only umask-independent guarantee of the final mode.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(tmp_path)
        .map_err(|source| Error::CreateTempFile {
            path: tmp_path.to_path_buf(),
            source,
        })?;
    file.write_all(bytes)
        .map_err(|source| Error::WriteTempFile {
            path: tmp_path.to_path_buf(),
            source,
        })?;
    file.sync_all().map_err(|source| Error::SyncTempFile {
        path: tmp_path.to_path_buf(),
        source,
    })?;
    set_permissions(tmp_path, mode)
}

fn set_permissions(path: &Path, mode: u32) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|source| {
        Error::SetPermissions {
            path: path.to_path_buf(),
            source,
        }
    })
}

/// Writes `bytes` to `path` atomically: a tmp file is created in the same directory,
/// fsynced, chmod'd to `mode`, then renamed onto `path`. On any failure the tmp file
/// is best-effort removed and `path` is left untouched.
pub fn write_bytes(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let tmp_path = tmp_path_for(path);

    if let Err(err) = write_tmp_file(&tmp_path, bytes, mode) {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }

    fs::rename(&tmp_path, path).map_err(|source| {
        let _ = fs::remove_file(&tmp_path);
        Error::Rename {
            from: tmp_path.clone(),
            to: path.to_path_buf(),
            source,
        }
    })?;

    // rename(2) is atomic, but the directory entry it updates is not synced
    // to disk with it: without this, a crash right after a successful
    // rename can still resurrect the old file on remount.
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    File::open(parent)
        .and_then(|dir| dir.sync_all())
        .map_err(|source| Error::SyncDir {
            path: parent.to_path_buf(),
            source,
        })
}

/// Serializes `value` as pretty-printed JSON and writes it atomically with mode 0600.
pub fn write_json(path: &Path, value: &serde_json::Value) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|source| Error::SerializeJson {
        path: path.to_path_buf(),
        source,
    })?;
    write_bytes(path, &bytes, 0o600)
}

/// Creates `path` (and any missing parents) and sets its permissions to `mode`.
pub fn ensure_dir(path: &Path, mode: u32) -> Result<()> {
    fs::create_dir_all(path).map_err(|source| Error::CreateDir {
        path: path.to_path_buf(),
        source,
    })?;
    set_permissions(path, mode)
}

/// Returns true if `name` matches the `{file_name}.tmp.{pid}.{nonce}` convention,
/// i.e. ends with `.tmp.<digits>.<digits>`.
fn is_stale_tmp_name(name: &str) -> bool {
    let Some((_, suffix)) = name.rsplit_once(".tmp.") else {
        return false;
    };
    let Some((pid, nonce)) = suffix.split_once('.') else {
        return false;
    };
    !pid.is_empty()
        && !nonce.is_empty()
        && pid.bytes().all(|b| b.is_ascii_digit())
        && nonce.bytes().all(|b| b.is_ascii_digit())
}

/// Scans `dir` for leftover `*.tmp.<pid>.<nonce>` files (e.g. from a crash mid-write),
/// logs a warning for each to stderr, and returns their paths. Does not delete them.
/// A missing `dir` is not an error.
pub fn warn_stale_tmp_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(Error::ReadDir {
                path: dir.to_path_buf(),
                source,
            });
        }
    };

    let mut stale = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| Error::ReadDir {
            path: dir.to_path_buf(),
            source,
        })?;
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if is_stale_tmp_name(name) {
            let path = entry.path();
            eprintln!(
                "warning: stale temporary file left behind by a previous run: {}",
                path.display()
            );
            stale.push(path);
        }
    }
    stale.sort();

    Ok(stale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::{fixture, rstest};
    use tempfile::TempDir;

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
                    .is_some_and(is_stale_tmp_name)
            })
            .collect();
        entries.sort();
        entries
    }

    #[rstest]
    fn write_bytes_creates_target_and_leaves_no_tmp_file(dir: TempDir) {
        let target = dir.path().join("settings.json");

        write_bytes(&target, b"hello", 0o600).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"hello");
        assert_eq!(tmp_like_entries(dir.path()), Vec::<PathBuf>::new());
    }

    #[rstest]
    fn write_json_serializes_value_and_leaves_no_tmp_file(dir: TempDir) {
        let target = dir.path().join("state.json");
        let value = serde_json::json!({ "version": 1, "settings": {} });

        write_json(&target, &value).unwrap();

        let written: serde_json::Value =
            serde_json::from_slice(&fs::read(&target).unwrap()).unwrap();
        assert_eq!(written, value);
        assert_eq!(mode_of(&target), 0o600);
        assert_eq!(tmp_like_entries(dir.path()), Vec::<PathBuf>::new());
    }

    #[rstest]
    fn write_bytes_does_not_overwrite_target_when_rename_fails(dir: TempDir) {
        // Renaming a regular file onto an existing directory fails on POSIX.
        let target = dir.path().join("settings.json");
        fs::create_dir(&target).unwrap();

        let err = write_bytes(&target, b"new content", 0o600).unwrap_err();

        assert!(matches!(err, Error::Rename { .. }));
        assert!(fs::metadata(&target).unwrap().is_dir());
        assert_eq!(tmp_like_entries(dir.path()), Vec::<PathBuf>::new());
    }

    #[rstest]
    fn write_bytes_does_not_touch_target_when_tmp_creation_fails(dir: TempDir) {
        let target = dir.path().join("settings.json");
        fs::write(&target, b"original content").unwrap();
        // Remove write permission on the directory so creating the tmp file fails.
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();

        let result = write_bytes(&target, b"new content", 0o600);

        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        assert!(matches!(result, Err(Error::CreateTempFile { .. })));
        assert_eq!(fs::read(&target).unwrap(), b"original content");
        assert_eq!(tmp_like_entries(dir.path()), Vec::<PathBuf>::new());
    }

    #[rstest]
    #[case::owner_read_write(0o600)]
    #[case::owner_and_group_read(0o640)]
    fn write_bytes_sets_expected_file_permissions(dir: TempDir, #[case] mode: u32) {
        let target = dir.path().join("out.bin");

        write_bytes(&target, b"data", mode).unwrap();

        assert_eq!(mode_of(&target), mode);
    }

    #[rstest]
    #[case::owner_only(0o700)]
    #[case::owner_and_group(0o750)]
    fn ensure_dir_creates_directory_with_expected_permissions(dir: TempDir, #[case] mode: u32) {
        let target = dir.path().join("nested").join("ccform");

        ensure_dir(&target, mode).unwrap();

        assert!(fs::metadata(&target).unwrap().is_dir());
        assert_eq!(mode_of(&target), mode);
    }

    #[rstest]
    fn ensure_dir_is_idempotent(dir: TempDir) {
        let target = dir.path().join("ccform");

        ensure_dir(&target, 0o700).unwrap();
        ensure_dir(&target, 0o700).unwrap();

        assert_eq!(mode_of(&target), 0o700);
    }

    #[rstest]
    #[case::valid("settings.json.tmp.123.45", true)]
    #[case::no_tmp_marker("settings.json", false)]
    #[case::missing_nonce("settings.json.tmp.123", false)]
    #[case::non_numeric_pid("settings.json.tmp.abc.45", false)]
    #[case::non_numeric_nonce("settings.json.tmp.123.abc", false)]
    #[case::empty_nonce("settings.json.tmp.123.", false)]
    fn is_stale_tmp_name_matches_pid_nonce_suffix(#[case] name: &str, #[case] expected: bool) {
        assert_eq!(is_stale_tmp_name(name), expected);
    }

    #[rstest]
    fn warn_stale_tmp_files_reports_only_tmp_pattern_entries(dir: TempDir) {
        fs::write(dir.path().join("settings.json"), b"{}").unwrap();
        let stale_a = dir.path().join("settings.json.tmp.111.1");
        let stale_b = dir.path().join("state.json.tmp.222.2");
        fs::write(&stale_a, b"partial").unwrap();
        fs::write(&stale_b, b"partial").unwrap();

        let found = warn_stale_tmp_files(dir.path()).unwrap();

        let mut expected = vec![stale_a, stale_b];
        expected.sort();
        assert_eq!(found, expected);
    }

    #[rstest]
    fn warn_stale_tmp_files_on_missing_dir_returns_empty(dir: TempDir) {
        let missing = dir.path().join("does-not-exist");

        assert_eq!(
            warn_stale_tmp_files(&missing).unwrap(),
            Vec::<PathBuf>::new()
        );
    }

    #[rstest]
    fn ensure_dir_rejects_when_path_is_occupied_by_a_file(dir: TempDir) {
        let target = dir.path().join("ccform");
        fs::write(&target, b"not a directory").unwrap();

        let err = ensure_dir(&target, 0o700).unwrap_err();

        assert!(matches!(err, Error::CreateDir { .. }));
    }
}
