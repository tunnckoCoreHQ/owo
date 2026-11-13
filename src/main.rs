//! owo: one world, one identity. The CLI: it parses arguments, calls `owo::run`, and prints
//! the output. `owo mcp` is a server and `owo acp` a proxy, so they run here instead.

mod acp;
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
    // `owo mcp` and `owo acp` ignore the signal: a closed pipe is a write error there, not a kill.
    #[cfg(unix)]
    if matches!(cli.command, Command::Mcp { .. } | Command::Acp { .. }) {
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_IGN);
        }
    }
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
        Command::Acp { mcp, agent } => match acp::serve(&cli, agent, *mcp) {
            Ok(code) => return ExitCode::from(code),
            Err(e) => Output::from(e),
        },
        _ => owo::run(
            &cli,
            env(&cli, || std::io::read_to_string(std::io::stdin())),
        ),
    };
    match usage(&out) {
        // clap prints its own error, so it keeps its colour on a terminal.
        Some(e) => {
            eprint!("{}", warnings(&out));
            let _ = e.print();
        }
        None => {
            let (stdout, stderr) = render(&cli, &out);
            eprint!("{stderr}");
            print!("{stdout}");
        }
    }
    ExitCode::from(out.code as u8)
}

/// The environment values `owo::run` uses, and `stdin` read only for a command that takes it.
fn env(cli: &Cli, stdin: impl FnOnce() -> std::io::Result<String>) -> owo::Env {
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
        stdin: reads_stdin.then(stdin),
    }
}

/// The global `--root`, `-i` and `--project` flags of a command line that runs in process.
fn global_args(cli: &Cli, identity: Option<String>, project: Option<String>) -> Vec<String> {
    let mut out = vec![];
    if let Some(root) = &cli.root {
        out.extend(["--root".into(), root.display().to_string()]);
    }
    if let Some(i) = identity {
        out.extend(["-i".into(), i]);
    }
    if let Some(p) = project {
        out.extend(["--project".into(), p]);
    }
    out
}

/// Parses `argv` (without the program name) with clap and calls `owo::run` in process,
/// with `stdin` as the stdin text.
fn run_argv(argv: Vec<String>, stdin: String) -> Result<(Cli, Output), clap::Error> {
    let cli = Cli::try_parse_from(std::iter::once("owo".into()).chain(argv))?;
    let out = owo::run(&cli, env(&cli, || Ok(stdin)));
    Ok((cli, out))
}

/// The warnings of an `Output`, one `owo: warning: <text>` line each.
fn warnings(out: &Output) -> String {
    out.warnings
        .iter()
        .map(|w| format!("owo: warning: {w}\n"))
        .collect()
}

/// The clap usage error in an `Output`, if it holds one.
fn usage(out: &Output) -> Option<&clap::Error> {
    out.error.as_ref().and_then(|e| e.downcast_ref())
}

/// The stdout and stderr text of an `Output`. Stderr holds the warnings, then a usage error
/// or the error line; stdout holds the JSON envelope with `--json`, else the text.
fn render(cli: &Cli, out: &Output) -> (String, String) {
    let mut stderr = warnings(out);
    if let Some(usage) = usage(out) {
        return usage_text(usage, stderr);
    }
    let stdout = if cli.json {
        let text = serde_json::to_string_pretty(&out.envelope).expect("envelope serializes");
        format!("{text}\n")
    } else {
        if let Some(e) = &out.error {
            stderr.push_str(&format!("owo: {e:#}\n"));
        }
        out.text.clone()
    };
    (stdout, stderr)
}

/// The stdout and stderr text of a clap error, as clap prints it: help and version on stdout,
/// errors on stderr after `stderr`.
fn usage_text(usage: &clap::Error, mut stderr: String) -> (String, String) {
    let text = usage.render().to_string();
    if usage.use_stderr() {
        stderr.push_str(&text);
        (String::new(), stderr)
    } else {
        (text, stderr)
    }
}
