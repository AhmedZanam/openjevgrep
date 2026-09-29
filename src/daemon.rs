use std::ffi::OsString;
use std::process::{Command, Stdio};

use anyhow::Result;

use crate::cli::ServeCommand;

pub fn spawn_server(args: &ServeCommand) -> Result<u32> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(server_arguments(args))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    configure_detached_process(&mut command);

    Ok(command.spawn()?.id())
}

fn server_arguments(args: &ServeCommand) -> Vec<OsString> {
    vec![
        OsString::from("serve"),
        OsString::from("--host"),
        OsString::from(&args.host),
        OsString::from("--port"),
        OsString::from(args.port.to_string()),
        OsString::from("--model"),
        OsString::from(&args.model),
    ]
}

#[cfg(unix)]
fn configure_detached_process(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(windows)]
fn configure_detached_process(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    command.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
}

#[cfg(not(any(unix, windows)))]
fn configure_detached_process(_: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_arguments_do_not_reenter_daemon_mode() {
        let args = ServeCommand {
            host: "127.0.0.1".to_string(),
            port: 9090,
            model: "verdict-1.4".to_string(),
            daemon: true,
        };

        let values = server_arguments(&args)
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            values,
            vec![
                "serve",
                "--host",
                "127.0.0.1",
                "--port",
                "9090",
                "--model",
                "verdict-1.4"
            ]
        );
    }
}
