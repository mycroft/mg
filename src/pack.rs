use std::{
    fs::File,
    io::{BufReader, Cursor, Read, Seek, SeekFrom},
};

use anyhow::Error;
use flate2::read::ZlibDecoder;

use crate::repository::Repository;

#[derive(Debug)]
#[allow(dead_code)]
struct PackHeader {
    signature: [u8; 4],
    version: u32,
    num_objects: u32,
}

#[derive(Debug)]
#[allow(dead_code)]
struct PackObject {
    object_type: PackObjectType,
    object_size: u32,
    object_data: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
enum PackObjectType {
    Commit,
    Tree,
    Blob,
    Tag,
    OfsDelta,
    RefDelta,
}

impl PackObjectType {
    fn from_u8(value: u8) -> Result<PackObjectType, Error> {
        match value {
            1 => Ok(PackObjectType::Commit),
            2 => Ok(PackObjectType::Tree),
            3 => Ok(PackObjectType::Blob),
            4 => Ok(PackObjectType::Tag),
            6 => Ok(PackObjectType::OfsDelta),
            7 => Ok(PackObjectType::RefDelta),
            _ => Err(Error::msg("Unknown object type")),
        }
    }
}

impl std::fmt::Display for PackObjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            PackObjectType::Commit => "commit",
            PackObjectType::Tree => "tree",
            PackObjectType::Blob => "blob",
            PackObjectType::Tag => "tag",
            PackObjectType::OfsDelta => "ofs-delta",
            PackObjectType::RefDelta => "ref-delta",
        };

        write!(f, "{}", s)
    }
}

fn parse_pack_header(file: &mut File) -> Result<PackHeader, Error> {
    let mut header = [0; 12];
    file.read_exact(&mut header)?;

    let signature: &[u8] = &header[0..4];
    if signature != b"PACK" {
        return Err(Error::msg("Invalid pack file"));
    }

    let version = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
    if version != 2 {
        return Err(Error::msg("Invalid pack file version"));
    }

    let num_objects = u32::from_be_bytes([header[8], header[9], header[10], header[11]]);
    let signature: [u8; 4] = signature[0..4].try_into().unwrap();

    Ok(PackHeader {
        signature,
        version,
        num_objects,
    })
}

fn read_vli_le<R>(file: &mut BufReader<R>) -> Result<u32, Error>
where
    R: Read,
{
    let mut val: u32 = 0;
    let mut shift = 0;
    loop {
        let mut byte = [0; 1];
        file.read_exact(&mut byte)?;
        let byt = byte[0] as u32;

        val |= (byt & 0x7f) << shift;
        shift += 7;

        if byt & 0x80 == 0 {
            break;
        }
    }

    Ok(val)
}

fn read_vli_be<R>(file: &mut BufReader<R>, offset: bool) -> Result<u32, Error>
where
    R: Read,
{
    let mut val: u32 = 0;
    loop {
        let mut byte = [0; 1];
        file.read_exact(&mut byte)?;
        let byt = byte[0] as u32;

        val = (val << 7) | (byt & 0x7f);
        if byt & 0x80 == 0 {
            break;
        }

        if offset {
            val += 1;
        }
    }

    Ok(val)
}

fn make_delta_obj(
    file: &mut File,
    base_obj: PackObject,
    object_size: u32,
) -> Result<PackObject, Error> {
    let mut object_data = Vec::new();

    let pos = file.seek(SeekFrom::Current(0))?;
    let mut zlib_decoder = ZlibDecoder::new(&mut *file);
    zlib_decoder.read_to_end(&mut object_data)?;
    let read_bytes = zlib_decoder.total_in();
    file.seek(std::io::SeekFrom::Start(pos + read_bytes))?;

    assert_eq!(object_data.len(), object_size as usize);

    let mut fp2 = BufReader::new(Cursor::new(object_data.as_slice()));

    let _base_obj_size = read_vli_le(&mut fp2)?;
    let patched_obj_size = read_vli_le(&mut fp2)?;

    // println!(
    //     "base_obj_size={}, obj_size={}",
    //     base_obj_size, patched_obj_size
    // );

    let mut obj_data = Vec::new();
    while fp2.seek(SeekFrom::Current(0))? < object_data.len() as u64 {
        let mut byte = [0; 1];
        fp2.read_exact(&mut byte)?;
        let byt = byte[0];

        if byt == 0x00 {
            continue;
        }

        if byt & 0x80 != 0 {
            // copy data from base object
            let mut vals = [0; 6];
            for i in 0..6 {
                let bmask = 1 << i;
                if byt & bmask != 0 {
                    fp2.read_exact(&mut byte)?;
                    vals[i] = byte[0];
                } else {
                    vals[i] = 0;
                }
            }

            let start = u32::from_le_bytes(vals[0..4].try_into().expect("4 bytes"));
            let nbytes = u16::from_le_bytes(vals[4..6].try_into().expect("2 bytes"));
            let nbytes = if nbytes == 0 { 0x10000 } else { nbytes as u32 };

            obj_data.extend_from_slice(
                &base_obj.object_data[start as usize..(start + nbytes) as usize],
            );
        } else {
            // add new data
            let nbytes = byt & 0x7f;
            // println!("APPEND NEW BYTES #bytes={}", nbytes);
            let mut data = vec![0; nbytes as usize];
            fp2.read_exact(&mut data)?;
            obj_data.extend_from_slice(&data);
        }
    }

    // println!("Final object data: #bytes={}", obj_data.len());

    assert_eq!(obj_data.len(), patched_obj_size as usize);

    Ok(PackObject {
        object_type: base_obj.object_type,
        object_size: patched_obj_size,
        object_data: obj_data,
    })
}

