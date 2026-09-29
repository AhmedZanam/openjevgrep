use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use crate::backend::Score;
use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheKey {
    pub endpoint: String,
    pub model: String,
    pub query: String,
    pub candidate_hash: String,
}

impl CacheKey {
    pub fn digest(&self) -> String {
        let mut hasher = Sha256::new();
        for value in [&self.endpoint, &self.model, &self.query, &self.candidate_hash] {
            hasher.update(value.as_bytes());
            hasher.update([0]);
        }
        format!("{:x}", hasher.finalize())
    }
}

#[async_trait]
pub trait ScoreCache: Send + Sync {
    async fn get(&self, key: &CacheKey) -> Result<Option<Score>>;
    async fn put(&self, key: &CacheKey, score: &Score) -> Result<()>;
    async fn clear(&self) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct FileScoreCache {
    directory: PathBuf,
}

impl FileScoreCache {
    pub fn new(directory: &Path) -> Result<Self> {
        fs::create_dir_all(directory)?;
        Ok(Self {
            directory: directory.to_path_buf(),
        })
    }

    pub fn entry_path(&self, key: &CacheKey) -> PathBuf {
        self.directory.join(format!("{}.json", key.digest()))
    }
}

#[async_trait]
impl ScoreCache for FileScoreCache {
    async fn get(&self, key: &CacheKey) -> Result<Option<Score>> {
        let path = self.entry_path(key);
        if !path.is_file() {
            return Ok(None);
        }
        let bytes = fs::read(path)?;
        Ok(serde_json::from_slice(&bytes).ok())
    }

    async fn put(&self, key: &CacheKey, score: &Score) -> Result<()> {
        let final_path = self.entry_path(key);
        if final_path.is_file() {
            return Ok(());
        }
        let temp_path = self
            .directory
            .join(format!(".{}.{}.tmp", key.digest(), std::process::id()));
        let bytes = serde_json::to_vec(score)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        match fs::rename(&temp_path, &final_path) {
            Ok(()) => Ok(()),
            Err(error) if final_path.exists() => {
                let _ = fs::remove_file(&temp_path);
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    Ok(())
                } else {
                    Ok(())
                }
            }
            Err(error) => {
                let _ = fs::remove_file(&temp_path);
                Err(error.into())
            }
        }
    }

    async fn clear(&self) -> Result<()> {
        if self.directory.exists() {
            fs::remove_dir_all(&self.directory)?;
        }
        fs::create_dir_all(&self.directory)?;
        Ok(())
    }
}

pub fn default_cache_directory() -> Result<PathBuf> {
    let base = dirs::cache_dir().ok_or_else(|| anyhow::anyhow!("cache directory is unavailable"))?;
    Ok(base.join("openjevgrep").join("scores"))
}
