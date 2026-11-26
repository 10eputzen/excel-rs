//! Reads the workbook stream of an xls file exactly the way calamine does.
//!
//! The size prescan must look at the same bytes calamine will parse. A
//! different compound-file reader can pick a different stream (calamine takes
//! the first directory entry by name, including entries outside the tree) or
//! assemble it differently, so this is a port of calamine's own reader
//! (`calamine/src/cfb.rs`, MIT License, Copyright (c) 2016 Johann Tuffe). Where calamine would panic or loop without
//! bound, this returns an error, and the caller rejects the file.

use std::io::{Cursor, Read};

const RESERVED_SECTORS: u32 = 0xFFFF_FFFA;
const DIFSECT: u32 = 0xFFFF_FFFC;
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const SIGNATURE: &[u8; 8] = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1";

/// Any reason the stream cannot be read; the caller treats all as corrupt.
#[derive(Debug)]
pub struct OleError;

type Result<T> = std::result::Result<T, OleError>;

impl From<std::io::Error> for OleError {
    fn from(_: std::io::Error) -> Self {
        OleError
    }
}

/// The `Workbook` stream, or `Book` if that fails, as calamine selects it.
pub fn workbook_stream(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut reader = Cursor::new(bytes);
    let mut cfb = Cfb::new(&mut reader, bytes.len())?;
    cfb.get_stream("Workbook", &mut reader)
        .or_else(|_| cfb.get_stream("Book", &mut reader))
}

struct Cfb {
    directories: Vec<Directory>,
    sectors: Sectors,
    fats: Vec<u32>,
    mini_sectors: Sectors,
    mini_fats: Vec<u32>,
}

impl Cfb {
    fn new<R: Read>(reader: &mut R, len: usize) -> Result<Cfb> {
        let (h, mut difat) = Header::from_reader(reader)?;
        let sector_size = h.sector_size;
        let maxid = len / sector_size;
        let mut sectors = Sectors::new(sector_size, Vec::new(), maxid as u32);

        let mut sector_id = h.difat_start;
        let mut difat_sectors = 0usize;
        while sector_id < RESERVED_SECTORS {
            difat_sectors += 1;
            if difat_sectors > maxid.saturating_add(1) {
                return Err(OleError);
            }
            difat.extend(to_u32(sectors.get(sector_id, reader)?)?);
            sector_id = difat.pop().ok_or(OleError)?;
        }

        let mut fats = Vec::new();
        for id in difat.into_iter().filter(|id| *id < DIFSECT) {
            fats.extend(to_u32(sectors.get(id, reader)?)?);
        }

        let dirs = sectors.get_chain(h.dir_start, &fats, reader, usize::MAX)?;
        let dirs = dirs
            .chunks(128)
            .filter(|c| c.len() == 128)
            .map(|c| Directory::from_slice(c, sector_size))
            .collect::<Result<Vec<_>>>()?;
        if dirs.is_empty() || (h.version != 3 && dirs[0].start == ENDOFCHAIN) {
            return Err(OleError);
        }

        let (mini_fats, ministream) = if h.mini_fat_len > 0 {
            let ministream = sectors.get_chain(dirs[0].start, &fats, reader, dirs[0].len)?;
            let minifat = sectors.get_chain(
                h.mini_fat_start,
                &fats,
                reader,
                h.mini_fat_len.checked_mul(sector_size).ok_or(OleError)?,
            )?;
            (to_u32(&minifat)?, ministream)
        } else {
            (Vec::new(), Vec::new())
        };
        Ok(Cfb {
            directories: dirs,
            sectors,
            fats,
            mini_sectors: Sectors::new(64, ministream, maxid.saturating_mul(8) as u32),
            mini_fats,
        })
    }

    fn get_stream<R: Read>(&mut self, name: &str, r: &mut R) -> Result<Vec<u8>> {
        let d = self
            .directories
            .iter()
            .find(|d| d.name.as_bytes().eq_ignore_ascii_case(name.as_bytes()))
            .ok_or(OleError)?;
        if d.len < 4096 {
            self.mini_sectors
                .get_chain(d.start, &self.mini_fats, r, d.len)
        } else {
            self.sectors.get_chain(d.start, &self.fats, r, d.len)
        }
    }
}

