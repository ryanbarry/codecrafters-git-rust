use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

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
        obj_sha: String,
    },
}

fn main() -> ExitCode {
    let ret_not_impl: ExitCode = ExitCode::from(1);
    let ret_invalid_objsha: ExitCode = ExitCode::from(128);

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
            obj_sha,
        } => {
            if !pretty_print {
                println!("cat-file without pretty-print not implemented");
                return ret_not_impl;
            }
            if is_plausibly_obj_sha(obj_sha) {
                let p = obj_path_from_sha(obj_sha);
                if let Ok(blobfile) = File::open(p) {
                    let (objtype, objsz, mut reader) = object_decoder(blobfile);
                    std::io::copy(&mut reader, &mut std::io::stdout());
                    return ExitCode::SUCCESS;
                } else {
                    return ret_invalid_objsha;
                }
            } else {
                println!("fatal: Not a valid object name {}", obj_sha);
                return ret_invalid_objsha;
            }
        }
    }
}

fn is_plausibly_obj_sha(maybe_obj_sha: &str) -> bool {
    maybe_obj_sha.len() == 40 && maybe_obj_sha.chars().all(|c| c.is_ascii_hexdigit())
}

fn obj_path_from_sha(obj_sha: &str) -> PathBuf {
    let (obj_dirname, obj_filename) = obj_sha.split_at(2);
    [".git", "objects", obj_dirname, obj_filename]
        .iter()
        .collect()
}

enum ObjType {
    BLOB,
    COMMIT,
    Tree,
}

fn object_decoder(object: File) -> (ObjType, usize, BufReader<ZlibDecoder<File>>) {
    let mut z = ZlibDecoder::new(object);

    let mut magic = [0u8; 5];
    if let Err(e) = z.read_exact(&mut magic) {
        panic!("{}", e); // TODO
    }
    let mut brzdf = BufReader::new(z);
    let mut objsz = vec![];
    match &magic {
        b"blob " => {
            brzdf.read_until(0u8, &mut objsz);
            objsz.pop(); // remove terminating null byte before parsing
            let objsz = usize::from_str(&String::from_utf8(objsz).unwrap()).expect("blob header concludes with object len");

            return (ObjType::BLOB, objsz, brzdf);
        }
        b"tree " => {
            return (ObjType::Tree, 0, brzdf);
        }
        b"commi" => {
            return (ObjType::COMMIT, 0, brzdf);
        }
        _ => {
            return (ObjType::BLOB, 0, brzdf);
        }
    }
}
