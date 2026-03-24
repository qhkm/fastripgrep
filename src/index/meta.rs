use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexMeta {
    pub version: u32,
    pub commit_hash: Option<String>,
    pub file_count: u32,
    pub ngram_count: u32,
    pub timestamp: u64,
}

impl IndexMeta {
    pub fn write(&self, path: &std::path::Path) -> anyhow::Result<()> {
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
    pub fn read(path: &std::path::Path) -> anyhow::Result<Self> {
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    }
    pub fn timestamp_now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_meta_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("meta.json");
        let meta = IndexMeta {
            version: 1,
            commit_hash: Some("abc123".to_string()),
            file_count: 42,
            ngram_count: 100,
            timestamp: IndexMeta::timestamp_now(),
        };
        meta.write(&path).unwrap();
        let loaded = IndexMeta::read(&path).unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.commit_hash.as_deref(), Some("abc123"));
        assert_eq!(loaded.file_count, 42);
        assert_eq!(loaded.ngram_count, 100);
    }

    #[test]
    fn test_meta_none_commit_hash() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("meta.json");
        let meta = IndexMeta {
            version: 1,
            commit_hash: None,
            file_count: 0,
            ngram_count: 0,
            timestamp: 0,
        };
        meta.write(&path).unwrap();
        let loaded = IndexMeta::read(&path).unwrap();
        assert!(loaded.commit_hash.is_none());
    }
}
