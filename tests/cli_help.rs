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
        .stdout(contains("exact"))
        .stdout(contains("model"))
        .stdout(contains("serve"))
        .stdout(contains("status"));
}

#[test]
fn completions_emit_shell_script() {
    let mut command = Command::cargo_bin("ojg").expect("ojg binary");
    command
        .args(["completions", "powershell"])
        .assert()
        .success()
        .stdout(contains("ojg"));
}
