use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::str::FromStr;

use anyhow::{ensure, Context, Result};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use clap::Parser;
use flate2::{read::ZlibDecoder, write::ZlibEncoder};

mod cli;

use cli::{Args, Commands};

fn main() -> ExitCode {
    let ret_not_impl: ExitCode = ExitCode::from(1);
    let ret_invalid_objsha: ExitCode = ExitCode::from(128);
    let ret_bad_file = ExitCode::from(128);

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
        } => match hash_object(&infilepath, do_write) {
            Ok(hash) => {
                let hex_hash = hex::encode(hash);
                println!("{}", hex_hash);
                ExitCode::SUCCESS
            }
            Err(e) => {
                println!("error: {}", e);
                ExitCode::FAILURE
            }
        },
        Commands::LsTree {
            name_only,
            tree_ish,
        } => {
            let obj_path = obj_path_from_sha(&tree_ish);
            if let Ok(objfile) = File::open(obj_path) {
                match object_decoder(objfile) {
                    (ObjType::Tree, _objsz, mut reader) => {
                        let mut tree_ents: Vec<TreeEntry> = vec![];
                        let mut pnbuf = vec![];
                        loop {
                            let mode: TreeObjMode;
                            let otype: ObjType;

                            match reader.read_until(b' ', &mut pnbuf) {
                                Ok(0) => {
                                    // EOF
                                    break;
                                }
                                Ok(_nbytes) => {
                                    mode = TreeObjMode::from(&pnbuf);
                                    otype = match mode {
                                        TreeObjMode::Directory => ObjType::Tree,
                                        TreeObjMode::RegularFile
                                        | TreeObjMode::ExecutableFile
                                        | TreeObjMode::Link => ObjType::Blob,
                                    };
                                }
                                Err(e) => {
                                    panic!("failed to read next tree entry up to the NUL separator before its sha: {}", e);
                                }
                            };
                            pnbuf.clear();

                            let name: String = match reader.read_until(b'\0', &mut pnbuf) {
                                Ok(0) => {
                                    // EOF
                                    break;
                                }
                                Ok(_nbytes) => {
                                    pnbuf.pop();
                                    String::from_utf8_lossy(&pnbuf).into()
                                }
                                Err(e) => {
                                    panic!("failed to read the name after the tree entry's permissions: {}", e);
                                }
                            };
                            pnbuf.clear();

                            let mut hash = [0u8; 20];
                            reader
                                .read_exact(&mut hash)
                                .expect("20 bytes after mode+name for the hash");

                            let ent = TreeEntry {
                                mode,
                                otype,
                                name,
                                hash,
                            };
                            tree_ents.push(ent);
                        }

                        if name_only {
                            for ent in tree_ents {
                                println!("{}", ent.name);
                            }
                        } else {
                            for ent in tree_ents {
                                println!("{}", ent);
                            }
                        }
                    }
                    (objt, _, _) => {
                        println!("fatal: not a tree object (found {})", objt.type_name());
                        return ret_bad_file;
                    }
                }
            }

            ExitCode::FAILURE
        }
        Commands::WriteTree => {
            let cur_dir = std::env::current_dir().expect("read cwd");
            let git_dir = {
                let mut d = cur_dir.clone();
                d.push(".git");
                d
            };
            assert!(
                git_dir.exists() && git_dir.is_dir(),
                "expect to be run in directory with .git"
            );

            fn write_tree_recursive(path: &Path) -> Vec<TreeEntry> {
                let mut res = vec![];
                let sorted_dirents = {
                    let mut dirents: Vec<std::fs::DirEntry> =
                        path.read_dir().unwrap().map(|re| re.unwrap()).collect();
                    dirents.sort_by_key(|enta| enta.file_name());
                    dirents
                };
                for ent in sorted_dirents {
                    if ent.file_name() == ".git" {
                        continue;
                    }
                    let ent = ent.path();
                    let entry_type: ObjType;
                    let entry_mode: TreeObjMode;
                    let entry_hash: [u8; 20];
                    if ent.is_dir() {
                        let tree = write_tree_recursive(&ent);
                        entry_hash = hash_tree(tree).expect("to hash every entry");
                        entry_type = ObjType::Tree;
                        entry_mode = TreeObjMode::Directory;
                    } else {
                        entry_hash = hash_object(&ent, true).expect("to hash every entry");
                        entry_type = ObjType::Blob;
                        entry_mode = TreeObjMode::RegularFile;
                    }
                    res.push(TreeEntry {
                        name: ent
                            .file_name()
                            .unwrap_or_else(|| {
                                panic!(
                                    "entry `{}` has a file name since it isn't a dir",
                                    ent.to_string_lossy()
                                )
                            })
                            .to_string_lossy()
                            .to_string(),
                        hash: entry_hash,
                        mode: entry_mode,
                        otype: entry_type,
                    })
                }
                res
            }

            let tree = write_tree_recursive(&cur_dir);
            let hash = hash_tree(tree).expect("to insert a tree object for the current dir");

            println!("{}", hex::encode(hash));

            ExitCode::SUCCESS
        }
    }
}

