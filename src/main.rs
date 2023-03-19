use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::str::FromStr;

use anyhow::{ensure, Context, Result};
use clap::Parser;
use flate2::{read::ZlibDecoder, write::ZlibEncoder};

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
            if !pretty_print {
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
            write: do_write,
            file: infilepath,
        } => match File::open(infilepath) {
            Ok(mut infilepath) => {
                let hash = hash_file(&infilepath);
                let hex_hash = hex::encode(hash);

                if do_write {
                    let obj_db_path = obj_path_from_sha(&hex_hash);

                    if !obj_db_path.exists() {
                        infilepath
                            .rewind()
                            .expect("start reading given file from beginning to copy into obj db");

                        if let Err(e) = encode_object(&mut infilepath, obj_db_path) {
                            println!("Error writing object to database:\n{}", e);
                            return ExitCode::FAILURE;
                        }
                    }
                }

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

fn hash_file(mut f: &File) -> [u8; 20] {
    use sha1::{Digest, Sha1};

    let mut hasher = Sha1::new_with_prefix("blob ");
    let filesz = f.metadata().unwrap().len();
    hasher.update(filesz.to_string());
    hasher.update([0u8]);
    let mut buf = [0u8; 1024];
    let mut bytes_read = f.read(&mut buf).expect("read given file for hashing");
    while bytes_read > 0 {
        hasher.update(&buf[..bytes_read]);
        bytes_read = f.read(&mut buf).expect("read given file for hashing");
    }

    let mut h = hasher.finalize();
    *h.as_mut()
}

fn encode_object<P: AsRef<Path>>(input: &mut File, obj_db_path: P) -> Result<()> {
    let obj_db_path = obj_db_path.as_ref();
    let filesz = input
        .metadata()
        .context("get input file metadata, for size")?
        .len();

    let obj_db_dir = obj_db_path.parent().with_context(|| {
        format!(
            "object path doesn't have two-char dir preceding filename: {}",
            obj_db_path.to_string_lossy()
        )
    })?;

    if obj_db_dir.exists() {
        ensure!(
            obj_db_dir.is_dir(),
            "object database should only have directories at top level"
        );
    } else {
        std::fs::create_dir(obj_db_dir).context("creating prefix dir in obj db")?;
    }

    let outputfile = OpenOptions::new()
        .create(true)
        .write(true)
        .open(obj_db_path)
        .context("Failed to open file for writing object to db")?;

    let header = format!("blob {}\0", filesz);

    let mut compressedout = ZlibEncoder::new(outputfile, flate2::Compression::default());

    compressedout
        .write_all(header.as_bytes())
        .context("write header to object file in db")?;

    std::io::copy(input, &mut compressedout)
        .context("copying given file's contents to object in db")?;

    {
        // set file read-only (i.e. 0400) once it's been written, as og impl does
        let outputfile = compressedout.get_ref();
        let mut perms = outputfile
            .metadata()
            .context("getting db obj file metadata, after writing")?
            .permissions();
        perms.set_readonly(true);
        outputfile
            .set_permissions(perms)
            .context("setting permissions on db obj file after writing")?;
    }
    Ok(())
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
