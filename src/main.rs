use std::fs::File;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use flate2::read::ZlibDecoder;

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
        #[arg()]
        blob_sha: String,
    },
}

fn main() -> ExitCode {
    let ret_not_impl: ExitCode = ExitCode::from(1);
    let ret_invalid_blobsha: ExitCode = ExitCode::from(128);

    let cli = Cli::parse();

    match &cli.command {
        Commands::Init => {
            std::fs::create_dir(".git").unwrap();
            std::fs::create_dir(".git/objects").unwrap();
            std::fs::create_dir(".git/refs").unwrap();
            std::fs::write(".git/HEAD", "ref: refs/heads/master\n").unwrap();
            println!("Initialized git directory");
            return ExitCode::SUCCESS;
        }
        Commands::CatFile {
            pretty_print,
            blob_sha,
        } => {
            if !pretty_print {
                println!("cat-file without pretty-print not implemented");
                return ret_not_impl;
            }
            if is_plausibly_blob_sha(blob_sha) {
                let p = blob_path_from_sha(blob_sha);
                if let Ok(blobfile) = File::open(p) {
                    let mut z = ZlibDecoder::new(&blobfile);
                    std::io::copy(&mut z, &mut std::io::stdout()).expect("failed to read blob");
                    return ExitCode::SUCCESS;
                } else {
                    return ret_invalid_blobsha;
                }
            } else {
                println!("fatal: Not a valid object name {}", blob_sha);
                return ret_invalid_blobsha;
            }
        }
    }
}

fn is_plausibly_blob_sha(maybe_blob_sha: &str) -> bool {
    maybe_blob_sha.len() == 40 && maybe_blob_sha.chars().all(|c| c.is_ascii_hexdigit())
}

fn blob_path_from_sha(blob_sha: &str) -> PathBuf {
    let (obj_dirname, obj_filename) = blob_sha.split_at(2);
    [".git", "objects", obj_dirname, obj_filename]
        .iter()
        .collect()
}