fn hash_object<P: AsRef<Path>>(path: P, do_write: bool) -> Result<[u8; 20]> {
    let mut infile = File::open(path).context("opening file for hashing")?;
    let hash = hash_file(&infile)?;
    let hex_hash = hex::encode(hash);

    if do_write {
        let obj_db_path = obj_path_from_sha(&hex_hash);

        if !obj_db_path.exists() {
            infile
                .rewind()
                .expect("start reading given file from beginning to copy into obj db");

            let filesz = infile
                .metadata()
                .context("get input file metadata, for size")?
                .len();

            encode_object(ObjType::Blob, &mut infile, filesz, obj_db_path)
                .context("encoding object into database")?;
        }
    }

    Ok(hash)
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

fn hash_file(mut f: &File) -> Result<[u8; 20]> {
    use sha1::{Digest, Sha1};

    let mut hasher = Sha1::new_with_prefix("blob ");
    let filesz = f.metadata().unwrap().len();
    hasher.update(filesz.to_string());
    hasher.update([0u8]);
    let mut buf = [0u8; 1024];
    let mut bytes_read = f.read(&mut buf).context("read given file for hashing")?;
    while bytes_read > 0 {
        hasher.update(&buf[..bytes_read]);
        bytes_read = f.read(&mut buf).context("read given file for hashing")?;
    }

    let mut h = hasher.finalize();
    Ok(*h.as_mut())
}

fn hash_tree(tree: Vec<TreeEntry>) -> Result<[u8; 20]> {
    use sha1::{Digest, Sha1};

    let mut buf = BytesMut::with_capacity(tree.len() * 48);
    for ent in tree {
        buf.put_slice(&ent.mode.as_bytes());
        buf.put_u8(b' ');
        buf.put_slice(ent.name.as_bytes());
        buf.put_u8(b'\0');
        buf.put_slice(&ent.hash);
    }
    let buf = buf.freeze();
    let bufsz = buf.len();
    let bufsz_str = bufsz.to_string();

    let mut header = Vec::with_capacity(5 + bufsz_str.len() + 1);
    header.extend_from_slice(b"tree ");
    header.extend_from_slice(bufsz_str.as_bytes());
    header.push(b'\0');

    let mut hasher = Sha1::new_with_prefix(header);
    hasher.update(&buf);
    let hash = *hasher.finalize().as_mut();
    let hex_hash = hex::encode(hash);

    let obj_db_path = obj_path_from_sha(&hex_hash);
    if !obj_db_path.exists() {
        encode_object(
            ObjType::Tree,
            buf.reader(),
            bufsz.try_into().unwrap(),
            obj_db_path,
        )
        .context("encoding tree into db")?;
    }
    Ok(hash)
}

fn encode_object<P: AsRef<Path>, R: Read>(
    otype: ObjType,
    mut input: R,
    filesz: u64,
    obj_db_path: P,
) -> Result<()> {
    let obj_db_path = obj_db_path.as_ref();

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

    let header = format!("{} {}\0", otype.type_name(), filesz);

    let mut compressedout = ZlibEncoder::new(outputfile, flate2::Compression::default());

    compressedout
        .write_all(header.as_bytes())
        .context("write header to object file in db")?;

    std::io::copy(&mut input, &mut compressedout)
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

#[allow(dead_code)]
enum ObjType {
    None,
    Commit,
    Tree,
    Blob,
    Tag,
}

impl ObjType {
    fn type_name(&self) -> &'static str {
        match self {
            ObjType::Commit => "commit",
            ObjType::Tree => "tree",
            ObjType::Blob => "blob",
            ObjType::Tag => "tag",
            _ => unimplemented!("unexpected object type for type_name()"),
        }
    }
}

trait DbObj {}

//struct Blob {}

#[derive(Debug)]
enum TreeObjMode {
    Directory,
    RegularFile,
    ExecutableFile,
    Link,
}

impl TreeObjMode {
    fn from(bytes: &[u8]) -> Self {
        match bytes[0] {
            b'1' => match bytes[1] {
                b'0' => Self::RegularFile,
                b'2' => Self::Link,
                unk => {
                    unimplemented!("unknown object type: 0{:o}", unk);
                }
            },
            b'4' => Self::Directory,
            unk => {
                unimplemented!("unknown object type: {:o}", unk);
            }
        }
    }

    fn as_bytes(&self) -> Bytes {
        match &self {
            Self::RegularFile => Bytes::from_static(b"100644"),
            Self::Directory => Bytes::from_static(b"40000"),
            _ => unimplemented!(),
        }
    }
}

impl std::fmt::Display for TreeObjMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            TreeObjMode::Directory => write!(f, "040000"),
            TreeObjMode::RegularFile => write!(f, "100644"),
            omode => unimplemented!("can't display mode {:?}", omode),
        }
    }
}

