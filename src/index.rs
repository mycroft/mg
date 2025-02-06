use std::{os::linux::fs::MetadataExt, path::Path};

use nom::{
    bytes::complete::take,
    number::complete::{be_u16, be_u32},
    IResult, Parser,
};

use anyhow::{anyhow, Error, Result};
use sha1::{Digest, Sha1};
use walkdir::WalkDir;

use crate::repository::Repository;

#[derive(Debug)]
#[allow(dead_code)]
struct IndexHeader {
    signature: [u8; 4], // "DIRC"
    version: u32,       // 2, 3, or 4
    entries_count: u32,
}

#[derive(Debug)]
#[allow(dead_code)]
struct IndexEntry {
    ctime_s: u32,
    ctime_n: u32,
    mtime_s: u32,
    mtime_n: u32,
    dev: u32,
    ino: u32,
    mode: u32,
    uid: u32,
    gid: u32,
    size: u32,
    sha1: [u8; 20],
    flags: u16,
    file_path: String,
}

#[derive(Debug)]
#[allow(dead_code)]
struct Index {
    header: IndexHeader,
    entries: Vec<IndexEntry>,
}

fn parse_index(input: &[u8]) -> IResult<&[u8], Index> {
    let (mut input, header) = parse_header(input)?;

    let mut entries = Vec::with_capacity(header.entries_count as usize);

    for _ in 0..header.entries_count {
        let (remaining, entry) = parse_entry(input)?;
        entries.push(entry);
        input = remaining;
    }

    Ok((input, Index { header, entries }))
}

fn parse_header(input: &[u8]) -> IResult<&[u8], IndexHeader> {
    let (input, (signature, version, entries_count)) =
        (take(4usize), be_u32, be_u32).parse(input)?;

    let mut sig = [0u8; 4];
    sig.copy_from_slice(signature);

    Ok((
        input,
        IndexHeader {
            signature: sig,
            version,
            entries_count,
        },
    ))
}

fn parse_entry(input: &[u8]) -> IResult<&[u8], IndexEntry> {
    let start_input_len = input.len();
    let (
        input,
        (ctime_s, ctime_n, mtime_s, mtime_n, dev, ino, mode, uid, gid, size, sha1_bytes, flags),
    ) = (
        be_u32,
        be_u32,
        be_u32,
        be_u32,
        be_u32,
        be_u32,
        be_u32,
        be_u32,
        be_u32,
        be_u32,
        take(20usize),
        be_u16,
    )
        .parse(input)?;
    let current_input_len = input.len();

    let path_len = flags & 0xFFF;
    let (input, path_bytes) = take(path_len as usize)(input)?;
    let file_path = String::from_utf8_lossy(path_bytes).into_owned();

    //  between 1 and 8 NUL bytes to pad the entry.
    let padding_len = 8 - ((start_input_len - current_input_len) + path_len as usize) % 8;
    let (input, _) = take(padding_len)(input)?;

    let mut sha1 = [0u8; 20];
    sha1.copy_from_slice(sha1_bytes);

    Ok((
        input,
        IndexEntry {
            ctime_s,
            ctime_n,
            mtime_s,
            mtime_n,
            dev,
            ino,
            mode,
            uid,
            gid,
            size,
            sha1,
            flags,
            file_path,
        },
    ))
}

impl Index {
    pub fn read_from_file(path: &Path) -> Result<Self, Error> {
        let content = std::fs::read(path)?;
        let (_remaining, index) =
            parse_index(&content).map_err(|e| anyhow!("Failed to parse index: {}", e))?;
        Ok(index)
    }
}

impl Repository {
    pub fn read_index(&self) -> Result<()> {
        let index_path = self.path.join(".git").join("index");
        let index = Index::read_from_file(&index_path)?;

        for entry in index.entries {
            println!("{} {}", hex::encode(entry.sha1), entry.file_path);
        }

        Ok(())
    }

    pub fn write_index(&self) -> Result<()> {
        let index_path = self.path.join(".git").join("index");

        // list all files in the repository
        let files = list_all_files(&self.path, &self.ignore)?;

        let index = Index {
            header: IndexHeader {
                signature: *b"DIRC",
                version: 2,
                entries_count: files.len() as u32,
            },
            entries: Vec::new(),
        };

        let mut content = Vec::new();
        content.extend_from_slice(&index.header.signature);
        content.extend_from_slice(&index.header.version.to_be_bytes());
        content.extend_from_slice(&index.header.entries_count.to_be_bytes());

        for file in files {
            let metadata = std::fs::metadata(self.path.join(file.clone()))?;

            let entry = IndexEntry {
                ctime_s: metadata.st_ctime() as u32,
                ctime_n: metadata.st_ctime_nsec() as u32,
                mtime_s: metadata.st_mtime() as u32,
                mtime_n: metadata.st_mtime_nsec() as u32,
                dev: metadata.st_dev() as u32,
                ino: metadata.st_ino() as u32,
                mode: metadata.st_mode(),
                uid: metadata.st_uid(),
                gid: metadata.st_gid(),
                size: metadata.st_size() as u32,
                sha1: hash_file(&self.path.join(file.clone()))?,
                flags: 0,
                file_path: file,
            };

            let mut entry_content = Vec::new();
            entry_content.extend_from_slice(&entry.ctime_s.to_be_bytes());
            entry_content.extend_from_slice(&entry.ctime_n.to_be_bytes());
            entry_content.extend_from_slice(&entry.mtime_s.to_be_bytes());
            entry_content.extend_from_slice(&entry.mtime_n.to_be_bytes());
            entry_content.extend_from_slice(&entry.dev.to_be_bytes());
            entry_content.extend_from_slice(&entry.ino.to_be_bytes());
            entry_content.extend_from_slice(&entry.mode.to_be_bytes());
            entry_content.extend_from_slice(&entry.uid.to_be_bytes());
            entry_content.extend_from_slice(&entry.gid.to_be_bytes());
            entry_content.extend_from_slice(&entry.size.to_be_bytes());
            entry_content.extend_from_slice(&entry.sha1);
            //entry_content.extend_from_slice(&entry.flags.to_be_bytes());

            let path_bytes = entry.file_path.as_bytes();
            entry_content.extend_from_slice(&(path_bytes.len() as u16).to_be_bytes());
            entry_content.extend_from_slice(path_bytes);

            //  between 1 and 8 NUL bytes to pad the entry.
            let padding_len = 8 - entry_content.len() % 8;
            entry_content.extend(vec![0u8; padding_len]);

            content.extend(entry_content);
        }

        std::fs::write(index_path, content)?;

        Ok(())
    }
}

pub fn list_all_files(path: &Path, ignore_list: &[String]) -> Result<Vec<String>> {
    let mut files = Vec::new();

    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if ignore_list.iter().any(|i| entry.path().ends_with(i)) {
                continue;
            }

            let s = entry.path().to_path_buf().to_str().unwrap().to_string();
            let s = s.strip_prefix(path.to_str().unwrap()).unwrap().to_string();

            if ignore_list.iter().any(|i| s.starts_with(i)) {
                continue;
            }

            files.push(s.strip_prefix("/").unwrap().to_string());
        }
    }

    files.sort();

    Ok(files)
}

fn hash_file(path: &Path) -> Result<[u8; 20]> {
    let content = std::fs::read(path)?;

    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", content.len()).as_bytes());
    hasher.update(content);

    Ok(hasher.finalize().into())
}
