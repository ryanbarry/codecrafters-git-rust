use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

use clap::Parser;
use flate2::read::ZlibDecoder;
use sha1::{digest::FixedOutput, Digest, Sha1};

mod cli;

use cli::{Args, Commands};

fn main() -> ExitCode {
    let ret_not_impl: ExitCode = ExitCode::from(1);
    let ret_invalid_objsha: ExitCode = ExitCode::from(128);

    let cli = Args::parse();

    match cli.command {
        Commands::Init => {
            std::fs::create_dir(".git").unwrap();
            std::fs::create_dir(".git/objects").unwrap();
            std::fs::create_dir(".git/refs").unwrap();
            std::fs::write(".git/HEAD", "ref: refs/heads/master\n").unwrap();
            println!("Initialized git directory");
            ExitCode::SUCCESS
        }
        Commands::CatFile {
            pretty_print,
            obj_sha,
        } => {
            if pretty_print == false {
                println!("cat-file without pretty-print not implemented");
                return ret_not_impl;
            }
            if is_plausibly_obj_sha(&obj_sha) {
                let p = obj_path_from_sha(&obj_sha);
                if let Ok(blobfile) = File::open(p) {
                    let (_objtype, _objsz, mut reader) = object_decoder(blobfile);
                    if std::io::copy(&mut reader, &mut std::io::stdout()).is_err() {
                        ExitCode::FAILURE
                    } else {
                        ExitCode::SUCCESS
                    }
                } else {
                    ret_invalid_objsha
                }
            } else {
                println!("fatal: Not a valid object name {}", obj_sha);
                ret_invalid_objsha
            }
        }
        Commands::HashObject {
            write: _,
            file: inputfile,
        } => match File::open(inputfile) {
            Ok(mut inputfile) => {
                let mut hasher = Sha1::new_with_prefix("blob ");
                hasher.update(inputfile.metadata().unwrap().len().to_string());
                hasher.update([0u8]);
                let mut buf = [0u8; 1024];
                let mut bytes_read = inputfile.read(&mut buf).expect("no trouble reading file");
                while bytes_read > 0 {
                    hasher.update(&buf[..bytes_read]);
                    bytes_read = inputfile.read(&mut buf).expect("no trouble reading file");
                }

                let hex_hash = hex::encode(hasher.finalize_fixed());

                println!("{}", hex_hash);
                ExitCode::SUCCESS
            }
            Err(e) => {
                println!("{}", e);
                ExitCode::FAILURE
            }
        },
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
    Blob,
    Commit,
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
            brzdf
                .read_until(0u8, &mut objsz)
                .expect("object has >5 bytes");
            objsz.pop(); // remove terminating null byte before parsing
            let objsz = usize::from_str(&String::from_utf8(objsz).unwrap())
                .expect("blob header concludes with object len");

            (ObjType::Blob, objsz, brzdf)
        }
        b"tree " => (ObjType::Tree, 0, brzdf),
        b"commi" => (ObjType::Commit, 0, brzdf),
        _ => (ObjType::Blob, 0, brzdf),
    }
}
