//! Shared harness for CLI integration tests: a fully isolated `$HOME` + XDG
//! environment so `ccform` never touches the real one, regardless of which
//! of these paths a given subcommand's test actually exercises.

use std::path::PathBuf;
use std::process::{Command, Output};

use rstest::fixture;
use tempfile::TempDir;

// `#[cfg(test)]` below isn't conditional compilation — this whole file is
// already only built for `cargo test` — it's there so clippy's
// `allow-unwrap-in-tests` recognizes the `unwrap()` calls as test code:
// that check looks for the attribute on an enclosing item, and rstest's
// `#[fixture]` macro regenerates the function it wraps and drops any
// attribute it doesn't itself recognize, which is why `new_env` is a plain
// function the `env` fixture delegates to rather than being annotated
// directly.
#[cfg(test)]
pub(crate) struct Env {
    pub(crate) home: TempDir,
    pub(crate) config: TempDir,
    pub(crate) state: TempDir,
}

#[cfg(test)]
impl Env {
    pub(crate) fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ccform"))
            .args(args)
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.config.path())
            .env("XDG_STATE_HOME", self.state.path())
            .output()
            .unwrap()
    }

    pub(crate) fn state_path(&self) -> PathBuf {
        self.state.path().join("ccform").join("state.json")
    }
}

#[cfg(test)]
pub(crate) fn new_env() -> Env {
    Env {
        home: tempfile::tempdir().unwrap(),
        config: tempfile::tempdir().unwrap(),
        state: tempfile::tempdir().unwrap(),
    }
}

#[fixture]
pub(crate) fn env() -> Env {
    new_env()
}
