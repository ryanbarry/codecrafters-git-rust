use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Init,
    CatFile {
        #[arg(short, help = "pretty-print <object> content")]
        pretty_print: bool,
        #[arg()]
        obj_sha: String,
    },
}
