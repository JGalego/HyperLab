//! Draws a stack, or tells you what is wrong with it.
//!
//! ```text
//! hyperlab-graph <stack.hl> [--dot | --json | --report]
//! ```
//!
//! The default is DOT on stdout, because the useful thing to do with it is
//! pipe it somewhere that can lay it out:
//!
//! ```sh
//! hyperlab-graph Myst.hl | sfdp -Tsvg -o myst.svg
//! ```
//!
//! `--report` is the same reading in prose, and exits non-zero if it found
//! anything — so it is worth running in CI over a stack you care about.

use std::{path::PathBuf, process::ExitCode};

use hyperlab_graph::{Destination, Graph, to_dot};
use hyperlab_persistence::load;
use hyperlab_stack::ObjectId;

fn main() -> ExitCode {
    match run() {
        Ok(clean) if clean => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(reason) => {
            eprintln!("hyperlab-graph: {reason}");
            ExitCode::FAILURE
        }
    }
}

/// Returns whether the stack came out clean, which only `--report` can fail.
fn run() -> Result<bool, String> {
    let (path, output) = arguments(std::env::args().skip(1))?;
    let stack = load(&path).map_err(|error| format!("could not open {path:?}: {error}"))?;
    let graph = Graph::of(&stack);

    match output {
        Output::Dot => {
            print!("{}", to_dot(&graph));
            Ok(true)
        }
        Output::Json => {
            let json = serde_json::to_string_pretty(&graph)
                .map_err(|error| format!("could not write the graph out: {error}"))?;
            println!("{json}");
            Ok(true)
        }
        Output::Report => Ok(report(&graph)),
    }
}

/// The findings, in the order they are worth acting on.
fn report(graph: &Graph) -> bool {
    let broken = graph.broken();
    let unreachable = graph.unreachable();
    let dead_ends = graph.dead_ends();

    println!(
        "{}: {}, {}",
        graph.stack,
        count(graph.nodes.len(), "card"),
        count(graph.edges.len(), "route")
    );

    // A link to a card that is not there is a plain bug; the other two are
    // worth looking at but can be exactly what the author meant.
    if !broken.is_empty() {
        println!("\nlinks to cards that are not there:");
        for edge in &broken {
            let Destination::Missing { wanted } = &edge.to else {
                continue;
            };
            let from = graph.node(edge.from).map_or("?", |node| node.name.as_str());
            println!("  {} → {wanted}", place(from, edge.via, edge.line));
        }
    }
    if !unreachable.is_empty() {
        println!("\nnothing leads to these from the first card:");
        for node in &unreachable {
            println!("  {}. {}", node.position, node.name);
        }
    }
    if !dead_ends.is_empty() {
        println!("\nno way out of these:");
        for node in &dead_ends {
            println!("  {}. {}", node.position, node.name);
        }
    }

    let unresolved = graph.unresolved();
    if unresolved > 0 {
        println!(
            "\n{} only running the stack would settle",
            count(unresolved, "route")
        );
    }

    let clean = broken.is_empty() && unreachable.is_empty() && dead_ends.is_empty();
    if clean {
        println!("\nevery card can be reached, and leads somewhere");
    }
    clean
}

fn count(many: usize, thing: &str) -> String {
    let plural = if many == 1 { "" } else { "s" };
    format!("{many} {thing}{plural}")
}

/// Which script said so, and where in it.
fn place(card: &str, via: ObjectId, line: u32) -> String {
    format!("{card} ({via}, line {line})")
}

/// What to print.
#[derive(Debug, Clone, Copy, Default)]
enum Output {
    #[default]
    Dot,
    Json,
    Report,
}

/// What the command line asked for.
fn arguments(given: impl Iterator<Item = String>) -> Result<(PathBuf, Output), String> {
    let mut path = None;
    let mut output = Output::default();

    for argument in given {
        match argument.as_str() {
            "--dot" => output = Output::Dot,
            "--json" => output = Output::Json,
            "--report" => output = Output::Report,
            "--help" | "-h" => {
                eprintln!("{USAGE}");
                return Err("nothing to do".to_string());
            }
            other if other.starts_with('-') => {
                return Err(format!("I do not understand \"{other}\"\n\n{USAGE}"));
            }
            other => path = Some(PathBuf::from(other)),
        }
    }

    let path = path.ok_or_else(|| format!("I need a stack to read\n\n{USAGE}"))?;
    Ok((path, output))
}

const USAGE: &str = "\
hyperlab-graph — which cards lead where

    hyperlab-graph <stack.hl> [--dot | --json | --report]

    --dot                 Graphviz, on stdout (the default)
    --json                the graph as data
    --report              what is wrong with it, in prose; exits 1 if anything is
    --help                this

    hyperlab-graph Myst.hl | sfdp -Tsvg -o myst.svg
";
