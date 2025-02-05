use std::fs::{read_to_string, File};

use anyhow::{Context, Result};
use hex::FromHex;

use crate::{kind::Kind, repository::Repository};

impl Repository {
    pub fn read_head(&self) -> Result<String> {
        let head_path = self.path.join(".git").join("HEAD");
        read_to_string(head_path).context("reading head")
    }

    pub fn current_branch(&self) -> Result<String> {
        let head = self.read_head()?;
        Ok(head
            .trim_start_matches("ref: refs/heads/")
            .trim_end()
            .to_string())
    }

    pub fn current_commit(&self) -> Result<[u8; 20]> {
        let current_branch = self.current_branch()?;
        let branch_path = self
            .path
            .join(".git")
            .join("refs")
            .join("heads")
            .join(&current_branch);

        let r = read_to_string(branch_path).context("could not read current branch")?;
        let r = r.trim();

        Ok(<[u8; 20]>::from_hex(r)?)
    }

    pub fn has_current_commit(&self) -> bool {
        self.current_commit().is_ok()
    }

    pub fn set_current_commit(&self, hash: &[u8; 20]) -> Result<()> {
        let current_branch = self
            .current_branch()
            .context("could not find current branch")?;

        let branch_path = self.path.join(".git").join("refs").join("heads");

        if !branch_path.exists() {
            std::fs::create_dir_all(&branch_path)?;
        }

        let branch_path = branch_path.join(&current_branch);

        // file does not exist
        if !branch_path.exists() {
            File::create(&branch_path)?;
        }

        std::fs::write(branch_path, hex::encode(hash))?;

        Ok(())
    }

    pub fn commit(&self, message: &str) -> Result<[u8; 20]> {
        let has_current_commit = self.has_current_commit();
        let mut out: Vec<u8> = Vec::new();

        let tree_hash = self
            .write_tree(&self.path)
            .context("could not write_tree")?;
        out.extend_from_slice(b"tree ");
        out.extend_from_slice(hex::encode(tree_hash).as_bytes());
        out.push(b'\n');

        if has_current_commit {
            let current_commit_id = self.current_commit()?;
            out.extend_from_slice(b"parent ");
            out.extend_from_slice(hex::encode(current_commit_id).as_bytes());
            out.push(b'\n');
        }

        out.push(b'\n');
        out.extend_from_slice(message.as_bytes());
        out.push(b'\n');

        let hash = self.write_object(Kind::Commit, &out).context("Write")?;

        // update current branch's commit id
        self.set_current_commit(&hash)?;

        Ok(hash)
    }

    pub fn show(&self, hash: Option<String>) -> Result<()> {
        let mut commit = if let Some(hash) = hash {
            self.read_object(&hash)?
        } else {
            let current_commit = self.current_commit()?;
            self.read_object(&hex::encode(current_commit))?
        };

        println!("{}", commit.string()?);

        Ok(())
    }
}
