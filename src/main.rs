//! owo: one world, one identity. The CLI: it parses arguments, calls `owo::run`, and prints
//! the output. `owo mcp` is a server, so it runs here instead.

mod mcp;

use clap::Parser;
use owo::{Cli, Command, Output, SyncAction};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    // Exit quietly when stdout closes early (`owo search x | head`).
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let cli = Cli::parse();
    let out = match &cli.command {
        Command::Mcp { remote, port } => {
            let served = match remote {
                Some(addr) => mcp::serve_http(addr, *port, &cli),
                None => mcp::serve(&cli),
            };
            match served {
                Ok(()) => return ExitCode::SUCCESS,
                Err(e) => Output::from(e),
            }
        }
        _ => owo::run(&cli, env(&cli)),
    };
    print(&cli, &out);
    ExitCode::from(out.code as u8)
}

/// The environment values `owo::run` uses, and stdin only for a command that reads it.
fn env(cli: &Cli) -> owo::Env {
    let non_empty = |key: &str| std::env::var(key).ok().filter(|v| !v.is_empty());
    let reads_stdin = matches!(
        &cli.command,
        Command::Write { content: None, .. }
            | Command::Sync {
                action: SyncAction::Push { turns: None, .. }
            }
    );
    owo::Env {
        owo_root: non_empty("OWO_ROOT"),
        owo_identity: non_empty("OWO_IDENTITY"),
        owo_project: non_empty("OWO_PROJECT"),
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        home: std::env::var_os("HOME").map(PathBuf::from),
        xdg_config_home: non_empty("XDG_CONFIG_HOME").map(PathBuf::from),
        xdg_data_home: non_empty("XDG_DATA_HOME").map(PathBuf::from),
        stdin: reads_stdin.then(|| std::io::read_to_string(std::io::stdin())),
    }
}

/// Warnings to stderr; then a usage error, the JSON envelope, or the error line and the text.
fn print(cli: &Cli, out: &Output) {
    for w in &out.warnings {
        eprintln!("owo: warning: {w}");
    }
    if let Some(usage) = out
        .error
        .as_ref()
        .and_then(|e| e.downcast_ref::<clap::Error>())
    {
        let _ = usage.print();
    } else if cli.json {
        let text = serde_json::to_string_pretty(&out.envelope).expect("envelope serializes");
        println!("{text}");
    } else {
        if let Some(e) = &out.error {
            eprintln!("owo: {e:#}");
        }
        print!("{}", out.text);
    }
}
