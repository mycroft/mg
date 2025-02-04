use anyhow::{Error, Result};
use std::env;
use std::{fs, path::PathBuf};

use clap::Parser;
use clap::Subcommand;

mod error;
mod kind;
mod object;

use crate::object::read_object;
use crate::object::write_object;

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
    /// Write a blob object
    WriteBlob {
        /// The file to write
        file: PathBuf,
    },
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

fn main() -> Result<(), Error> {
    let cli = Cli::parse();
    let path = default_init_path();

    match cli.command {
        Command::Init { path } => match init_repository(path) {
            Ok(path) => println!("Initialized empty Git repository in {:?}", path),
            Err(e) => eprintln!("Failed to initialize repository: {}", e),
        },
        Command::CatFile { hash } => match read_object(&path, &hash) {
            Ok(mut obj) => print!("{}", obj.string()?),
            Err(e) => eprintln!("Failed to read object: {}", e),
        },
        Command::WriteBlob { file } => match write_object(&path, &file) {
            Ok(hash) => println!("{}", hash),
            Err(e) => eprintln!("Failed to write object: {}", e),
        },
    }

    Ok(())
}
