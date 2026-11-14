//! owo: one world, one identity. The CLI connects the process to the library: it parses
//! arguments, builds `owo::Env` from the environment, hands over stdio and the agent process,
//! prints the output and returns the exit code.

use clap::Parser;
use owo::{Cli, Command, Output};
use std::io::Write;
use std::path::PathBuf;
use std::process::{ExitCode, Stdio};

fn main() -> ExitCode {
    let cli = Cli::parse();
    // Exit quietly when stdout closes early (`owo search x | head`). `owo mcp` and `owo acp` keep
    // the ignored signal: a closed pipe is a write error there, not a kill.
    #[cfg(unix)]
    if !matches!(cli.command, Command::Mcp { .. } | Command::Acp { .. }) {
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
        }
    }
    let env = env();
    let out = match &cli.command {
        Command::Mcp { remote, port } => {
            let served = match remote {
                Some(addr) => mcp_http(addr, *port, &cli, &env),
                None => mcp_stdio(&cli, &env),
            };
            match served {
                Ok(()) => return ExitCode::SUCCESS,
                Err(e) => Output::from(e),
            }
        }
        Command::Acp { inject, agent } => match acp(&cli, &env, agent, *inject) {
            Ok(code) => return ExitCode::from(code),
            Err(e) => Output::from(e),
        },
        _ => owo::run(&cli, &env, &mut std::io::stdin()),
    };
    match out
        .error
        .as_ref()
        .and_then(|e| e.downcast_ref::<clap::Error>())
    {
        // clap prints its own error, so it keeps its colour on a terminal.
        Some(e) => {
            eprint!("{}", out.warning_lines());
            let _ = e.print();
        }
        None => {
            let (stdout, stderr) = out.render(cli.json);
            eprint!("{stderr}");
            print!("{stdout}");
        }
    }
    ExitCode::from(out.code as u8)
}

/// The environment values the library uses, read from the process.
fn env() -> owo::Env {
    let non_empty = |key: &str| std::env::var(key).ok().filter(|v| !v.is_empty());
    owo::Env {
        owo_root: non_empty("OWO_ROOT"),
        owo_identity: non_empty("OWO_IDENTITY"),
        owo_project: non_empty("OWO_PROJECT"),
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        home: std::env::var_os("HOME").map(PathBuf::from),
        xdg_config_home: non_empty("XDG_CONFIG_HOME").map(PathBuf::from),
        xdg_data_home: non_empty("XDG_DATA_HOME").map(PathBuf::from),
    }
}

/// Serves MCP on the process's stdin and stdout until stdin closes.
fn mcp_stdio(cli: &Cli, env: &owo::Env) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        let (input, output) = own_stdio()?;
        owo::mcp::serve(std::io::BufReader::new(input), output, cli, env)
    }
    #[cfg(not(unix))]
    owo::mcp::serve(std::io::stdin().lock(), std::io::stdout(), cli, env)
}

/// Serves MCP over HTTP on `addr`, after it prints the URL.
fn mcp_http(addr: &str, port: Option<u16>, cli: &Cli, env: &owo::Env) -> anyhow::Result<()> {
    let listener = owo::mcp::bind(addr, port)?;
    println!("http://{}/mcp", listener.local_addr()?);
    std::io::stdout().flush()?;
    #[cfg(unix)]
    detach_stdio()?;
    owo::mcp::serve_http(listener, cli, env)
}

/// Points the process's stdin at `/dev/null` and its stdout at stderr. A tool call runs in this
/// process, so a tool path such as `/dev/stdin` or `/dev/stdout` must not reach the server's
/// own streams.
#[cfg(unix)]
fn detach_stdio() -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    let null = std::fs::File::open("/dev/null")?;
    // SAFETY: `dup2` only replaces descriptors 0 and 1 with copies of open descriptors.
    if unsafe { libc::dup2(null.as_raw_fd(), 0) < 0 || libc::dup2(2, 1) < 0 } {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// The server's own stdin and stdout for the protocol, taken before `detach_stdio`. The copies
/// are close-on-exec, so a `git` child does not inherit them.
#[cfg(unix)]
fn own_stdio() -> std::io::Result<(std::fs::File, std::fs::File)> {
    use std::os::fd::AsFd;
    let input = std::io::stdin().as_fd().try_clone_to_owned()?;
    let output = std::io::stdout().as_fd().try_clone_to_owned()?;
    detach_stdio()?;
    Ok((input.into(), output.into()))
}

/// Runs the agent behind the ACP proxy until it exits, and returns its exit code, or 1 when a
/// signal killed it.
fn acp(cli: &Cli, env: &owo::Env, agent: &[String], inject: bool) -> anyhow::Result<u8> {
    use anyhow::Context;
    let proxy = owo::acp::Proxy::new(cli, env, owo::acp::Options { inject }, std::io::stderr())?;
    let mut child = std::process::Command::new(&agent[0])
        .args(&agent[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("starting {}", agent[0]))?;
    let agent_in = child.stdin.take().expect("piped stdin");
    let agent_out = std::io::BufReader::new(child.stdout.take().expect("piped stdout"));
    let client_in = std::io::BufReader::new(std::io::stdin());
    let copied = proxy.run(client_in, std::io::stdout(), agent_in, agent_out);
    // A client that closed its stdout ends the session: stop the agent.
    if copied.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    copied?;
    Ok(status.code().unwrap_or(1) as u8)
}
