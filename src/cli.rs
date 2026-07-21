//! Command-line argument parsing for `ccform`.
//!
//! Subcommands and flags mirror Terraform's naming (`plan`/`apply`,
//! `-y`/`--auto-approve`) so Terraform users need no new vocabulary.

use clap::{Args, Parser, Subcommand};

/// Terraform-style declarative manager for Claude Code settings via a Lua DSL.
#[derive(Debug, Parser)]
#[command(name = "ccform", version)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Command,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum Command {
    /// Creates ccform.lua and state.json from the current settings.
    Init(InitArgs),
    /// Shows the changes `apply` would make, without writing anything.
    Plan(PlanArgs),
    /// Applies the desired configuration to settings.json and mcpServers.
    Apply(ApplyArgs),
    /// Prints the state recorded by the last successful `apply`.
    Show(ShowArgs),
    /// Pulls settings not yet declared in ccform.lua into the DSL.
    Import(ImportArgs),
}

/// `ccform init` arguments.
#[derive(Debug, PartialEq, Args)]
pub struct InitArgs {
    /// Overwrites ccform.lua and state.json if they already exist.
    #[arg(long = "force")]
    pub force: bool,
}

/// `ccform plan` arguments.
#[derive(Debug, PartialEq, Args)]
pub struct PlanArgs {
    /// Prints the plan as JSON instead of human-readable text.
    #[arg(short = 'j', long = "json")]
    pub json: bool,
}

/// `ccform apply` arguments.
#[derive(Debug, PartialEq, Args)]
pub struct ApplyArgs {
    /// Skips the interactive confirmation prompt.
    #[arg(short = 'y', long = "auto-approve")]
    pub auto_approve: bool,
}

/// `ccform show` arguments.
#[derive(Debug, PartialEq, Args)]
pub struct ShowArgs {}

/// `ccform import` arguments.
#[derive(Debug, PartialEq, Args)]
pub struct ImportArgs {}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::init_default(&["ccform", "init"], Command::Init(InitArgs { force: false }))]
    #[case::init_force_long(&["ccform", "init", "--force"], Command::Init(InitArgs { force: true }))]
    #[case::plan_default(&["ccform", "plan"], Command::Plan(PlanArgs { json: false }))]
    #[case::plan_json_short(&["ccform", "plan", "-j"], Command::Plan(PlanArgs { json: true }))]
    #[case::plan_json_long(&["ccform", "plan", "--json"], Command::Plan(PlanArgs { json: true }))]
    #[case::apply_default(&["ccform", "apply"], Command::Apply(ApplyArgs { auto_approve: false }))]
    #[case::apply_auto_approve_short(&["ccform", "apply", "-y"], Command::Apply(ApplyArgs { auto_approve: true }))]
    #[case::apply_auto_approve_long(&["ccform", "apply", "--auto-approve"], Command::Apply(ApplyArgs { auto_approve: true }))]
    #[case::show(&["ccform", "show"], Command::Show(ShowArgs {}))]
    #[case::import(&["ccform", "import"], Command::Import(ImportArgs {}))]
    fn test_parses_subcommands_and_flags(#[case] argv: &[&str], #[case] expected: Command) {
        let cli = Cli::try_parse_from(argv).unwrap();

        assert_eq!(cli.cmd, expected);
    }

    #[rstest]
    fn test_unknown_subcommand_fails_with_exit_code_2() {
        let err = Cli::try_parse_from(["ccform", "bogus"]).unwrap_err();

        assert_eq!(err.exit_code(), 2);
    }

    #[rstest]
    fn test_missing_subcommand_fails_with_exit_code_2() {
        let err = Cli::try_parse_from(["ccform"]).unwrap_err();

        assert_eq!(err.exit_code(), 2);
    }

    #[rstest]
    #[case::top_level_help(&["ccform", "--help"])]
    #[case::top_level_version(&["ccform", "--version"])]
    #[case::init_help(&["ccform", "init", "--help"])]
    #[case::plan_help(&["ccform", "plan", "--help"])]
    #[case::apply_help(&["ccform", "apply", "--help"])]
    #[case::show_help(&["ccform", "show", "--help"])]
    #[case::import_help(&["ccform", "import", "--help"])]
    fn test_help_and_version_exit_with_code_0(#[case] argv: &[&str]) {
        let err = Cli::try_parse_from(argv).unwrap_err();

        assert_eq!(err.exit_code(), 0);
    }
}
