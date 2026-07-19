//! Fixtures and helpers shared by unit tests across modules.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use rstest::fixture;
use tempfile::TempDir;

#[fixture]
pub fn dir() -> TempDir {
    tempfile::tempdir().unwrap()
}

pub fn mode_of(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}
