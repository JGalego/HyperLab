//! Tool calls, from the outside.

use hyperlab_mcp::{ToolError, ToolRegistry};
use hyperlab_runtime::Runtime;
use hyperlab_stack::{Object, Stack};
use serde_json::{Value as Json, json};

fn setup() -> (Runtime, ToolRegistry) {
    (Runtime::new(Stack::new("Test")), ToolRegistry::new())
}

fn call(runtime: &mut Runtime, name: &str, arguments: Json) -> Json {
    ToolRegistry::new()
        .call(runtime, name, &arguments)
        .unwrap_or_else(|error| panic!("{name} failed: {error}"))
}

#[test]
fn the_current_card_is_described_well_enough_to_act_on() {
    let (mut runtime, _) = setup();
    call(&mut runtime, "create_field", json!({ "name": "Title" }));
    call(&mut runtime, "create_button", json!({ "name": "Go" }));

    let described = call(&mut runtime, "current_card", json!({}));
    assert_eq!(described["card"]["position"], 1);
    assert_eq!(described["card"]["parts"].as_array().unwrap().len(), 2);
    assert_eq!(described["background"]["name"], "Background 1");
}

#[test]
fn fields_can_be_written_and_read_back() {
    let (mut runtime, _) = setup();
    call(&mut runtime, "create_field", json!({ "name": "Title" }));
    call(
        &mut runtime,
        "write_field",
        json!({ "name": "Title", "text": "Hello" }),
    );
    let read = call(&mut runtime, "read_field", json!({ "name": "Title" }));
    assert_eq!(read["text"], "Hello");
}

#[test]
fn a_field_can_also_be_found_by_id() {
    let (mut runtime, _) = setup();
    let created = call(&mut runtime, "create_field", json!({ "name": "Title" }));
    let id = created["id"].clone();
    call(
        &mut runtime,
        "write_field",
        json!({ "id": id, "text": "by id" }),
    );
    assert_eq!(
        call(&mut runtime, "read_field", json!({ "id": id }))["text"],
        "by id"
    );
}

#[test]
fn a_missing_field_says_how_to_find_out_what_exists() {
    let (mut runtime, registry) = setup();
    let error = registry
        .call(&mut runtime, "read_field", &json!({ "name": "Nowhere" }))
        .unwrap_err();
    assert!(
        error.to_string().contains("current_card"),
        "an error a model can act on: {error}"
    );
}

#[test]
fn everything_a_tool_does_can_be_undone() {
    let (mut runtime, _) = setup();
    call(&mut runtime, "create_card", json!({ "name": "Second" }));
    assert_eq!(runtime.stack().card_count(), 2);

    let undone = call(&mut runtime, "undo", json!({}));
    assert_eq!(undone["undone"], true);
    assert_eq!(
        runtime.stack().card_count(),
        2,
        "the rename was undone first"
    );

    call(&mut runtime, "undo", json!({}));
    assert_eq!(runtime.stack().card_count(), 1);
}

#[test]
fn a_button_can_be_created_with_a_script_and_then_clicked() {
    let (mut runtime, _) = setup();
    call(&mut runtime, "create_field", json!({ "name": "Out" }));
    let button = call(
        &mut runtime,
        "create_button",
        json!({
            "name": "Go",
            "script": "on mouseUp\n  put \"clicked\" into field \"Out\"\nend mouseUp"
        }),
    );

    call(
        &mut runtime,
        "send_message",
        json!({ "message": "mouseUp", "id": button["id"], "kind": "button" }),
    );
    let read = call(&mut runtime, "read_field", json!({ "name": "Out" }));
    assert_eq!(read["text"], "clicked");
}

#[test]
fn a_script_that_does_not_parse_takes_the_whole_call_down_with_it() {
    let (mut runtime, registry) = setup();
    let before = count_parts(&runtime);

    let error = registry
        .call(
            &mut runtime,
            "create_button",
            &json!({ "name": "Broken", "script": "on mouseUp\n  repeat" }),
        )
        .unwrap_err();

    assert!(matches!(error, ToolError::Runtime(_)), "{error}");
    // A call that said no must not have left half of itself behind. A model
    // told "no" will try again, and two nameless buttons is how that ends.
    assert_eq!(
        count_parts(&runtime),
        before,
        "the button was created anyway"
    );
    assert!(error.to_string().contains("was not created"), "{error}");
}

/// How many parts the current card has.
fn count_parts(runtime: &Runtime) -> usize {
    use hyperlab_stack::PartContainer;
    runtime
        .stack()
        .card(runtime.current_card())
        .map_or(0, |card| card.parts().len())
}

#[test]
fn properties_can_be_set_on_any_object() {
    let (mut runtime, _) = setup();
    let button = call(&mut runtime, "create_button", json!({ "name": "Go" }));
    call(
        &mut runtime,
        "set_property",
        json!({ "id": button["id"], "kind": "button", "property": "visible", "value": false }),
    );

    let described = call(&mut runtime, "current_card", json!({}));
    let _ = described;
    let id = hyperlab_stack::Id::new(button["id"].as_u64().unwrap());
    assert_eq!(
        runtime.stack().part(id).unwrap().property("visible"),
        Some(hyperlab_stack::Value::Bool(false))
    );
}

#[test]
fn a_fragment_of_hypertalk_can_be_run_directly() {
    let (mut runtime, _) = setup();
    call(&mut runtime, "create_field", json!({ "name": "Out" }));
    let result = call(
        &mut runtime,
        "run_script",
        json!({ "script": "put 6 * 7 into field \"Out\"\nanswer \"done\"" }),
    );
    assert_eq!(
        call(&mut runtime, "read_field", json!({ "name": "Out" }))["text"],
        "42"
    );
    assert_eq!(result["effects"][0]["kind"], "answer");
}

#[test]
fn cards_can_be_searched_and_visited() {
    let (mut runtime, _) = setup();
    call(&mut runtime, "create_field", json!({ "name": "Body" }));
    call(
        &mut runtime,
        "write_field",
        json!({ "name": "Body", "text": "chocolate cake" }),
    );
    call(&mut runtime, "create_card", json!({ "name": "Second" }));

    let found = call(&mut runtime, "find_cards", json!({ "text": "CHOCOLATE" }));
    let matches = found["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["position"], 1);
    assert_eq!(matches[0]["found_in"][0], "field \"Body\"");

    let moved = call(&mut runtime, "go_to_card", json!({ "position": 2 }));
    assert_eq!(moved["position"], 2);
}

#[test]
fn tools_report_missing_arguments_plainly() {
    let (mut runtime, registry) = setup();
    let error = registry
        .call(&mut runtime, "write_field", &json!({ "name": "x" }))
        .unwrap_err();
    assert!(error.to_string().contains("text"), "{error}");
}

#[test]
fn the_tool_list_is_stable_and_documented() {
    let names: Vec<&str> = ToolRegistry::new().names().collect();
    for expected in [
        "current_card",
        "read_field",
        "write_field",
        "create_card",
        "create_button",
        "run_script",
        "find_cards",
    ] {
        assert!(names.contains(&expected), "{expected} is missing");
    }
}
