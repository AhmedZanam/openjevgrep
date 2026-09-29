use std::collections::BTreeMap;
use std::path::Path;

use ojg_core::model::{default_model_directory, ModelId, ModelStore, MODEL_REVISION};
use sha2::{Digest, Sha256};
use tempfile::tempdir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn fixture_hashes() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("model.onnx".to_string(), digest(b"model")),
        ("tokenizer.json".to_string(), digest(b"tokenizer")),
        ("calibrator.json".to_string(), digest(b"calibrator")),
    ])
}

async fn mount_fixture(server: &MockServer) {
    for (name, body) in [
        ("model.onnx", "model"),
        ("tokenizer.json", "tokenizer"),
        ("calibrator.json", "calibrator"),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/{}", name)))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(server)
            .await;
    }
}

#[test]
fn model_identity_and_cache_paths_are_stable() {
    assert_eq!(ModelId::Verdict14.as_str(), "verdict-1.4");
    assert_eq!(ModelId::parse("verdict-1.4").unwrap(), ModelId::Verdict14);
    assert!(default_model_directory()
        .unwrap()
        .ends_with(Path::new("openjevgrep").join("models")));

    let root = tempdir().unwrap();
    let store = ModelStore::for_testing(root.path(), "http://127.0.0.1", fixture_hashes());
    assert_eq!(
        store.model_directory(ModelId::Verdict14),
        root.path().join("verdict-1.4")
    );
    assert_eq!(MODEL_REVISION, "8af2496eb63c7fa66d7d234e1f62629380030eb4");
}

#[tokio::test]
async fn downloads_atomically_and_writes_manifest_last() {
    let server = MockServer::start().await;
    mount_fixture(&server).await;
    let root = tempdir().unwrap();
    let store = ModelStore::for_testing(root.path(), server.uri(), fixture_hashes());

    let files = store.ensure(ModelId::Verdict14).await.unwrap();

    assert!(files.model_path.is_file());
    assert!(files.tokenizer_path.is_file());
    assert!(files.calibrator_path.is_file());
    assert!(store.manifest_path(ModelId::Verdict14).is_file());
    assert!(!root
        .path()
        .join("verdict-1.4")
        .read_dir()
        .unwrap()
        .any(|entry| entry.unwrap().file_name().to_string_lossy().ends_with(".part")));
}

#[tokio::test]
async fn rejects_checksum_mismatch_without_activation() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/model.onnx"))
        .respond_with(ResponseTemplate::new(200).set_body_string("tampered"))
        .mount(&server)
        .await;
    for (name, body) in [("tokenizer.json", "tokenizer"), ("calibrator.json", "calibrator")] {
        Mock::given(method("GET"))
            .and(path(format!("/{}", name)))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;
    }
    let root = tempdir().unwrap();
    let store = ModelStore::for_testing(root.path(), server.uri(), fixture_hashes());

    let error = store.ensure(ModelId::Verdict14).await.unwrap_err();

    assert!(error.to_string().contains("checksum mismatch"));
    assert!(!store.manifest_path(ModelId::Verdict14).exists());
    assert!(!store.model_directory(ModelId::Verdict14).join("model.onnx").exists());
}

#[tokio::test]
async fn recovers_from_corrupt_existing_file() {
    let server = MockServer::start().await;
    mount_fixture(&server).await;
    let root = tempdir().unwrap();
    let store = ModelStore::for_testing(root.path(), server.uri(), fixture_hashes());
    std::fs::create_dir_all(store.model_directory(ModelId::Verdict14)).unwrap();
    std::fs::write(
        store.model_directory(ModelId::Verdict14).join("model.onnx"),
        b"corrupt",
    )
    .unwrap();

    let files = store.ensure(ModelId::Verdict14).await.unwrap();

    assert_eq!(std::fs::read(files.model_path).unwrap(), b"model");
    assert!(store.manifest_path(ModelId::Verdict14).exists());
}
