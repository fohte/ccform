use ccform::cli::{ApplyArgs, Cli, Command, ImportArgs, InitArgs, PlanArgs, ShowArgs};
use ccform::command;
use clap::Parser;

fn main() {
    let cli = Cli::parse();

    let result = match cli.cmd {
        Command::Init(args) => run_init(args),
        Command::Plan(args) => run_plan(args),
        Command::Apply(args) => run_apply(args),
        Command::Show(args) => run_show(args),
        Command::Import(args) => run_import(args),
    };

    if let Err(err) = result {
        eprintln!("Error: {err:#}");
        std::process::exit(exit_code(&err));
    }
}

/// Maps a top-level error to its process exit code, defaulting to 1 (general
/// error) except for errors that document a specific one.
fn exit_code(err: &anyhow::Error) -> i32 {
    match err.downcast_ref::<command::init::Error>() {
        Some(command::init::Error::AlreadyExists { .. }) => 3,
        _ => 1,
    }
}

fn run_init(args: InitArgs) -> anyhow::Result<()> {
    command::init::run(args.force)?;
    Ok(())
}

fn run_plan(_args: PlanArgs) -> anyhow::Result<()> {
    todo!()
}

fn run_apply(_args: ApplyArgs) -> anyhow::Result<()> {
    todo!()
}

fn run_show(_args: ShowArgs) -> anyhow::Result<()> {
    todo!()
}

fn run_import(_args: ImportArgs) -> anyhow::Result<()> {
    command::import::run()?;
    Ok(())
}
