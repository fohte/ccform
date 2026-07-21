//! End-to-end checks that `fn main` wires `Cli::parse()` up correctly: the
//! process actually exits with the codes clap's derive parser produces,
//! rather than that behavior being caught or altered on the way out.

use std::process::Command;

use rstest::rstest;

fn ccform(args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ccform"));
    cmd.args(args);
    cmd
}

#[rstest]
#[case::unknown_subcommand(&["bogus"])]
#[case::missing_subcommand(&[])]
fn test_invalid_arguments_exit_with_usage_error(#[case] args: &[&str]) {
    let status = ccform(args).status().unwrap();

    assert_eq!(status.code(), Some(2));
}

#[rstest]
#[case::top_level_help(&["--help"])]
#[case::top_level_version(&["--version"])]
#[case::subcommand_help(&["apply", "--help"])]
fn test_help_and_version_exit_successfully(#[case] args: &[&str]) {
    let status = ccform(args).status().unwrap();

    assert_eq!(status.code(), Some(0));
}
