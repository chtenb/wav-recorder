extern crate anyhow;

use anyhow::Result;
use clap::Parser;

mod record;

/// Simple program to record wav files
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Optionally specify a different working directory than the current one
    #[clap(short, long, default_value("."))]
    working_dir: String,
    #[clap(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Interactively edit the configuration settings
    Start {},
}

fn main() {
    let args = Args::parse();
    match run(args) {
        Err(e) => {
            println!("{}", e);
            std::process::exit(1)
        }
        Ok(msg) => {
            println!("{}", msg);
            std::process::exit(0)
        }
    };
}

fn run(args: Args) -> Result<String> {
    match args.command {
        Command::Start {} => {
            record::main("default".to_string())?;
            Ok("finished".to_string())
        }
    }
}
