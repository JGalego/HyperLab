//! The graph as Graphviz sees it.
//!
//! Thirty lines that hand the hard part to somebody else. HyperLab has no
//! business shipping a force-directed layout engine to write a file with,
//! and `dot`, `neato` and `sfdp` are already on most machines — as are Gephi
//! and every other thing that reads DOT.
//!
//! ```sh
//! hyperlab-graph Myst.hl | sfdp -Tsvg -o myst.svg
//! ```

use std::fmt::Write as _;

use crate::graph::{Destination, Graph};

/// Renders the graph as a DOT digraph.
///
/// Cards are boxes, grouped into a subgraph per background so a layout engine
/// clusters them — which is what puts Myst Island in the middle and the Ages
/// around the outside. The rest is honesty about certainty:
///
/// | Drawn | Means |
/// | --- | --- |
/// | solid | a route that was read from the script |
/// | dashed | only running it would say where it goes |
/// | dotted, red | it names a card that is not there |
/// | double border | nothing leads here from the first card |
#[must_use]
pub fn to_dot(graph: &Graph) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "digraph {} {{", quoted(&graph.stack));
    out.push_str(
        "  graph [overlap=false, splines=true, bgcolor=\"white\"];\n\
         \x20 node  [shape=box, style=filled, fillcolor=\"white\", color=\"black\", \
         fontname=\"Helvetica\", fontsize=10];\n\
         \x20 edge  [color=\"black\", arrowsize=0.7];\n\n",
    );

    for (background, nodes) in graph.by_background() {
        let _ = writeln!(out, "  subgraph cluster_{background} {{");
        let _ = writeln!(out, "    label = \"background {background}\";");
        out.push_str("    color = \"grey60\";\n");
        for node in nodes {
            let mut attributes = vec![
                format!(
                    "label={}",
                    quoted(&format!("{}. {}", node.position, node.name))
                ),
                "shape=box".to_string(),
            ];
            if !node.reachable {
                attributes.push("peripheries=2".to_string());
            }
            if !node.leads_anywhere {
                attributes.push("color=\"grey40\"".to_string());
                attributes.push("fontcolor=\"grey40\"".to_string());
            }
            let _ = writeln!(out, "    n{} [{}];", node.id, attributes.join(", "));
        }
        out.push_str("  }\n\n");
    }

    // Anything a card can reach but that is not itself a card needs somewhere
    // to point, so each gets one node of its own rather than a shared blob:
    // two different broken links are two different bugs.
    let mut elsewhere = Vec::new();
    for (index, edge) in graph.edges.iter().enumerate() {
        let (target, style) = match &edge.to {
            Destination::Card { id } => (format!("n{id}"), "solid"),
            Destination::Back => {
                elsewhere.push((index, "go back".to_string(), "grey40"));
                (format!("x{index}"), "dashed")
            }
            Destination::Unresolved { because } => {
                elsewhere.push((index, because.clone(), "grey40"));
                (format!("x{index}"), "dashed")
            }
            Destination::Missing { wanted } => {
                elsewhere.push((index, wanted.clone(), "red"));
                (format!("x{index}"), "dotted")
            }
        };
        let colour = if matches!(edge.to, Destination::Missing { .. }) {
            ", color=\"red\""
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "  n{} -> {} [style={}{}, tooltip={}];",
            edge.from,
            target,
            style,
            colour,
            quoted(&format!("{} line {}", edge.via, edge.line))
        );
    }

    if !elsewhere.is_empty() {
        out.push('\n');
        for (index, label, colour) in elsewhere {
            let _ = writeln!(
                out,
                "  x{index} [label={}, shape=note, fillcolor=\"grey95\", color=\"{colour}\", fontsize=9];",
                quoted(&label)
            );
        }
    }

    out.push_str("}\n");
    out
}

/// A DOT string literal.
fn quoted(text: &str) -> String {
    let escaped: String = text
        .chars()
        .flat_map(|character| match character {
            '"' | '\\' => vec!['\\', character],
            '\n' => vec!['\\', 'n'],
            other => vec![other],
        })
        .collect();
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use hyperlab_stack::Stack;

    use super::*;

    #[test]
    fn a_stack_with_nothing_in_it_is_still_valid_dot() {
        let dot = to_dot(&Graph::of(&Stack::new("Empty")));
        assert!(dot.starts_with("digraph \"Empty\" {"));
        assert!(dot.trim_end().ends_with('}'));
    }

    #[test]
    fn a_quote_in_a_stack_name_does_not_break_the_file() {
        let dot = to_dot(&Graph::of(&Stack::new("He said \"hello\"")));
        assert!(
            dot.contains(r#"digraph "He said \"hello\"" {"#),
            "got {dot}"
        );
    }
}
