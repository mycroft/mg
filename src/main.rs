use anyhow::{Error, Result};
use repository::default_init_path;
use std::path::PathBuf;

use clap::Parser;
use clap::Subcommand;

mod error;
mod kind;
mod object;
mod repository;
mod tree;

use crate::repository::Repository;

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
    /// Write a tree object
    WriteTree {
        /// The path to write
        path: PathBuf,
    },
}

fn main() -> Result<(), Error> {
    let cli = Cli::parse();

    let mut repo = Repository::new()?;

    match cli.command {
        Command::Init { path } => match repo.init_repository(&path) {
            Ok(path) => println!("Initialized empty Git repository in {:?}", path),
            Err(e) => eprintln!("Failed to initialize repository: {}", e),
        },
        Command::CatFile { hash } => match repo.read_object(&hash) {
            Ok(mut obj) => print!("{}", obj.string()?),
            Err(e) => eprintln!("Failed to read object: {}", e),
        },
        Command::WriteBlob { file } => match repo.write_blob(&file) {
            Ok(hash) => println!("{}", hex::encode(hash)),
            Err(e) => eprintln!("Failed to write object: {}", e),
        },
        Command::WriteTree { path } => match repo.write_tree(&path) {
            Ok(hash) => println!("{}", hex::encode(hash)),
            Err(e) => eprintln!("Failed to write tree: {}", e),
        },
    }

    Ok(())
}
