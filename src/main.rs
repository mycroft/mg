use anyhow::{Error, Result};
use repository::default_init_path;
use std::path::PathBuf;

use clap::Parser;
use clap::Subcommand;

mod commit;
mod error;
mod index;
mod kind;
mod log;
mod object;
mod pack;
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
    /// Commit current changes
    Commit {
        /// The commit message
        message: String,
    },
    /// Get the current branch
    Branch,
    /// Get the latest commit
    Show {
        /// The commit to show
        hash: Option<String>,
    },
    /// Show the commit log
    Log,
    /// List the index entries
    LsIndex,
    /// Write the index file
    WriteIndex,
    /// Dump Pack Files
    DumpPackFiles,
    /// Dump Pack Index file
    DumpPackIndexFile {
        /// The pack index file to dump
        pack_id: String,
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
        Command::Commit { message } => match repo.commit(&message) {
            Ok(hash) => println!("{}", hex::encode(hash)),
            Err(e) => eprintln!("Failed to commit: {}", e),
        },
        Command::Branch => match repo.current_branch() {
            Ok(branch) => println!("{}", branch),
            Err(e) => eprintln!("Failed to get branch: {}", e),
        },
        Command::Show { hash } => match repo.show(hash) {
            Ok(_) => (),
            Err(e) => eprintln!("Failed to show: {}", e),
        },
        Command::Log => match repo.log() {
            Ok(_) => (),
            Err(e) => eprintln!("Failed to show log: {}", e),
        },
        Command::LsIndex => match repo.read_index() {
            Ok(_) => (),
            Err(e) => eprintln!("Failed to list index: {}", e),
        },
        Command::WriteIndex => match repo.write_index() {
            Ok(_) => (),
            Err(e) => eprintln!("Failed to write index: {}", e),
        },
        Command::DumpPackFiles => match repo.dump_pack_files() {
            Ok(_) => (),
            Err(e) => eprintln!("Failed to dump pack files: {}", e),
        },
        Command::DumpPackIndexFile { pack_id } => match repo.dump_pack_index_file(&pack_id) {
            Ok(_) => (),
            Err(e) => eprintln!("Failed to dump pack index file: {}", e),
        },
    }

    Ok(())
}
