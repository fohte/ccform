//! End-to-end checks for `ccform show` against a temporary `$HOME` and XDG
//! state directory, so the real user's state.json is never touched.

use std::fs;

use indoc::indoc;
use rstest::rstest;

mod common;
use common::{Env, env};

// See the equivalent comment in tests/common/mod.rs for why this needs
// `#[cfg(test)]` despite the whole file already being test-only.
#[cfg(test)]
impl Env {
    fn write_state(&self, contents: &str) {
        let path = self.state_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
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
            "Error: {} not found. Run `ccform init` first.\n",
            env.state_path().display()
        )
    );
}
