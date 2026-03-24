use anyhow::Result;
use memmap2::Mmap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

const SLOT_SIZE: usize = 20;
const LOAD_FACTOR: f64 = 0.7;
const MAGIC: &[u8; 4] = b"RSLK";

#[derive(Clone, Copy)]
pub struct LookupEntry {
    pub hash: u64,
    pub offset: u64,
    pub length: u32,
}

pub fn write_lookup_table(entries: &[LookupEntry], file: &mut File) -> Result<()> {
    let num_slots = if entries.is_empty() {
        16
    } else {
        (entries.len() as f64 / LOAD_FACTOR).ceil() as usize
    };

    file.write_all(MAGIC)?;
    file.write_all(&(num_slots as u64).to_le_bytes())?;

    let mut slots = vec![0u8; num_slots * SLOT_SIZE];
    for entry in entries {
        let mut idx = (entry.hash as usize) % num_slots;
        loop {
            let off = idx * SLOT_SIZE;
            let stored = u64::from_le_bytes(slots[off..off + 8].try_into().unwrap());
            if stored == 0 {
                slots[off..off + 8].copy_from_slice(&entry.hash.to_le_bytes());
                slots[off + 8..off + 16].copy_from_slice(&entry.offset.to_le_bytes());
                slots[off + 16..off + 20].copy_from_slice(&entry.length.to_le_bytes());
                break;
            }
            idx = (idx + 1) % num_slots;
        }
    }

    file.write_all(&slots)?;
    file.flush()?;
    Ok(())
}

pub struct MmapLookupTable {
    mmap: Mmap,
    num_slots: usize,
}

impl MmapLookupTable {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        if mmap.len() < 12 || &mmap[0..4] != MAGIC {
            anyhow::bail!("invalid lookup table: bad magic");
        }
        let num_slots = u64::from_le_bytes(mmap[4..12].try_into().unwrap()) as usize;
        if mmap.len() < 12 + num_slots * SLOT_SIZE {
            anyhow::bail!("lookup table truncated");
        }
        Ok(Self { mmap, num_slots })
    }

    pub fn lookup(&self, hash: u64) -> Option<(u64, u32)> {
        if hash == 0 || self.num_slots == 0 {
            return None;
        }
        let data = &self.mmap[12..];
        let mut idx = (hash as usize) % self.num_slots;
        loop {
            let off = idx * SLOT_SIZE;
            let stored = u64::from_le_bytes(data[off..off + 8].try_into().unwrap());
            if stored == 0 {
                return None;
            }
            if stored == hash {
                let offset = u64::from_le_bytes(data[off + 8..off + 16].try_into().unwrap());
                let length = u32::from_le_bytes(data[off + 16..off + 20].try_into().unwrap());
                return Some((offset, length));
            }
            idx = (idx + 1) % self.num_slots;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_lookup_build_and_query() {
        let entries = vec![
            LookupEntry {
                hash: 100,
                offset: 0,
                length: 50,
            },
            LookupEntry {
                hash: 200,
                offset: 50,
                length: 30,
            },
            LookupEntry {
                hash: 300,
                offset: 80,
                length: 20,
            },
        ];
        let mut file = NamedTempFile::new().unwrap();
        write_lookup_table(&entries, file.as_file_mut()).unwrap();
        let table = MmapLookupTable::open(file.path()).unwrap();
        assert_eq!(table.lookup(100), Some((0, 50)));
        assert_eq!(table.lookup(200), Some((50, 30)));
        assert_eq!(table.lookup(300), Some((80, 20)));
        assert_eq!(table.lookup(999), None);
    }

    #[test]
    fn test_lookup_empty() {
        let mut file = NamedTempFile::new().unwrap();
        write_lookup_table(&[], file.as_file_mut()).unwrap();
        let table = MmapLookupTable::open(file.path()).unwrap();
        assert_eq!(table.lookup(123), None);
    }

    #[test]
    fn test_lookup_hash_zero_returns_none() {
        let mut file = NamedTempFile::new().unwrap();
        write_lookup_table(
            &[LookupEntry {
                hash: 1,
                offset: 0,
                length: 10,
            }],
            file.as_file_mut(),
        )
        .unwrap();
        let table = MmapLookupTable::open(file.path()).unwrap();
        assert_eq!(table.lookup(0), None);
    }
}
