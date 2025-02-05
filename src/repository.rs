use anyhow::Result;
use std::{
    env,
    fs::{create_dir, read_to_string},
    path::{Path, PathBuf},
};

pub struct Repository {
    pub path: PathBuf,
    pub ignore: Vec<String>,
}

pub fn default_init_path() -> PathBuf {
    env::var("REPO_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

impl Repository {
    pub fn new() -> Result<Repository> {
        let path = default_init_path();

        let mut repo = Repository {
            path,
            ignore: Vec::new(),
        };

        repo.load_ignore()?;

        Ok(repo)
    }

    fn load_ignore(&mut self) -> Result<bool> {
        let ignore_path = self.path.join(".gitignore");
        if !ignore_path.exists() {
            return Ok(false);
        }

        let ignore_content = read_to_string(ignore_path)?;
        self.ignore = ignore_content.lines().map(String::from).collect();

        Ok(true)
    }

    pub fn init_repository(&mut self, path: &Path) -> Result<PathBuf> {
        self.path = path.to_path_buf();
        let git_dir = self.path.join(".git");

        create_dir(&git_dir)?;
        create_dir(git_dir.join("objects"))?;
        create_dir(git_dir.join("refs"))?;

        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n")?;

        Ok(self.path.clone())
    }
}
