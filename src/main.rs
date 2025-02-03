use anyhow::{Error, Result};
use std::{fs, path::PathBuf};

fn init_repository(path: PathBuf) -> Result<PathBuf> {
    let git_dir = path.join(".git");

    fs::create_dir(&git_dir)?;
    fs::create_dir(git_dir.join("objects"))?;
    fs::create_dir(git_dir.join("refs"))?;

    fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n")?;

    Ok(path)
}

fn main() -> Result<(), Error> {
    let mut current_dir = std::env::current_dir()?;
    let env_dir = std::env::var("REPO_PATH");

    if let Ok(path) = env_dir {
        current_dir = PathBuf::from(path);
    }

    let repository = init_repository(current_dir)?;

    Ok(())
}
