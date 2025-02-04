use anyhow::{Context, Result};
use std::{fs::File, io::BufRead, path::Path};

use crate::{error::RuntimeError, kind::Kind};

#[derive(Debug)]
pub struct Object<Reader> {
    kind: Kind,
    _size: usize,
    data: Reader,
}

#[derive(Debug)]
struct TreeObject {
    mode: String,
    kind: Kind,
    name: String,
    hash: [u8; 20],
}

pub fn read_object(path: &Path, object: &str) -> Result<Object<impl BufRead>> {
    let object_path = path
        .join(".git")
        .join("objects")
        .join(&object[..2])
        .join(&object[2..]);

    let fd = File::open(&object_path).context("opening the object")?;
    let zfd = flate2::read::ZlibDecoder::new(fd);
    let mut buf_reader = std::io::BufReader::new(zfd);

    let mut buf: Vec<u8> = Vec::new();
    buf_reader
        .read_until(0, &mut buf)
        .context("read the object")?;

    match buf.pop() {
        Some(0) => {}
        Some(_) | None => return Err(RuntimeError::UnexpectedChar.into()),
    };

    let header = String::from_utf8(buf.clone()).context("converting header to utf-8")?;

    let Some((object_type, object_size)) = header.split_once(' ') else {
        anyhow::bail!("could not parse object header correctly");
    };

    let object_type = match object_type {
        "blob" => Kind::Blob,
        "commit" => Kind::Commit,
        "tree" => Kind::Tree,
        _ => anyhow::bail!("invalid object type found"),
    };

    let object_size = object_size.parse::<usize>()?;

    Ok(Object {
        kind: object_type,
        _size: object_size,
        data: buf_reader,
    })
}

impl<R: BufRead> Object<R> {
    pub fn string(&mut self) -> Result<String> {
        let mut buf: Vec<u8> = Vec::new();
        let mut buf_hash: [u8; 20] = [0; 20];

        let res = match self.kind {
            Kind::Blob | Kind::Commit => {
                self.data.read_to_end(&mut buf)?;
                String::from_utf8(buf)?
            }
            Kind::Tree => {
                let mut max_name_len = 0;
                let mut entries = Vec::new();
                loop {
                    let read_bytes_len = self.data.read_until(0, &mut buf)?;
                    if read_bytes_len == 0 {
                        break;
                    }

                    let mode_name = buf.clone();
                    buf.clear();
                    let mut splits = mode_name.splitn(2, |&b| b == b' ');

                    let mode = splits
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("could not parse mode"))?;
                    let mode = std::str::from_utf8(mode)?;
                    let name = splits
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("could not parse name"))?;
                    let name = std::str::from_utf8(name)?;

                    self.data.read_exact(&mut buf_hash)?;

                    if name.len() > max_name_len {
                        max_name_len = name.len();
                    }

                    entries.push(TreeObject {
                        name: name.to_string(),
                        kind: Kind::from_mode(mode)?,
                        mode: mode.to_string(),
                        hash: buf_hash,
                    });
                }

                entries.sort_by(|a, b| a.name.cmp(&b.name));

                entries
                    .iter()
                    .map(|entry| {
                        let hash = hex::encode(entry.hash);
                        format!(
                            "{:0>6} {} {}    {:name_len$}",
                            entry.mode,
                            entry.kind.string(),
                            hash,
                            entry.name,
                            name_len = max_name_len
                        )
                    })
                    .collect::<Vec<String>>()
                    .join("\n")
            }
            _ => unimplemented!(),
        };

        Ok(res)
    }
}