fn parse_pack_ofs_delta_object(
    file: &mut File,
    object_size: u32,
    fpos: u64,
) -> Result<PackObject, Error> {
    // println!("pos: 0x{:x}", file.seek(SeekFrom::Current(0))?);

    let mut reader = BufReader::new(&mut *file);
    let offset = read_vli_be(&mut reader, true)?;
    let new_position = reader.stream_position()?;
    file.seek(SeekFrom::Start(new_position))?;

    let base_obj_offset = fpos - offset as u64;

    // println!(
    //     "offset:0x{:x} base_obj_offset:0x{:x}",
    //     offset, base_obj_offset
    // );

    let prev_pos = file.seek(SeekFrom::Current(0))?;
    file.seek(SeekFrom::Start(base_obj_offset))?;

    let base_obj = parse_pack_entry(file)?;
    assert!(vec![
        PackObjectType::Commit,
        PackObjectType::Tree,
        PackObjectType::Blob,
        PackObjectType::Tag
    ]
    .contains(&base_obj.object_type));

    file.seek(SeekFrom::Start(prev_pos))?;

    make_delta_obj(file, base_obj, object_size)
}

fn parse_pack_entry(file: &mut File) -> Result<PackObject, Error> {
    let object_pos = file.seek(SeekFrom::Current(0))?;

    let mut byte = [0; 1];
    file.read_exact(&mut byte)?;
    let object_type: u8 = (byte[0] & 0x70) >> 4;
    let mut object_data = Vec::new();

    let mut object_size: u32 = (byte[0] & 0x0f) as u32;
    let mut bshift = 4;
    while (byte[0] & 0x80) == 0x80 {
        file.read_exact(&mut byte)?;
        object_size += (byte[0] as u32 & 0x7f) << bshift;
        bshift += 7;
    }

    println!(
        "Reading object: fpos=0x{:x}, type:{} size:{}",
        object_pos,
        PackObjectType::from_u8(object_type)?,
        object_size
    );

    match PackObjectType::from_u8(object_type)? {
        PackObjectType::Commit
        | PackObjectType::Tree
        | PackObjectType::Blob
        | PackObjectType::Tag => {
            // get current file offset
            let pos = file.seek(SeekFrom::Current(0))?;
            let mut zlib_decoder = ZlibDecoder::new(&mut *file);

            zlib_decoder.read_to_end(&mut object_data)?;
            let read_bytes = zlib_decoder.total_in();

            file.seek(std::io::SeekFrom::Start(pos + read_bytes))?;

            assert_eq!(object_data.len(), object_size as usize);
        }
        PackObjectType::OfsDelta => {
            return parse_pack_ofs_delta_object(file, object_size, object_pos);
        }
        PackObjectType::RefDelta => unimplemented!(),
    }

    Ok(PackObject {
        object_type: PackObjectType::from_u8(object_type)?,
        object_size,
        object_data,
    })
}

impl Repository {
    pub fn dump_pack_files(&self) -> Result<(), Error> {
        let pack_dir = self.path.join(".git/objects/pack");

        for entry in pack_dir.read_dir()? {
            let entry = entry?;
            let path = entry.file_name();
            let path_str = path.to_str().unwrap();
            if path_str.starts_with("pack-") && path_str.ends_with(".pack") {
                let pack_id = &path_str[5..path_str.len() - 5];
                self.dump_pack_file(pack_id)?;
            }
        }

        Ok(())
    }

    pub fn dump_pack_file(&self, pack_id: &str) -> Result<(), Error> {
        let file_path = self
            .path
            .join(format!(".git/objects/pack/pack-{}.pack", pack_id));

        let mut file = File::open(file_path)?;

        let header = parse_pack_header(&mut file)?;
        println!("{:?}", header);

        for _ in 0..header.num_objects {
            let _obj = parse_pack_entry(&mut file)?;
            // println!(
            //     "Read object: type={}, #bytes={}",
            //     obj.object_type, obj.object_size
            // );
            // println!("{:?}", obj);

            // println!();
        }

        // At the end of the file, there should be a 20-byte SHA-1 checksum
        // TBD

        Ok(())
    }
}
