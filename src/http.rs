use std::io::Write;

use anyhow::{Error, Result};
use nom::AsBytes;
use reqwest::Client;

pub async fn clone(repo: &str) -> Result<(), Error> {
    let (size, refs) = get_refs(repo).await?;

    println!("Refs:");
    for (sha1, name) in refs.iter() {
        println!("{} {}", sha1, name);
    }
    println!("Downloaded file size: {}", size);

    Ok(())
}

pub fn parse_refs(input: &[u8]) -> Result<Vec<(String, String)>> {
    let mut refs = Vec::new();
    let mut index: usize = 0;
    loop {
        if index >= input.len() {
            break;
        }
        // pick the next 4 bytes and convert to u32 from hex
        let mut bytes = [0; 4];
        bytes.copy_from_slice(&input[index..index + 4]);

        let hex_str = std::str::from_utf8(&bytes)?;
        let res = usize::from_str_radix(hex_str, 16)?;

        if res == 0 {
            index += 4;
            continue;
        }

        if input[index + 4] == b'#' {
            index += res;
            continue;
        }

        let mut sha1_bytes = [0; 40];
        sha1_bytes.copy_from_slice(&input[index + 4..index + 44]);
        let idx_0 = input[index + 45..index + res - 1]
            .iter()
            .position(|&x| x == 0);

        let sha1 = std::str::from_utf8(&sha1_bytes)?;
        let name = if let Some(idx_0) = idx_0 {
            std::str::from_utf8(&input[index + 45..index + 45 + idx_0])?
        } else {
            std::str::from_utf8(&input[index + 45..index + res - 1])?
        };

        refs.push((name.to_string(), sha1.to_string()));

        index += res;
    }
    Ok(refs)
}

pub async fn get_refs(repo_url: &str) -> Result<(usize, Vec<(String, String)>), Error> {
    let info_refs_url = format!("{}/info/refs?service=git-upload-pack", repo_url);

    let client = Client::new();
    let response = client
        .get(&info_refs_url)
        .header("User-Agent", "git/2.30.0")
        .send()
        .await?;

    response.error_for_status_ref()?;

    let content = response.bytes().await?;
    let refs = parse_refs(&content)?;

    get_packfile(repo_url, refs).await
}

pub fn packet_line(data: &str) -> Vec<u8> {
    let length = format!("{:04x}", data.len() + 4);
    let mut line = Vec::new();
    line.extend_from_slice(length.as_bytes());
    line.extend_from_slice(data.as_bytes());
    line
}

pub async fn get_packfile(
    repo_url: &str,
    refs: Vec<(String, String)>,
) -> Result<(usize, Vec<(String, String)>), Error> {
    let upload_pack_url = format!("{}/git-upload-pack", repo_url);

    let mut payload: Vec<u8> = Vec::new();

    payload.extend(packet_line("command=fetch").as_slice());
    payload.extend(packet_line("agent=git/2.30.0").as_slice());
    payload.extend(packet_line("object-format=sha1").as_slice());
    payload.extend("0001".as_bytes());
    payload.extend(packet_line("ofs-delta").as_slice());
    payload.extend(packet_line("no-progress").as_slice());

    for (_, sha1) in refs.iter() {
        let want = format!("want {}\n", sha1);
        payload.extend(packet_line(want.as_str()).as_slice());
    }

    payload.extend("0000".as_bytes());
    payload.extend(packet_line("done").as_slice());

    let client = Client::new();
    let response = client
        .post(&upload_pack_url)
        .header("User-Agent", "git/2.30.0")
        .header("Content-Type", "application/x-git-upload-pack-request")
        .header("Accept-Encoding", "deflate")
        .header("Accept", "application/x-git-upload-pack-result")
        .header("Git-Protocol", "version=2")
        .body(payload)
        .send()
        .await?;

    response.error_for_status_ref()?;

    let content = response.bytes().await?;
    decode_git_response(content.as_bytes())?;

    Ok((content.len(), refs))
}

fn decode_git_response(content: &[u8]) -> Result<(), Error> {
    let mut cursor = 0;
    let mut pack_data = Vec::new();

    while cursor < content.len() {
        let length_str = std::str::from_utf8(&content[cursor..cursor + 4])?;
        cursor += 4;

        let length = usize::from_str_radix(length_str, 16)?;
        if length == 0 {
            break;
        }

        let payload = &content[cursor..cursor + length - 4];
        cursor += length - 4;

        let side_band = payload[0];
        let data = &payload[1..];

        if side_band == 1 {
            pack_data.extend(data);
        } else if side_band == 2 {
            println!("Progress: {}", std::str::from_utf8(data)?);
        } else if side_band == 3 {
            println!("Error: {}", std::str::from_utf8(data)?);
        }
    }

    if !pack_data.is_empty() {
        let mut packfile = std::fs::File::create("downloaded.pack")?;
        packfile.write_all(&pack_data)?;
        println!("Packfile saved as 'downloaded.pack'");
    }

    Ok(())
}
