use std::fs;

use ojg_core::scanner::{scan_repository, ExclusionReason, ScanOptions};
use tempfile::tempdir;

fn write(root: &std::path::Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent directory");
    }
    fs::write(path, bytes).expect("file");
}

#[test]
fn respects_ignore_files_and_default_directories() {
    let root = tempdir().expect("root");
    write(root.path(), ".gitignore", b"ignored.txt\n");
    write(root.path(), ".ignore", b"ignored-by-ignore.txt\n");
    write(root.path(), ".ojgignore", b"ignored-by-ojg.txt\n");
    write(root.path(), "kept.rs", b"fn kept() {}\n");
    write(root.path(), "ignored.txt", b"ignored\n");
    write(root.path(), "ignored-by-ignore.txt", b"ignored\n");
    write(root.path(), "ignored-by-ojg.txt", b"ignored\n");
    write(root.path(), "target/generated.rs", b"fn generated() {}\n");
    write(
        root.path(),
        "node_modules/package.js",
        b"module.exports = {};\n",
    );

    let report = scan_repository(root.path(), &ScanOptions::default()).expect("scan");
    let included: Vec<_> = report
        .included
        .iter()
        .map(|file| file.relative_path.to_string_lossy().replace('\\', "/"))
        .collect();

    assert_eq!(included, vec!["kept.rs"]);
    assert!(report
        .excluded
        .iter()
        .any(|item| item.reason == ExclusionReason::Ignored));
    assert!(report
        .excluded
        .iter()
        .any(|item| item.reason == ExclusionReason::DefaultDirectory));
}

#[test]
fn include_pattern_can_reenable_safe_default_directory() {
    let root = tempdir().expect("root");
    write(root.path(), "vendor/library.rs", b"pub fn library() {}\n");
    let options = ScanOptions {
        includes: vec!["vendor/**".to_string()],
        ..ScanOptions::default()
    };

    let report = scan_repository(root.path(), &options).expect("scan");
    assert_eq!(report.included.len(), 1);
    assert_eq!(
        report.included[0]
            .relative_path
            .to_string_lossy()
            .replace('\\', "/"),
        "vendor/library.rs"
    );
}

#[test]
fn excludes_secret_binary_invalid_utf8_and_oversized_files() {
    let root = tempdir().expect("root");
    write(root.path(), ".env", b"TOKEN=secret\n");
    write(root.path(), "private.pem", b"key\n");
    write(root.path(), "binary.dat", b"abc\0def");
    write(root.path(), "invalid.txt", &[0xff, 0xfe]);
    write(root.path(), "large.txt", b"123456789");
    write(root.path(), "good.txt", b"good\n");
    let options = ScanOptions {
        max_file_size: 8,
        ..ScanOptions::default()
    };

    let report = scan_repository(root.path(), &options).expect("scan");
    assert_eq!(report.included.len(), 1);
    assert_eq!(
        report.included[0].relative_path.to_string_lossy(),
        "good.txt"
    );
    assert!(report
        .excluded
        .iter()
        .any(|item| item.reason == ExclusionReason::Sensitive));
    assert!(report
        .excluded
        .iter()
        .any(|item| item.reason == ExclusionReason::Binary));
    assert!(report
        .excluded
        .iter()
        .any(|item| item.reason == ExclusionReason::InvalidUtf8));
    assert!(report
        .excluded
        .iter()
        .any(|item| item.reason == ExclusionReason::TooLarge));
}

#[cfg(unix)]
#[test]
fn does_not_follow_symlinks_by_default() {
    let root = tempdir().expect("root");
    std::os::unix::fs::symlink(".", root.path().join("cycle")).expect("symlink");
    write(root.path(), "kept.rs", b"fn kept() {}\n");

    let report =
        ojg_core::scanner::scan_repository(root.path(), &ScanOptions::default()).expect("scan");
    assert_eq!(report.included.len(), 1);
    assert!(report
        .excluded
        .iter()
        .any(|item| item.reason == ExclusionReason::Symlink));
}
