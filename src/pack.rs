use std::{
    fs::File,
    io::{BufReader, Cursor, Read, Seek, SeekFrom},
    path::Path,
};

use anyhow::Error;
use flate2::read::ZlibDecoder;
use sha1::{Digest, Sha1};

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
    pos: u64,
    end_pos: u64,
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

fn read_vli_be(file: &mut File, offset: bool) -> Result<u32, Error> {
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

fn decompress_file(file: &mut File) -> Result<Vec<u8>, Error> {
    let mut object_data = Vec::new();

    let pos = file.stream_position()?;
    let mut zlib_decoder = ZlibDecoder::new(&mut *file);
    zlib_decoder.read_to_end(&mut object_data)?;
    let read_bytes = zlib_decoder.total_in();
    file.seek(std::io::SeekFrom::Start(pos + read_bytes))?;

    Ok(object_data)
}

fn make_delta_obj(
    file: &mut File,
    base_obj: PackObject,
    object_size: u32,
) -> Result<PackObject, Error> {
    let current_pos = file.stream_position()?;
    let object_data = decompress_file(file)?;

    assert_eq!(object_data.len(), object_size as usize);

    let mut fp2 = BufReader::new(Cursor::new(object_data.as_slice()));

    let _base_obj_size = read_vli_le(&mut fp2)?;
    let patched_obj_size = read_vli_le(&mut fp2)?;

    // println!(
    //     "base_obj_size={}, obj_size={}",
    //     base_obj_size, patched_obj_size
    // );

    let mut obj_data = Vec::new();
    while fp2.stream_position()? < object_data.len() as u64 {
        let mut byte = [0; 1];
        fp2.read_exact(&mut byte)?;
        let byt = byte[0];

        if byt == 0x00 {
            continue;
        }

        if byt & 0x80 != 0 {
            // copy data from base object
            let mut vals = [0; 6];

            for (i, val) in vals.iter_mut().enumerate() {
                let bmask = 1 << i;
                if byt & bmask != 0 {
                    fp2.read_exact(&mut byte)?;
                    *val = byte[0];
                } else {
                    *val = 0;
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
        pos: current_pos,
        end_pos: file.stream_position()?,
    })
}

fn parse_pack_ofs_delta_object(
    file: &mut File,
    object_size: u32,
    fpos: u64,
) -> Result<PackObject, Error> {
    // println!("pos: 0x{:x}", file.seek(SeekFrom::Current(0))?);

    // let mut reader = BufReader::new(&mut *file);
    let offset = read_vli_be(file, true)?;
    // let new_position = reader.stream_position()?;
    // file.seek(SeekFrom::Start(new_position))?;

    let base_obj_offset = fpos - offset as u64;

    // println!(
    //     "offset:0x{:x} base_obj_offset:0x{:x}",
    //     offset, base_obj_offset
    // );

    let prev_pos = file.stream_position()?;
    file.seek(SeekFrom::Start(base_obj_offset))?;

    let base_obj = parse_pack_entry(file)?;
    assert!([
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
    let object_pos = file.stream_position()?;

    let mut byte = [0; 1];
    file.read_exact(&mut byte)?;
    let object_type: u8 = (byte[0] & 0x70) >> 4;
    let object_data;

    let mut object_size: u32 = (byte[0] & 0x0f) as u32;
    let mut bshift = 4;
    while (byte[0] & 0x80) == 0x80 {
        file.read_exact(&mut byte)?;
        object_size += (byte[0] as u32 & 0x7f) << bshift;
        bshift += 7;
    }

    // println!(
    //     "Reading object: fpos=0x{:x}, type:{} size:{}",
    //     object_pos,
    //     PackObjectType::from_u8(object_type)?,
    //     object_size
    // );

    match PackObjectType::from_u8(object_type)? {
        PackObjectType::Commit
        | PackObjectType::Tree
        | PackObjectType::Blob
        | PackObjectType::Tag => {
            object_data = decompress_file(file)?;
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
        pos: object_pos,
        end_pos: file.stream_position()?,
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

    pub fn dump_pack(&self, path: &Path) -> Result<(), Error> {
        let mut file = File::open(path)?;

        let header = parse_pack_header(&mut file)?;
        println!("{:?}", header);

        for _ in 0..header.num_objects {
            let obj = parse_pack_entry(&mut file)?;

            let mut hasher = Sha1::new();
            hasher.update(format!("{} {}\0", obj.object_type, obj.object_size).as_bytes());
            hasher.update(obj.object_data);

            println!(
                "{} {} {} {} {}",
                hex::encode(hasher.finalize()),
                obj.object_type,
                obj.object_size,
                obj.end_pos - obj.pos,
                obj.pos,
            );
        }

        let mut checksum_pack = [0; 20];
        file.read_exact(&mut checksum_pack)?;

        Ok(())
    }

    pub fn dump_pack_file(&self, pack_id: &str) -> Result<(), Error> {
        let file_path = self
            .path
            .join(format!(".git/objects/pack/pack-{}.pack", pack_id));

        self.dump_pack(&file_path)
    }

    pub fn dump_pack_index_file(&self, pack_id: &str) -> Result<(), Error> {
        let file_path = self
            .path
            .join(format!(".git/objects/pack/pack-{}.idx", pack_id));

        let mut file = File::open(file_path)?;

        let mut buf = [0; 4];
        file.read_exact(&mut buf)?;

        if buf[0] != 0xff || "t0c".as_bytes() == &buf[1..4] {
            return Err(Error::msg("Invalid pack index magic"));
        }

        file.read_exact(&mut buf)?;
        let version = u32::from_be_bytes(buf);
        println!("{}", version);
        if version != 2 {
            return Err(Error::msg("Invalid pack index version"));
        }

        let mut num_objects: u32 = 0;
        let mut fanout_table = [0u32; 256];

        // fanout table: 256 x 8 bytes
        let mut buf = [0u8; 256 * 4];
        file.read_exact(&mut buf)?;

        for (idx, fanout_record) in fanout_table.iter_mut().enumerate() {
            num_objects = u32::from_be_bytes(buf[idx * 4..idx * 4 + 4].try_into().unwrap());
            *fanout_record = num_objects;
        }

        let mut names = vec![0u8; 20 * num_objects as usize];
        file.read_exact(&mut names)?;

        let mut crc32_buf = vec![0u8; 4 * num_objects as usize];
        file.read_exact(&mut crc32_buf)?;
        let crc32: Vec<u32> = crc32_buf
            .chunks_exact(4)
            .map(|chunk| u32::from_be_bytes(chunk.try_into().unwrap()))
            .collect();

        let mut offsets_buf = vec![0u8; 4 * num_objects as usize];
        file.read_exact(&mut offsets_buf)?;
        let offsets: Vec<u32> = offsets_buf
            .chunks_exact(4)
            .map(|chunk| u32::from_be_bytes(chunk.try_into().unwrap()))
            .collect();

        for i in 0..num_objects {
            let offset = offsets[i as usize];
            let crc32 = crc32[i as usize];
            let name = &names[(i * 20) as usize..(i * 20 + 20) as usize];
            println!(
                "{} offset: 0x{:x} crc32: {}",
                hex::encode(name),
                offset,
                crc32
            );
        }

        let mut checksum_pack = [0; 20];
        file.read_exact(&mut checksum_pack)?;
        let mut checksum_idx = [0; 20];
        file.read_exact(&mut checksum_idx)?;

        Ok(())
    }
}
