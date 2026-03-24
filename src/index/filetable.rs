use anyhow::Result;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"RSFT";

pub struct FileTableEntry {
    pub path: PathBuf,
    pub mtime: u64,
    pub size: u64,
}

#[derive(Default)]
pub struct FileTableBuilder {
    entries: Vec<FileTableEntry>,
}

impl FileTableBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, path: &str, mtime: u64, size: u64) -> u32 {
        let id = self.entries.len() as u32;
        self.entries.push(FileTableEntry {
            path: PathBuf::from(path),
            mtime,
            size,
        });
        id
    }

    pub fn write(&self, file: &mut File) -> Result<()> {
        file.write_all(MAGIC)?;
        file.write_all(&(self.entries.len() as u32).to_le_bytes())?;
        for entry in &self.entries {
            let path_bytes = entry.path.to_string_lossy().into_owned().into_bytes();
            file.write_all(&(path_bytes.len() as u32).to_le_bytes())?;
            file.write_all(&path_bytes)?;
            file.write_all(&entry.mtime.to_le_bytes())?;
            file.write_all(&entry.size.to_le_bytes())?;
        }
        file.flush()?;
        Ok(())
    }
}

pub struct FileTableReader {
    entries: Vec<FileTableEntry>,
}

impl FileTableReader {
    pub fn open(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;

        if data.len() < 8 || &data[0..4] != MAGIC {
            anyhow::bail!("invalid file table: bad magic");
        }

        let count = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
        let mut offset = 8;
        let mut entries = Vec::with_capacity(count);

        for _ in 0..count {
            if offset + 4 > data.len() {
                anyhow::bail!("file table truncated");
            }
            let path_len = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
            offset += 4;

            if offset + path_len + 16 > data.len() {
                anyhow::bail!("file table truncated");
            }
            let path = PathBuf::from(String::from_utf8_lossy(&data[offset..offset + path_len]).into_owned());
            offset += path_len;

            let mtime = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
            offset += 8;

            let size = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
            offset += 8;

            entries.push(FileTableEntry { path, mtime, size });
        }

        Ok(Self { entries })
    }

    pub fn get(&self, id: u32) -> Option<&FileTableEntry> {
        self.entries.get(id as usize)
    }

    pub fn all_file_ids(&self) -> Vec<u32> {
        (0..self.entries.len() as u32).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::path::PathBuf;

    #[test]
    fn test_filetable_roundtrip() {
        let mut table = FileTableBuilder::new();
        let id1 = table.add("src/main.rs", 1000, 500);
        let id2 = table.add("src/lib.rs", 2000, 300);
        let mut file = NamedTempFile::new().unwrap();
        table.write(file.as_file_mut()).unwrap();

        let reader = FileTableReader::open(file.path()).unwrap();
        let e1 = reader.get(id1).unwrap();
        assert_eq!(e1.path, PathBuf::from("src/main.rs"));
        assert_eq!(e1.mtime, 1000);
        let e2 = reader.get(id2).unwrap();
        assert_eq!(e2.path, PathBuf::from("src/lib.rs"));
    }

    #[test]
    fn test_filetable_all_ids() {
        let mut table = FileTableBuilder::new();
        table.add("a.rs", 0, 0);
        table.add("b.rs", 0, 0);
        let mut file = NamedTempFile::new().unwrap();
        table.write(file.as_file_mut()).unwrap();
        let reader = FileTableReader::open(file.path()).unwrap();
        assert_eq!(reader.all_file_ids(), vec![0, 1]);
    }
}
