use assert_cmd::Command;
use predicates::str::contains;

fn fixture() -> String {
    format!("{}/fixtures/sample-repo", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn exact_command_works_against_fixture_repository() {
    let fixture = fixture();
    let mut command = Command::cargo_bin("ojg").expect("ojg");
    command
        .args(["exact", "validate_access_token", fixture.as_str()])
        .assert()
        .success()
        .stdout(contains("src/auth/middleware.rs:1:"));
}

#[test]
fn inspect_command_reports_fixture_counts() {
    let fixture = fixture();
    let mut command = Command::cargo_bin("ojg").expect("ojg");
    command
        .args(["inspect", fixture.as_str()])
        .assert()
        .success()
        .stdout(contains("files: 3"))
        .stdout(contains("chunks:"));
}
