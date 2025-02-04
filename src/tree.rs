use anyhow::{Context, Result};
use sha1::{Digest, Sha1};
use std::path::PathBuf;

use crate::kind::Kind;
use crate::object::{hash_file, TreeObject};

pub fn write_tree(_repo_path: &PathBuf, path: &PathBuf) -> Result<[u8; 20]> {
    let mut entries = Vec::new();

    let files = std::fs::read_dir(path)?;
    for file in files {
        let file = file?;
        let file_type = file.file_type()?;
        let file_name = file.file_name();
        let file_path = file.path();

        let hash: [u8; 20];
        let kind;

        if file_type.is_dir() {
            hash = write_tree(_repo_path, &file_path).context("could not write_tree of subtree")?;
            kind = Kind::Tree;
        } else {
            hash = hash_file(&file_path)?;
            kind = Kind::Blob;
        }

        entries.push(TreeObject {
            mode: "100644".to_string(),
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

    let header = format!("tree {}\0", out.len());

    let mut hasher = Sha1::new();
    hasher.update(header);
    hasher.update(out);

    Ok(hasher.finalize().into())
}
