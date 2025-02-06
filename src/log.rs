use crate::repository::Repository;

use anyhow::Result;
use hex::FromHex;

impl Repository {
    pub fn log(&self) -> Result<()> {
        let mut current_commit = self.current_commit()?;

        loop {
            let mut commit = self.read_object(&hex::encode(current_commit))?;

            let commit_desc = commit.string()?;
            let lines = commit_desc.lines().collect::<Vec<&str>>();

            // find the first empty line
            let first_empty_line = lines.iter().position(|line| line.is_empty());

            println!(
                "{} {}",
                hex::encode(current_commit),
                lines[first_empty_line.unwrap() + 1]
            );

            let parent_commit_id = lines.iter().find(|line| line.starts_with("parent "));
            if parent_commit_id.is_none() {
                break;
            }

            let parent_commit_id = parent_commit_id.unwrap();
            current_commit = <[u8; 20]>::from_hex(parent_commit_id.split_once(' ').unwrap().1)?;
        }

        Ok(())
    }
}
