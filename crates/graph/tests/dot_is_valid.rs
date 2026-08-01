//! The DOT we write is DOT that Graphviz reads.
//!
//! Worth a test of its own because the failure is silent from in here: a
//! missing `shape=` makes an attribute list that still looks fine in a string
//! assertion and still contains every label you checked for, and `dot` is the
//! only thing that says otherwise. It found exactly that bug once already.

use std::{
    io::Write as _,
    process::{Command as Process, Stdio},
};

use hyperlab_graph::{Graph, to_dot};
use hyperlab_runtime::{Command, PartOwner, Runtime};
use hyperlab_stack::{Object, ObjectId, ObjectKind, PartKind, Rect, Stack};

/// A stack that exercises every shape and every style the renderer can emit.
fn everything() -> Runtime {
    let mut runtime = Runtime::new(Stack::new("Every \"case\" at once"));
    for index in 0..3 {
        runtime
            .execute(Command::CreateCard {
                after: index,
                background: None,
            })
            .unwrap();
    }
    let ids: Vec<_> = runtime.stack().cards().iter().map(Object::id).collect();

    let script = |runtime: &mut Runtime, object, body: &str| {
        runtime
            .execute(Command::SetScript {
                object,
                script: format!("on mouseUp\n{body}\nend mouseUp"),
            })
            .unwrap();
    };

    // A solid edge, a dotted red one, a dashed one, and `go back`.
    script(
        &mut runtime,
        ObjectId::new(ObjectKind::Card, ids[0]),
        "go to next card",
    );
    script(
        &mut runtime,
        ObjectId::new(ObjectKind::Card, ids[1]),
        r#"go to card "Gone""#,
    );
    script(
        &mut runtime,
        ObjectId::new(ObjectKind::Card, ids[2]),
        "go to card whicheverOneTheyPicked",
    );
    script(
        &mut runtime,
        ObjectId::new(ObjectKind::Card, ids[3]),
        "go back",
    );

    // A second background, so there are two clusters, reached through a part
    // rather than a card script.
    let other = runtime
        .execute(Command::CreateBackground {
            name: "Other".into(),
        })
        .unwrap()
        .unwrap();
    runtime
        .stack_mut_unchecked()
        .card_mut(ids[3])
        .unwrap()
        .set_background(other.id);
    runtime
        .execute(Command::CreatePart {
            owner: PartOwner::Card { id: ids[0] },
            kind: PartKind::Button,
            name: "Quotes \"and\" a \\ backslash".into(),
            geometry: Rect::new(0, 0, 60, 20),
        })
        .unwrap();

    runtime
}

#[test]
fn graphviz_reads_what_we_write() {
    let graph = Graph::of(everything().stack());
    // The stack has to be interesting, or a valid-DOT check proves nothing.
    assert!(!graph.broken().is_empty() && !graph.unreachable().is_empty());
    assert!(graph.by_background().len() == 2 && graph.unresolved() >= 2);

    let dot = to_dot(&graph);
    let Ok(mut graphviz) = Process::new("dot")
        .args(["-Tsvg", "-o", "/dev/null"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    else {
        eprintln!("skipped: graphviz is not installed");
        return;
    };

    graphviz
        .stdin
        .take()
        .unwrap()
        .write_all(dot.as_bytes())
        .unwrap();
    let finished = graphviz.wait_with_output().unwrap();
    assert!(
        finished.status.success(),
        "dot rejected this:\n{dot}\n{}",
        String::from_utf8_lossy(&finished.stderr)
    );
}
