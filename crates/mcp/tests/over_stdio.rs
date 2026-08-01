//! The transport, end to end, against the real binary.
//!
//! [`Client`] drives `hyperlab-mcp` as a child process over a real pipe, so
//! the framing, the handshake and the permission layer are exercised as they
//! actually run rather than as two halves of one test agreeing with each
//! other. If these pass, an MCP client written by somebody else will work.

use std::path::PathBuf;

use hyperlab_mcp::{Client, Launch};
use serde_json::json;

/// Where Cargo put the binary under test.
///
/// The test executable lives in `target/<profile>/deps`, so the binary is two
/// directories up. Building it first is what `required-features` cannot
/// express, so the test says plainly what is missing if it is not there.
fn binary() -> PathBuf {
    let mut path = std::env::current_exe().expect("a test knows its own path");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.push(format!("hyperlab-mcp{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.exists(),
        "{path:?} is missing — run `cargo test` rather than `cargo test --lib`, \
         so that the binary is built first"
    );
    path
}

fn connect(arguments: &[&str]) -> Client {
    let launch = Launch::new(binary().to_string_lossy()).arguments(arguments.iter().copied());
    Client::start("hyperlab", &launch).expect("the server starts and completes the handshake")
}

#[test]
fn a_client_can_discover_what_hyperlab_offers() {
    let mut client = connect(&[]);
    let tools = client.tools().expect("the server lists its tools");

    let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_str()).collect();
    assert!(names.contains(&"current_card"), "got {names:?}");
    assert!(
        tools
            .iter()
            .all(|tool| tool.input_schema["type"] == "object"),
        "every tool publishes an object schema"
    );
    assert!(tools.iter().all(|tool| tool.server == "hyperlab"));
}

#[test]
fn a_read_only_server_offers_nothing_that_would_change_the_stack() {
    // The default, and the one that matters: this program is started by other
    // software, with nobody watching to be asked.
    let mut client = connect(&[]);
    let names: Vec<String> = client
        .tools()
        .expect("the server lists its tools")
        .into_iter()
        .map(|tool| tool.name)
        .collect();

    assert!(names.iter().any(|name| name == "read_field"));
    assert!(
        !names.iter().any(|name| name == "write_field"),
        "got {names:?}"
    );
    assert!(
        !names.iter().any(|name| name == "create_card"),
        "got {names:?}"
    );
}

#[test]
fn a_read_only_server_refuses_to_write_even_when_asked_directly() {
    // Not being offered a tool is not the same as being unable to call it.
    let mut client = connect(&[]);
    let answer = client
        .call_tool("create_card", &json!({}))
        .expect("a refusal is a tool result, not a broken connection");
    assert!(answer.contains("may only read"), "got {answer}");
}

#[test]
fn a_writable_server_will_change_the_stack() {
    let mut client = connect(&["--writable"]);

    client
        .call_tool("create_field", &json!({ "name": "Title" }))
        .expect("a field is created");
    client
        .call_tool("write_field", &json!({ "name": "Title", "text": "Hello" }))
        .expect("the field is written");

    let read = client
        .call_tool("read_field", &json!({ "name": "Title" }))
        .unwrap();
    assert!(read.contains("Hello"), "got {read}");
}

#[test]
fn only_narrows_a_server_to_the_tools_it_was_given() {
    let mut client = connect(&["--writable", "--only", "current_card,list_cards"]);

    let names: Vec<String> = client
        .tools()
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, vec!["current_card", "list_cards"]);

    let refused = client.call_tool("create_card", &json!({})).unwrap();
    assert!(refused.contains("may not use"), "got {refused}");
}

#[test]
fn a_tool_that_fails_reports_it_without_ending_the_session() {
    let mut client = connect(&["--writable"]);

    let missing = client
        .call_tool("read_field", &json!({ "name": "NoSuchField" }))
        .expect("a failing tool is still a reply");
    assert!(!missing.is_empty());

    // The session is still usable, which is the part worth asserting.
    let after = client
        .call_tool("list_cards", &json!({}))
        .expect("the session survived");
    assert!(after.contains("cards"), "got {after}");
}

#[test]
fn a_stack_can_be_served_from_disk_and_changes_are_saved_back() {
    let directory = std::env::temp_dir().join(format!(
        "hyperlab-mcp-test-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    let path = directory.join("Notes.hl");

    hyperlab_persistence::save(&path, &hyperlab_stack::Stack::new("Notes"))
        .expect("a fresh stack saves");

    {
        let mut client = {
            let launch = Launch::new(binary().to_string_lossy()).arguments([
                "--stack".to_string(),
                path.to_string_lossy().into_owned(),
                "--writable".to_string(),
            ]);
            Client::start("hyperlab", &launch).expect("the server starts")
        };
        client
            .call_tool("create_card", &json!({ "name": "Second" }))
            .expect("a card is created");
        // Dropping the client closes the pipe, which is how the server is
        // told the session is over and it is time to save.
    }

    // The child needs a moment to write and exit after its input closes.
    let saved = wait_for(|| {
        hyperlab_persistence::load(&path)
            .ok()
            .filter(|stack| stack.card_count() == 2)
    })
    .expect("the served stack was saved back with its new card");

    assert_eq!(saved.card_count(), 2);
    let _ = std::fs::remove_dir_all(&directory);
}

/// Retries `check` until it yields something, or a second has gone by.
fn wait_for<T>(mut check: impl FnMut() -> Option<T>) -> Option<T> {
    for _ in 0..100 {
        if let Some(value) = check() {
            return Some(value);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    None
}
