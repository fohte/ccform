use ccform::cli::{ApplyArgs, Cli, Command, ImportArgs, InitArgs, PlanArgs, ShowArgs};
use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Command::Init(args) => run_init(args),
        Command::Plan(args) => run_plan(args),
        Command::Apply(args) => run_apply(args),
        Command::Show(args) => run_show(args),
        Command::Import(args) => run_import(args),
    }
}

fn run_init(_args: InitArgs) -> anyhow::Result<()> {
    todo!()
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
    todo!()
}
