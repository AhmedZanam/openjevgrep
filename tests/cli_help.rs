use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn help_names_the_primary_binary_and_search_modes() {
    let mut command = Command::cargo_bin("ojg").expect("ojg binary");
    command
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("OpenJevGrep"))
        .stdout(contains("search"))
        .stdout(contains("exact"));
}
