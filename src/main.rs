use std::fs;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Init,
    CatFile {
        #[arg(short, help = "pretty-print <object> content")]
        pretty_print: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Init => {
            fs::create_dir(".git").unwrap();
            fs::create_dir(".git/objects").unwrap();
            fs::create_dir(".git/refs").unwrap();
            fs::write(".git/HEAD", "ref: refs/heads/master\n").unwrap();
            println!("Initialized git directory")
        }
        _ => todo!(),
    }
}
