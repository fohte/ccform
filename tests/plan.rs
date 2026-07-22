//! End-to-end checks for `ccform plan` against a temporary `$HOME` and XDG
//! directories, so the real user's `~/.claude/settings.json` /
//! `~/.config/ccform/ccform.lua` are never touched.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use indoc::indoc;
use rstest::{fixture, rstest};
use serde_json::json;
use tempfile::TempDir;

// `#[cfg(test)]` below isn't conditional compilation — this whole file is
// already only built for `cargo test` — it's there so clippy's
// `allow-unwrap-in-tests` recognizes the `unwrap()` calls as test code: that
// check looks for the attribute on an enclosing item, and rstest's
// `#[fixture]` macro regenerates the function it wraps and drops any
// attribute it doesn't itself recognize, which is why `new_env` is a plain
// function the `env` fixture delegates to rather than being annotated
// directly.
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

    fn write_ccform_lua(&self, contents: &str) {
        let dir = self.config.path().join("ccform");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("ccform.lua"), contents).unwrap();
    }

    fn write_settings(&self, contents: &str) {
        let dir = self.home.path().join(".claude");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("settings.json"), contents).unwrap();
    }

    fn write_claude_json(&self, contents: &str) {
        fs::write(self.home.path().join(".claude.json"), contents).unwrap();
    }

    fn write_state(&self, contents: &str) {
        let dir = self.state.path().join("ccform");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("state.json"), contents).unwrap();
    }

    fn entry_path(&self) -> PathBuf {
        self.config.path().join("ccform").join("ccform.lua")
    }

    fn settings_path(&self) -> PathBuf {
        self.home.path().join(".claude").join("settings.json")
    }

    fn claude_json_path(&self) -> PathBuf {
        self.home.path().join(".claude.json")
    }

    fn state_path(&self) -> PathBuf {
        self.state.path().join("ccform").join("state.json")
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

// A Lua runtime error's `Display` embeds the erroring chunk's file path and
// truncates it with a leading `...` once it exceeds Lua's internal chunk-id
// length limit. The default `tempfile::tempdir()` base (e.g. macOS's
// `/var/folders/...`) is long enough to trigger that truncation, so the one
// test asserting on the exact Lua error text below needs paths short enough
// to stay under the limit.
#[cfg(test)]
fn new_short_path_env() -> Env {
    let short_dir = || tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    Env {
        home: short_dir(),
        config: short_dir(),
        state: short_dir(),
    }
}

#[fixture]
fn short_path_env() -> Env {
    new_short_path_env()
}

#[cfg(test)]
fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

#[cfg(test)]
fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

#[rstest]
#[case::no_changes(
    "return { settings = { model = 'opus' } }",
    r#"{"model":"opus"}"#,
    Some(r#"{"version":1,"settings":{"model":"opus"},"mcpServers":{}}"#),
    "Plan: 0 to add, 0 to change, 0 to remove.\n"
)]
#[case::plan_changes_across_settings_and_mcp_servers(
    indoc! {"
        return {
          settings = { model = 'opus' },
          mcpServers = { foo = { command = 'bar' } },
        }
    "},
    r#"{"model":"sonnet"}"#,
    None,
    indoc! {r#"
        Plan: 1 to add, 1 to change, 0 to remove.

        settings:
          ~ /model = "sonnet" -> "opus"

        mcpServers:
          + /foo = {"command":"bar"}
    "#}
)]
#[case::drift_from_state(
    "return { settings = { model = 'sonnet' } }",
    r#"{"model":"sonnet"}"#,
    Some(r#"{"version":1,"settings":{"model":"opus"},"mcpServers":{}}"#),
    indoc! {r#"
        Drift detected (0 to add, 1 to change, 0 to remove):

        settings:
          ~ /model = "opus" -> "sonnet"

        Plan: 0 to add, 0 to change, 0 to remove.
    "#}
)]
fn plan_renders_the_expected_text(
    env: Env,
    #[case] ccform_lua: &str,
    #[case] settings_json: &str,
    #[case] state_json: Option<&str>,
    #[case] expected_stdout: &str,
) {
    env.write_ccform_lua(ccform_lua);
    env.write_settings(settings_json);
    if let Some(state_json) = state_json {
        env.write_state(state_json);
    }

    let output = env.run(&["plan"]);

    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (Some(0), expected_stdout.to_string(), String::new())
    );
}

#[rstest]
fn prints_json_when_the_json_flag_is_set(env: Env) {
    env.write_ccform_lua("return { settings = { model = 'opus' } }");
    env.write_settings(r#"{"model":"sonnet"}"#);

    let output = env.run(&["plan", "--json"]);

    assert_eq!(
        (
            output.status.code(),
            serde_json::from_str::<serde_json::Value>(&stdout(&output)).unwrap(),
            stderr(&output)
        ),
        (
            Some(0),
            json!({
                "settings": {
                    "plan": [
                        {"path": "/model", "kind": "replace", "before": "sonnet", "after": "opus"}
                    ],
                    "drift": [],
                    "import_candidates": [],
                },
                "mcpServers": {
                    "plan": [],
                    "drift": [],
                    "import_candidates": [],
                },
            }),
            String::new()
        )
    );
}

#[rstest]
fn does_not_write_to_settings_json_claude_json_or_state_json(env: Env) {
    env.write_ccform_lua(indoc! {"
        return {
          settings = { model = 'opus' },
          mcpServers = { foo = { command = 'bar' } },
        }
    "});
    env.write_settings(r#"{"model":"sonnet"}"#);
    env.write_claude_json(r#"{"mcpServers":{"old":{"command":"old"}}}"#);

    let output = env.run(&["plan"]);

    assert_eq!(
        (
            output.status.code(),
            fs::read_to_string(env.settings_path()).unwrap(),
            fs::read_to_string(env.claude_json_path()).unwrap(),
            env.state_path().exists()
        ),
        (
            Some(0),
            r#"{"model":"sonnet"}"#.to_string(),
            r#"{"mcpServers":{"old":{"command":"old"}}}"#.to_string(),
            false
        )
    );
}

#[rstest]
fn exits_non_zero_with_the_lua_error_and_traceback_on_evaluation_failure(short_path_env: Env) {
    short_path_env.write_ccform_lua("error('boom')\n");

    let output = short_path_env.run(&["plan"]);

    let path = short_path_env.entry_path().display().to_string();
    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (
            Some(1),
            String::new(),
            format!(
                "Error: runtime error: {path}:1: boom\nstack traceback:\n\t[C]: in ?\n\t[C]: in function 'error'\n\t{path}:1: in main chunk\n"
            )
        )
    );
}
