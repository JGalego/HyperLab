//! HyperLab as an MCP server, over stdin and stdout.
//!
//! ```text
//! hyperlab-mcp [--stack <path>] [--writable] [--only <tool>[,<tool>…]]
//! ```
//!
//! With no `--stack` it serves an empty one, which is enough for a client
//! that only wants to see what the tools are.
//!
//! Read-only unless `--writable` is given. That default is the point: this
//! program is started by other software, usually without a person watching,
//! and nobody is going to be asked. Changes are saved back to the file the
//! stack came from when the session ends, and only if it was writable.
//!
//! Everything it has to say goes to stderr, because stdout is the protocol.

use std::{path::PathBuf, process::ExitCode};

use hyperlab_mcp::{DenyAll, Policy, Server};
use hyperlab_persistence::{load, save};
use hyperlab_runtime::Runtime;
use hyperlab_stack::Stack;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(reason) => {
            eprintln!("hyperlab-mcp: {reason}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let options = Options::from_arguments(std::env::args().skip(1))?;

    let stack = match &options.stack {
        Some(path) => load(path).map_err(|error| format!("could not open {path:?}: {error}"))?,
        None => Stack::new("Untitled"),
    };
    let mut runtime = Runtime::new(stack);

    let mut policy = if options.writable {
        Policy::trusted()
    } else {
        Policy::new()
    };
    if let Some(tools) = &options.only {
        policy = policy.only(tools.iter().cloned());
    }

    let mut server = Server::new(policy);

    // Nobody is watching a program started by other software, so nobody can
    // consent to anything: whatever `--writable` allowed is all there is.
    hyperlab_mcp::serve_stdio(&mut server, &mut runtime, &mut DenyAll)
        .map_err(|error| format!("the connection failed: {error}"))?;

    if let (Some(path), true) = (&options.stack, options.writable) {
        save(path, &runtime.into_stack())
            .map_err(|error| format!("could not save {path:?}: {error}"))?;
    }
    Ok(())
}

/// What the command line asked for.
#[derive(Debug, Default)]
struct Options {
    stack: Option<PathBuf>,
    writable: bool,
    only: Option<Vec<String>>,
}

impl Options {
    fn from_arguments(arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut arguments = arguments.peekable();

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--stack" => {
                    let path = arguments.next().ok_or("--stack needs a path")?;
                    options.stack = Some(PathBuf::from(path));
                }
                "--writable" => options.writable = true,
                "--only" => {
                    let list = arguments
                        .next()
                        .ok_or("--only needs a list of tool names")?;
                    options.only = Some(
                        list.split(',')
                            .map(str::trim)
                            .filter(|name| !name.is_empty())
                            .map(String::from)
                            .collect(),
                    );
                }
                "--help" | "-h" => {
                    eprintln!("{USAGE}");
                    return Err("nothing to do".to_string());
                }
                other => return Err(format!("I do not understand \"{other}\"\n\n{USAGE}")),
            }
        }
        Ok(options)
    }
}

const USAGE: &str = "\
hyperlab-mcp — HyperLab's tools, over MCP on stdin and stdout

    --stack <path>        the .hl bundle to serve; a new one if omitted
    --writable            allow tools that change the stack, and save on exit
    --only <a,b,c>        allow only these tools
    --help                this
";
