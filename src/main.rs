use anyhow::{Context, Error, Result};
use std::env;
use std::io::prelude::*;
use std::{fs, path::PathBuf};

use clap::Parser;
use clap::Subcommand;

#[derive(Parser)]
#[command(name = "mg", about = "A simple git clone")]
struct Cli {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize a new Git repository
    Init {
        /// The path where to create the repository. Defaults to current directory
        #[arg(default_value=default_init_path().into_os_string())]
        path: PathBuf,
    },
    /// Display a Git object
    CatFile {
        /// The object to display
        hash: String,
    },
}

#[derive(thiserror::Error, Debug)]
pub enum RuntimeError {
    #[error("Invalid character found")]
    UnexpectedChar,
}

#[derive(Debug)]
enum Kind {
    Blob,    // 100644 or 100755
    Commit,  // 120000
    Tree,    // 040000
    Symlink, // 120000
}

impl Kind {
    fn from_mode(mode: &str) -> Result<Self> {
        match mode {
            "100644" | "100755" => Ok(Kind::Blob),
            "120000" => Ok(Kind::Commit),
            "040000" | "40000" => Ok(Kind::Tree),
            _ => Err(anyhow::anyhow!(format!("invalid mode: {}", mode))),
        }
    }

    fn string(&self) -> &str {
        match self {
            Kind::Blob => "blob",
            Kind::Commit => "commit",
            Kind::Tree => "tree",
            Kind::Symlink => "symlink",
        }
    }
}

#[derive(Debug)]
struct Object<Reader> {
    kind: Kind,
    size: usize,
    data: Reader,
}

#[derive(Debug)]
struct TreeObject {
    mode: String,
    kind: Kind,
    name: String,
    hash: [u8; 20],
}

fn default_init_path() -> PathBuf {
    env::var("REPO_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn init_repository(path: PathBuf) -> Result<PathBuf> {
    let git_dir = path.join(".git");

    fs::create_dir(&git_dir)?;
    fs::create_dir(git_dir.join("objects"))?;
    fs::create_dir(git_dir.join("refs"))?;

    fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n")?;

    Ok(path)
}

fn read_object(path: &PathBuf, object: &str) -> Result<Object<impl BufRead>> {
    let object_path = path
        .join(".git")
        .join("objects")
        .join(&object[..2])
        .join(&object[2..]);

    let fd = fs::File::open(&object_path).context("opening the object")?;
    let zfd = flate2::read::ZlibDecoder::new(fd);
    let mut buf_reader = std::io::BufReader::new(zfd);

    let mut buf: Vec<u8> = Vec::new();
    buf_reader
        .read_until(0, &mut buf)
        .context("read the object")?;

    match buf.pop() {
        Some(0) => {}
        Some(_) | None => return Err(RuntimeError::UnexpectedChar.into()),
    };

    let header = String::from_utf8(buf.clone()).context("converting header to utf-8")?;

    let Some((object_type, object_size)) = header.split_once(' ') else {
        anyhow::bail!("could not parse object header correctly");
    };

    let object_type = match object_type {
        "blob" => Kind::Blob,
        "commit" => Kind::Commit,
        "tree" => Kind::Tree,
        _ => anyhow::bail!("invalid object type found"),
    };

    let object_size = object_size.parse::<usize>()?;

    Ok(Object {
        kind: object_type,
        size: object_size,
        data: buf_reader,
    })
}

impl<R: BufRead> Object<R> {
    fn print(&mut self) -> Result<()> {
        let mut buf: Vec<u8> = Vec::new();
        let mut buf_hash: [u8; 20] = [0; 20];

        match self.kind {
            Kind::Blob | Kind::Commit => {
                self.data.read_to_end(&mut buf)?;
                println!("{}", String::from_utf8(buf)?);
            }
            Kind::Tree => {
                let mut max_name_len = 0;
                let mut entries = Vec::new();
                loop {
                    let read_bytes_len = self.data.read_until(0, &mut buf)?;
                    if read_bytes_len <= 0 {
                        break;
                    }

                    let mode_name = buf.clone();
                    buf.clear();
                    let mut splits = mode_name.splitn(2, |&b| b == b' ');

                    let mode = splits
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("could not parse mode"))?;
                    let mode = std::str::from_utf8(mode)?;
                    let name = splits
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("could not parse name"))?;
                    let name = std::str::from_utf8(name)?;

                    self.data.read_exact(&mut buf_hash)?;

                    if name.len() > max_name_len {
                        max_name_len = name.len();
                    }

                    entries.push(TreeObject {
                        name: name.to_string(),
                        kind: Kind::from_mode(mode)?,
                        mode: mode.to_string(),
                        hash: buf_hash.clone(),
                    });
                }

                entries.sort_by(|a, b| a.name.cmp(&b.name));

                for entry in entries {
                    let hash = hex::encode(entry.hash);
                    println!(
                        "{:0>6} {} {}    {:name_len$}",
                        entry.mode,
                        entry.kind.string(),
                        hash,
                        entry.name,
                        name_len = max_name_len
                    );
                }
            }
            _ => unimplemented!(),
        }

        Ok(())
    }
}

fn main() -> Result<(), Error> {
    let cli = Cli::parse();
    let path = default_init_path();

    match cli.command {
        Command::Init { path } => match init_repository(path) {
            Ok(path) => println!("Initialized empty Git repository in {:?}", path),
            Err(e) => eprintln!("Failed to initialize repository: {}", e),
        },
        Command::CatFile { hash } => match read_object(&path, &hash) {
            Ok(mut obj) => obj.print()?,
            Err(e) => eprintln!("Failed to read object: {}", e),
        },
    }

    Ok(())
}
