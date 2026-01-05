use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    LspServer {},
}

impl Default for Commands {
    fn default() -> Self {
        Self::LspServer {}
    }
}

fn main() {
    let mut cli = Cli::parse();
    let subcommand = cli.command.take().unwrap_or_default();
    println!("{cli:?}");
}
