//! owo: one world, one identity. The binary is the CLI of the owo core: it builds its commands
//! from the core's operations, maps the parsed arguments into their requests, runs them and
//! renders the outcome as text or the JSON envelope, with an exit code. `main` also connects
//! the process (arguments, environment, stdio, the agent process) to the core's MCP server and
//! ACP proxy.

mod cli;
mod output;
mod render;
mod text;

use clap::ArgMatches;
use cli::Global;
use output::Output;
use owo_core::{mcp, operations, Args, Operation};
use std::io::Write;
use std::path::PathBuf;
use std::process::{ExitCode, Stdio};

fn main() -> ExitCode {
    let ops = operations();
    let mut command = cli::command(&ops);
    let matches = command.get_matches_mut();
    let global = Global::of(&matches);
    let (name, sub) = matches.subcommand().expect("a command is required");
    // Exit quietly when stdout closes early (`owo search x | head`). `owo mcp` and `owo acp` keep
    // the ignored signal: a closed pipe is a write error there, not a kill.
    #[cfg(unix)]
    if !matches!(name, "mcp" | "acp") {
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
        }
    }
    let env = env();
    let out = match name {
        "mcp" => {
            let served = match sub.get_one::<String>("remote") {
                Some(addr) => mcp_http(addr, sub.get_one::<u16>("port").copied(), &global, &env),
                None => mcp_stdio(&global, &env),
            };
            match served {
                Ok(()) => return ExitCode::SUCCESS,
                Err(e) => Output::from(e),
            }
        }
        "acp" => {
            let agent: Vec<String> = sub.get_many("agent").unwrap().cloned().collect();
            match acp(&global, &env, &agent, sub.get_flag("inject")) {
                Ok(code) => return ExitCode::from(code),
                Err(e) => Output::from(e),
            }
        }
        _ => run(&ops, &matches, &global, &env),
    };
    match out.usage() {
        // A usage error is worded as clap words its own, so it keeps its colour on a terminal.
        Some(e) => {
            eprint!("{}", out.warning_lines());
            let _ = command
                .error(clap::error::ErrorKind::MissingRequiredArgument, e)
                .print();
        }
        None => {
            let (stdout, stderr) = out.render(global.json);
            eprint!("{stderr}");
            print!("{stdout}");
        }
    }
    ExitCode::from(out.code as u8)
}

/// Runs the operation of the parsed command. The process stdin gives the text `owo write` and
/// `owo sync push` read when they take no `--content` or `--turns`.
fn run(ops: &[Operation], matches: &ArgMatches, global: &Global, env: &owo_core::Env) -> Output {
    let mut path = vec![];
    let mut m = matches;
    while let Some((name, sub)) = m.subcommand() {
        path.push(name);
        m = sub;
    }
    let op = ops
        .iter()
        .find(|op| op.path == path.as_slice())
        .expect("every command is an operation");
    let ran = Args::new(&op.name(), &op.fields, cli::args(op, m))
        .map(|args| cli::with_paths(op, m, args))
        .and_then(|args| op.run(&args, &global.select, env, &mut std::io::stdin()));
    match ran {
        Ok(ran) => render::output(ran, global.json, global.quiet),
        Err(e) => Output::from(anyhow::Error::from(e)),
    }
}

/// The environment values the library uses, read from the process.
fn env() -> owo_core::Env {
    let non_empty = |key: &str| std::env::var(key).ok().filter(|v| !v.is_empty());
    owo_core::Env {
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
fn mcp_stdio(global: &Global, env: &owo_core::Env) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        let (input, output) = own_stdio()?;
        Ok(mcp::serve(
            std::io::BufReader::new(input),
            output,
            &global.select,
            env,
        )?)
    }
    #[cfg(not(unix))]
    Ok(mcp::serve(
        std::io::stdin().lock(),
        std::io::stdout(),
        &global.select,
        env,
    )?)
}

/// Serves MCP over HTTP on `addr`, after it prints the URL.
fn mcp_http(
    addr: &str,
    port: Option<u16>,
    global: &Global,
    env: &owo_core::Env,
) -> anyhow::Result<()> {
    let listener = mcp::bind(addr, port)?;
    println!("http://{}/mcp", listener.local_addr()?);
    std::io::stdout().flush()?;
    #[cfg(unix)]
    detach_stdio()?;
    Ok(mcp::serve_http(listener, &global.select, env)?)
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
fn acp(global: &Global, env: &owo_core::Env, agent: &[String], inject: bool) -> anyhow::Result<u8> {
    use anyhow::Context;
    let proxy = owo_core::acp::Proxy::new(
        &global.select,
        env,
        owo_core::acp::Options { inject, mcp_server },
        std::io::stderr(),
    )?;
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

/// The command line of the owo MCP server the ACP proxy adds: this binary's `mcp`, with the
/// identity the proxy resolved at start.
fn mcp_server(identity: Option<&str>) -> (String, Vec<String>) {
    let mut args = vec!["mcp".to_string()];
    if let Some(id) = identity {
        args.extend(["-i".to_string(), id.to_string()]);
    }
    ("owo".to_string(), args)
}
