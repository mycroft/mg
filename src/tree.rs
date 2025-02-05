use anyhow::{Context, Result};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

use crate::kind::Kind;
use crate::object::TreeObject;
use crate::repository::Repository;

impl Repository {
    pub fn write_tree(&self, path: &PathBuf) -> Result<[u8; 20]> {
        let mut entries = Vec::new();

        let files = std::fs::read_dir(path)?;
        for file in files {
            let file = file?;
            let file_type = file.file_type()?;
            let file_name = file.file_name();
            let file_path = file.path();

            // Skip the .git directory
            if file_name == ".git" {
                continue;
            }

            let hash: [u8; 20];
            let kind;

            if file_type.is_dir() {
                hash = self
                    .write_tree(&file_path)
                    .context("could not write_tree of subtree")?;
                kind = Kind::Tree;
            } else {
                hash = self.write_blob(&file_path).context(format!(
                    "could not write object {:?}",
                    file_path.file_name()
                ))?;
                kind = Kind::Blob(file_path.metadata()?.mode() & 0o111 != 0);
            }

            entries.push(TreeObject {
                mode: kind.to_mode().to_string(),
                kind,
                name: file_name.into_string().unwrap(),
                hash,
            })
        }

        entries.sort_by(|a, b| a.name.cmp(&b.name));

        let mut out: Vec<u8> = Vec::new();
        for entry in &entries {
            out.extend_from_slice(entry.mode.as_bytes());
            out.push(b' ');
            out.extend_from_slice(entry.name.as_bytes());
            out.push(0);
            out.extend_from_slice(&entry.hash);
        }

        self.write_object(Kind::Tree, &out).context("Write")
    }
}
