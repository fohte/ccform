//! End-to-end checks for `ccform show` against a temporary `$HOME` and XDG
//! state directory, so the real user's state.json is never touched.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use indoc::indoc;
use rstest::{fixture, rstest};
use tempfile::TempDir;

// See the equivalent comment in tests/init.rs: this file is already only
// built for `cargo test`, but clippy's `allow-unwrap-in-tests` only
// recognizes `unwrap()` as test code when it sits under `#[cfg(test)]`.
#[cfg(test)]
struct Env {
    home: TempDir,
    config: TempDir,
    state: TempDir,
}

#[cfg(test)]
impl Env {
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ccform"))
            .args(args)
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.config.path())
            .env("XDG_STATE_HOME", self.state.path())
            .output()
            .unwrap()
    }

    fn state_path(&self) -> PathBuf {
        self.state.path().join("ccform").join("state.json")
    }

    fn write_state(&self, contents: &str) {
        let path = self.state_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
}

#[cfg(test)]
fn new_env() -> Env {
    Env {
        home: tempfile::tempdir().unwrap(),
        config: tempfile::tempdir().unwrap(),
        state: tempfile::tempdir().unwrap(),
    }
}

#[fixture]
fn env() -> Env {
    new_env()
}

#[rstest]
fn prints_state_json_pretty_printed_when_it_exists(env: Env) {
    env.write_state(
        r#"{"version":1,"settings":{"model":"opus"},"mcpServers":{"foo":{"command":"bar"}}}"#,
    );

    let output = env.run(&["show"]);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        indoc! {r#"
            {
              "version": 1,
              "settings": {
                "model": "opus"
              },
              "mcpServers": {
                "foo": {
                  "command": "bar"
                }
              }
            }
        "#}
    );
    assert_eq!(output.stderr, Vec::<u8>::new());
}

#[rstest]
fn fails_with_exit_code_1_when_state_is_missing(env: Env) {
    let output = env.run(&["show"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, Vec::<u8>::new());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "Error: {} not found. Run `ccform init` and `ccform apply` first.\n",
            env.state_path().display()
        )
    );
}
