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
    HashObject {
        #[arg(short, help = "write the object into the object database")]
        write: bool,
        #[arg()]
        file: String,
    },
    LsTree {
        #[arg(long, help = "list only filenames")]
        name_only: bool,
        #[arg(value_name = "tree-ish")]
        tree_ish: String,
    },
}