struct TreeEntry {
    mode: TreeObjMode,
    otype: ObjType,
    hash: [u8; 20],
    name: String,
}

impl std::fmt::Display for TreeEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {} {}\t{}",
            self.mode,
            self.otype.type_name(),
            hex::encode(self.hash),
            self.name
        )
    }
}
//struct Commit {}
//struct Tag {}

fn object_decoder(object: File) -> (ObjType, usize, BufReader<ZlibDecoder<File>>) {
    let mut z = ZlibDecoder::new(object);

    let mut magic = [0u8; 4];
    if let Err(e) = z.read_exact(&mut magic) {
        panic!("{}", e); // TODO
    }
    let mut brzdf = BufReader::new(z);
    let mut objsz = vec![];
    match &magic {
        b"blob" => {
            brzdf
                .read_exact(&mut [0u8; 1])
                .expect("to consume space before object length in header");
            brzdf
                .read_until(0u8, &mut objsz)
                .expect("object has >5 bytes");
            objsz.pop(); // remove terminating null byte before parsing
            let objsz = usize::from_str(&String::from_utf8(objsz).unwrap())
                .expect("blob header concludes with object len");

            (ObjType::Blob, objsz, brzdf)
        }
        b"tree" => {
            brzdf
                .read_exact(&mut [0u8; 1])
                .expect("to consume space before object length in header");
            brzdf
                .read_until(0u8, &mut objsz)
                .expect("object has >5 bytes");
            objsz.pop(); // remove terminating null byte before parsing
            let objsz = usize::from_str(&String::from_utf8(objsz).unwrap())
                .expect("blob header concludes with object len");

            (ObjType::Tree, objsz, brzdf)
        }
        b"comm" => {
            brzdf
                .read_exact(&mut [0u8; 3])
                .expect("to consume \"it \" before object length in header");
            (ObjType::Commit, 0, brzdf)
        }
        b"tag " => (ObjType::Tag, 0, brzdf),
        _ => (ObjType::Blob, 0, brzdf),
    }
}
