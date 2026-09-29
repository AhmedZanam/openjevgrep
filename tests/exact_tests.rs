use ojg_core::exact::{exact_search, ExactOptions};
use tempfile::tempdir;

#[test]
fn fallback_exact_search_returns_literal_matches_and_line_numbers() {
    let root = tempdir().expect("root");
    std::fs::write(
        root.path().join("auth.rs"),
        "fn validate() {}\nresolveCurrentFacilityId();\nresolveCurrentFacilityId();\n",
    )
    .expect("source");
    let options = ExactOptions {
        use_rg: false,
        ..ExactOptions::default()
    };

    let matches = exact_search("resolveCurrentFacilityId", &[root.path().to_path_buf()], &options)
        .expect("exact matches");
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].line, 2);
    assert_eq!(matches[0].path, "auth.rs");
}

#[test]
fn exact_search_excludes_binary_files() {
    let root = tempdir().expect("root");
    std::fs::write(root.path().join("binary.dat"), b"secret\0secret").expect("binary");
    let options = ExactOptions {
        use_rg: false,
        ..ExactOptions::default()
    };

    let matches = exact_search("secret", &[root.path().to_path_buf()], &options).expect("search");
    assert!(matches.is_empty());
}
