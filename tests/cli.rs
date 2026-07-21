//! End-to-end checks that the compiled `ccform` binary exits with the codes
//! clap's derive parser produces for usage errors, help, and version.

use std::process::Command;

use rstest::rstest;

fn ccform(args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ccform"));
    cmd.args(args);
    cmd
}

#[rstest]
#[case::unknown_subcommand(&["bogus"], 2)]
#[case::missing_subcommand(&[], 2)]
#[case::top_level_help(&["--help"], 0)]
#[case::top_level_version(&["--version"], 0)]
#[case::subcommand_help(&["apply", "--help"], 0)]
fn test_exit_code(#[case] args: &[&str], #[case] expected_code: i32) {
    let status = ccform(args).status().unwrap();

    assert_eq!(status.code(), Some(expected_code));
}
