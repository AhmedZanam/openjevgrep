use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("ojg-eval: no evaluation suite configured");
    ExitCode::from(2)
}
