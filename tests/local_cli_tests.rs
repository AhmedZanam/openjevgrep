use clap::Parser;
use ojg_core::cli::{Cli, Command, ModelAction};

#[test]
fn parses_model_install() {
    let cli = Cli::try_parse_from(["ojg", "model", "install"]).unwrap();

    assert!(matches!(
        cli.command,
        Some(Command::Model(command)) if matches!(command.action, ModelAction::Install)
    ));
}

#[test]
fn parses_serve_options() {
    let cli = Cli::try_parse_from([
        "ojg",
        "serve",
        "--host",
        "127.0.0.1",
        "--port",
        "8080",
        "--daemon",
    ])
    .unwrap();

    match cli.command.unwrap() {
        Command::Serve(command) => {
            assert_eq!(command.host, "127.0.0.1");
            assert_eq!(command.port, 8080);
            assert!(command.daemon);
        }
        _ => panic!("expected serve command"),
    }
}

#[test]
fn parses_status() {
    let cli = Cli::try_parse_from(["ojg", "status"]).unwrap();

    assert!(matches!(cli.command, Some(Command::Status(_))));
}
