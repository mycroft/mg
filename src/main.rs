use anyhow::{Error, Result};
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
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

fn init_repository(path: PathBuf) -> Result<PathBuf> {
    println!("Creating in {:?}", path);

    let git_dir = path.join(".git");

    fs::create_dir(&git_dir)?;
    fs::create_dir(git_dir.join("objects"))?;
    fs::create_dir(git_dir.join("refs"))?;

    fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n")?;

    Ok(path)
}

fn main() -> Result<(), Error> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init { path } => match init_repository(path) {
            Ok(path) => println!("Initialized empty Git repository in {:?}", path),
            Err(e) => eprintln!("Failed to initialize repository: {}", e),
        },
    }

    Ok(())
}
