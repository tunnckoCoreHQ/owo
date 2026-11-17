mod cli;
mod output;
mod render;
mod text;

use clap::ArgMatches;
use cli::Global;
use output::Output;
use owo_core::{acp::Options, mcp, operations, Args, Operation};
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
            let options = Options {
                inject: sub.get_flag("inject"),
                mcp_server,
                dry_run: sub.get_flag("dry-run"),
                agent: sub.get_many("agent").unwrap().cloned().collect(),
            };
            match acp(&global, &env, options) {
                Ok(code) => return ExitCode::from(code),
                Err(e) => Output::from(e),
            }
        }
        _ => run(&ops, &matches, &global, &env),
    };
    match out.usage() {
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

fn env() -> owo_core::Env {
    let non_empty = |key: &str| std::env::var(key).ok().filter(|v| !v.is_empty());
    owo_core::Env {
        owo_root: non_empty("OWO_ROOT"),
        owo_identity: non_empty("OWO_IDENTITY"),
        owo_project: non_empty("OWO_PROJECT"),
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        home: std::env::var_os("HOME").map(PathBuf::from),
        xdg_data_home: non_empty("XDG_DATA_HOME").map(PathBuf::from),
    }
}

fn mcp_stdio(global: &Global, env: &owo_core::Env) -> anyhow::Result<()> {
    #[cfg(unix)]
    let (input, output) = own_stdio()?;
    #[cfg(not(unix))]
    let (input, output) = (std::io::stdin().lock(), std::io::stdout());
    Ok(mcp::serve(input, output, &global.select, env)?)
}

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
fn own_stdio() -> std::io::Result<(std::io::BufReader<std::fs::File>, std::fs::File)> {
    use std::os::fd::AsFd;
    let input = std::io::stdin().as_fd().try_clone_to_owned()?;
    let output = std::io::stdout().as_fd().try_clone_to_owned()?;
    detach_stdio()?;
    Ok((std::io::BufReader::new(input.into()), output.into()))
}

fn acp(global: &Global, env: &owo_core::Env, options: Options) -> anyhow::Result<u8> {
    use anyhow::Context;
    let agent = options.agent.clone();
    let proxy = owo_core::acp::Proxy::new(&global.select, env, options, std::io::stderr())?;
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

fn mcp_server(identity: Option<&str>) -> (String, Vec<String>) {
    let mut args = vec!["mcp".to_string()];
    if let Some(id) = identity {
        args.extend(["-i".to_string(), id.to_string()]);
    }
    let command = std::env::current_exe()
        .map_or_else(|_| "owo".to_string(), |p| p.to_string_lossy().into_owned());
    (command, args)
}
