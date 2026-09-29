use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::anyhow;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::Result;

pub const MODEL_REVISION: &str = "8af2496eb63c7fa66d7d234e1f62629380030eb4";
const MODEL_REPOSITORY: &str = "https://huggingface.co/heman10x/rlcd-modernbert-151m";
const MODEL_SHA256: &str = "4ae01f822538b000fa0e55859d4b3e6b40871d860149397e8784428b2a42ee5e";
const TOKENIZER_SHA256: &str = "8bb449eb0c037aae44115b65905bb339b8f3f74eb37067c19127feb3c0755723";
const CALIBRATOR_SHA256: &str = "af2a876993148efa0726b6ccf710fe2303897d20c0ce8c7c9036eb50f64d23de";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelId {
    Verdict14,
}

impl ModelId {
    pub fn parse(value: &str) -> Result<Self> {
        value.parse()
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Verdict14 => "verdict-1.4",
        }
    }
}

impl FromStr for ModelId {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "verdict-1.4" => Ok(Self::Verdict14),
            _ => Err(anyhow!("unsupported model: {value}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFiles {
    pub directory: PathBuf,
    pub model_path: PathBuf,
    pub tokenizer_path: PathBuf,
    pub calibrator_path: PathBuf,
}

#[derive(Clone)]
pub struct ModelStore {
    root: PathBuf,
    base_url: String,
    expected_hashes: BTreeMap<String, String>,
    client: Client,
}

#[derive(Debug, Deserialize, Serialize)]
struct Manifest {
    revision: String,
    hashes: BTreeMap<String, String>,
}

pub fn default_model_directory() -> Result<PathBuf> {
    dirs::cache_dir()
        .map(|path| path.join("openjevgrep").join("models"))
        .ok_or_else(|| anyhow!("unable to determine the platform cache directory"))
}

impl ModelStore {
    pub fn from_default() -> Result<Self> {
        Ok(Self::new(default_model_directory()?))
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            base_url: format!("{MODEL_REPOSITORY}/resolve/{MODEL_REVISION}"),
            expected_hashes: BTreeMap::from([
                ("model.onnx".to_string(), MODEL_SHA256.to_string()),
                ("tokenizer.json".to_string(), TOKENIZER_SHA256.to_string()),
                ("calibrator.json".to_string(), CALIBRATOR_SHA256.to_string()),
            ]),
            client: Client::new(),
        }
    }

    pub fn for_testing(
        root: impl Into<PathBuf>,
        base_url: impl Into<String>,
        expected_hashes: BTreeMap<String, String>,
    ) -> Self {
        Self {
            root: root.into(),
            base_url: base_url.into(),
            expected_hashes,
            client: Client::new(),
        }
    }

    pub fn model_directory(&self, model: ModelId) -> PathBuf {
        self.root.join(model.as_str())
    }

    pub fn manifest_path(&self, model: ModelId) -> PathBuf {
        self.model_directory(model).join("manifest.json")
    }

    pub async fn ensure(&self, model: ModelId) -> Result<ModelFiles> {
        let directory = self.model_directory(model);
        tokio::fs::create_dir_all(&directory).await?;
        let files = self.files(model);
        if self.is_activated(model, &files)? {
            return Ok(files);
        }

        let _ = tokio::fs::remove_file(self.manifest_path(model)).await;
        for filename in self.expected_hashes.keys() {
            let _ = tokio::fs::remove_file(directory.join(filename)).await;
            let _ = tokio::fs::remove_file(directory.join(format!(".{filename}.part"))).await;
        }

        for (filename, expected_hash) in &self.expected_hashes {
            self.download_file(&directory, filename, expected_hash)
                .await?;
        }

        let manifest = Manifest {
            revision: MODEL_REVISION.to_string(),
            hashes: self.expected_hashes.clone(),
        };
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
        let temporary = directory.join(".manifest.json.part");
        let mut file = tokio::fs::File::create(&temporary).await?;
        file.write_all(&manifest_bytes).await?;
        file.flush().await?;
        file.sync_all().await?;
        tokio::fs::rename(temporary, self.manifest_path(model)).await?;
        Ok(files)
    }

    fn files(&self, model: ModelId) -> ModelFiles {
        let directory = self.model_directory(model);
        ModelFiles {
            model_path: directory.join("model.onnx"),
            tokenizer_path: directory.join("tokenizer.json"),
            calibrator_path: directory.join("calibrator.json"),
            directory,
        }
    }

    fn is_activated(&self, model: ModelId, files: &ModelFiles) -> Result<bool> {
        let manifest_path = self.manifest_path(model);
        if !manifest_path.is_file() {
            return Ok(false);
        }
        let manifest: Manifest = serde_json::from_slice(&std::fs::read(manifest_path)?)?;
        if manifest.revision != MODEL_REVISION || manifest.hashes != self.expected_hashes {
            return Ok(false);
        }
        for path in [
            &files.model_path,
            &files.tokenizer_path,
            &files.calibrator_path,
        ] {
            if !path.is_file() {
                return Ok(false);
            }
        }
        for (filename, expected_hash) in &self.expected_hashes {
            let actual_hash = sha256_file(&files.directory.join(filename))?;
            if &actual_hash != expected_hash {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn download_file(
        &self,
        directory: &Path,
        filename: &str,
        expected_hash: &str,
    ) -> Result<()> {
        let url = format!("{}/{}", self.base_url.trim_end_matches('/'), filename);
        let response = self.client.get(url).send().await?.error_for_status()?;
        let temporary = directory.join(format!(".{filename}.part"));
        let mut file = tokio::fs::File::create(&temporary).await?;
        let mut hasher = Sha256::new();
        let result = async {
            let mut response = response;
            while let Some(chunk) = response.chunk().await? {
                hasher.update(&chunk);
                file.write_all(&chunk).await?;
            }
            file.flush().await?;
            file.sync_all().await?;
            Ok::<String, anyhow::Error>(format!("{:x}", hasher.finalize()))
        }
        .await;
        let actual_hash = match result {
            Ok(hash) => hash,
            Err(error) => {
                let _ = tokio::fs::remove_file(&temporary).await;
                return Err(error.context(format!("downloading {filename}")));
            }
        };
        if actual_hash != expected_hash {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(anyhow!(
                "checksum mismatch for {filename}: expected {expected_hash}, got {actual_hash}"
            ));
        }
        tokio::fs::rename(temporary, directory.join(filename)).await?;
        Ok(())
    }
}

fn sha256_file(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
