use std::path::Path;

use nom::{
    bytes::complete::take,
    number::complete::{be_u16, be_u32},
    IResult, Parser,
};

use anyhow::{anyhow, Error, Result};

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
}
