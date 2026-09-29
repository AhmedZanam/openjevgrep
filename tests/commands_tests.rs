use ojg_core::commands::{inspect_repository, InspectOptions};
use tempfile::tempdir;

#[test]
fn inspect_reports_files_chunks_and_estimated_requests() {
    let root = tempdir().expect("root");
    std::fs::write(root.path().join("auth.rs"), "fn validate() {}\n").expect("source");
    let report = inspect_repository(
        root.path(),
        &InspectOptions {
            batch_size: 4,
            ..InspectOptions::default()
        },
    )
    .expect("inspect");

    assert_eq!(report.files_included, 1);
    assert_eq!(report.chunks_found, 1);
    assert_eq!(report.estimated_requests, 1);
    assert!(report.estimated_input_bytes > 0);
}