struct Header {
    version: u16,
    sector_size: usize,
    dir_start: u32,
    mini_fat_len: usize,
    mini_fat_start: u32,
    difat_start: u32,
}

impl Header {
    fn from_reader<R: Read>(f: &mut R) -> Result<(Header, Vec<u32>)> {
        let mut n = 0;
        let mut buf = [0u8; 512];
        loop {
            let k = f.read(&mut buf[n..])?;
            if k == 0 {
                break;
            }
            n += k;
        }
        if n < buf.len() || &buf[..8] != SIGNATURE {
            return Err(OleError);
        }
        let version = u16_at(&buf, 26);
        let sector_size = match u16_at(&buf, 30) {
            0x0009 => 512,
            0x000C => {
                let mut rest = [0u8; 4096 - 512];
                f.read_exact(&mut rest)?;
                4096
            }
            _ => return Err(OleError),
        };
        if u16_at(&buf, 32) != 0x0006 {
            return Err(OleError);
        }
        let header = Header {
            version,
            sector_size,
            dir_start: u32_at(&buf, 48),
            mini_fat_start: u32_at(&buf, 60),
            mini_fat_len: u32_at(&buf, 64) as usize,
            difat_start: u32_at(&buf, 68),
        };
        Ok((header, to_u32(&buf[76..512])?))
    }
}

/// Sectors read so far; `data` grows as later sectors are requested.
struct Sectors {
    data: Vec<u8>,
    size: usize,
    maxid: u32,
}

impl Sectors {
    fn new(size: usize, data: Vec<u8>, maxid: u32) -> Sectors {
        Sectors { data, size, maxid }
    }

    fn get<R: Read>(&mut self, id: u32, r: &mut R) -> Result<&[u8]> {
        if self.maxid < id {
            return Err(OleError);
        }
        let start = (id as usize).checked_mul(self.size).ok_or(OleError)?;
        let end = start.checked_add(self.size).ok_or(OleError)?;
        if end > self.data.len() {
            let mut len = self.data.len();
            self.data.resize(end, 0);
            while len < end {
                let read = r.read(&mut self.data[len..end])?;
                if read == 0 {
                    // calamine slices `start..len` here and panics if start > len.
                    return self.data.get(start..len).ok_or(OleError);
                }
                len += read;
            }
        }
        Ok(&self.data[start..end])
    }

    fn get_chain<R: Read>(
        &mut self,
        mut sector_id: u32,
        fats: &[u32],
        r: &mut R,
        len: usize,
    ) -> Result<Vec<u8>> {
        let mut chain = Vec::new();
        // A chain visits each sector at most once; a longer one is a cycle,
        // which calamine would follow until memory runs out.
        let mut steps = 0u64;
        while sector_id != ENDOFCHAIN {
            steps += 1;
            if steps > u64::from(self.maxid) + 1 {
                return Err(OleError);
            }
            let sector = self.get(sector_id, r)?;
            let to_copy = sector.len().min(len - chain.len());
            chain.extend_from_slice(&sector[..to_copy]);
            if chain.len() >= len {
                break;
            }
            sector_id = *fats.get(sector_id as usize).ok_or(OleError)?;
        }
        Ok(chain)
    }
}

struct Directory {
    name: String,
    start: u32,
    len: usize,
}

impl Directory {
    fn from_slice(buf: &[u8], sector_size: usize) -> Result<Directory> {
        let mut name = encoding_rs::UTF_16LE.decode(&buf[..64]).0.into_owned();
        if let Some(l) = name.as_bytes().iter().position(|b| *b == 0) {
            name.truncate(l);
        }
        let len = if sector_size == 512 {
            usize::try_from(u32_at(buf, 120))
        } else {
            usize::try_from(u64::from_le_bytes(
                buf[120..128].try_into().map_err(|_| OleError)?,
            ))
        };
        Ok(Directory {
            name,
            start: u32_at(buf, 116),
            len: len.map_err(|_| OleError)?,
        })
    }
}

/// calamine asserts a length divisible by 4 (a sector cut short by the end of file).
fn to_u32(s: &[u8]) -> Result<Vec<u32>> {
    if !s.len().is_multiple_of(4) {
        return Err(OleError);
    }
    Ok(s.as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect())
}

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}
