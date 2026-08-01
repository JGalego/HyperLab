//! End-to-end tests: HyperTalk in, changed stack out.

use hyperlab_runtime::{Command, Effect, Host, Message, PartOwner, Runtime, RuntimeResult};
use hyperlab_stack::{Id, Object, ObjectId, ObjectKind, PartKind, Rect, Stack, Value};

/// A stack with one button and two fields on its only card, plus a second
/// card, which is enough to exercise almost everything.
struct Fixture {
    runtime: Runtime,
    button: ObjectId,
    name_field: ObjectId,
    notes_field: ObjectId,
    first_card: Id,
    second_card: Id,
}

impl Fixture {
    fn new() -> Self {
        Self::with_host(Box::new(hyperlab_runtime::SilentHost))
    }

    fn with_host(host: Box<dyn Host>) -> Self {
        let mut runtime = Runtime::with_host(Stack::new("Test"), host);
        let first_card = runtime.current_card();
        let owner = PartOwner::Card { id: first_card };

        let button = runtime
            .execute(Command::CreatePart {
                owner,
                kind: PartKind::Button,
                name: "Go".into(),
                geometry: Rect::new(10, 10, 80, 20),
            })
            .unwrap()
            .unwrap();
        let name_field = runtime
            .execute(Command::CreatePart {
                owner,
                kind: PartKind::Field,
                name: "Name".into(),
                geometry: Rect::new(10, 40, 200, 20),
            })
            .unwrap()
            .unwrap();
        let notes_field = runtime
            .execute(Command::CreatePart {
                owner,
                kind: PartKind::Field,
                name: "Notes".into(),
                geometry: Rect::new(10, 70, 200, 60),
            })
            .unwrap()
            .unwrap();

        runtime
            .execute(Command::CreateCard {
                after: 0,
                background: None,
            })
            .unwrap();
        let second_card = runtime.stack().cards()[1].id();
        runtime
            .execute(Command::Rename {
                object: ObjectId::new(ObjectKind::Card, second_card),
                name: "Second".into(),
            })
            .unwrap();

        runtime.take_effects();
        Self {
            runtime,
            button,
            name_field,
            notes_field,
            first_card,
            second_card,
        }
    }

    /// Puts `body` in the button's `mouseUp` handler and clicks it.
    fn click(&mut self, body: &str) -> RuntimeResult<Value> {
        self.script(self.button, &format!("on mouseUp\n{body}\nend mouseUp"));
        self.runtime.take_effects();
        self.runtime
            .send_message(&Message::new("mouseUp"), self.button)
    }

    /// Clicks and expects success.
    fn run(&mut self, body: &str) {
        if let Err(error) = self.click(body) {
            panic!("script failed:\n{body}\n\n{error}");
        }
    }

    fn script(&mut self, object: ObjectId, source: &str) {
        self.runtime
            .execute(Command::SetScript {
                object,
                script: source.to_string(),
            })
            .unwrap();
    }

    fn text_of(&self, field: ObjectId) -> String {
        self.runtime
            .object(field)
            .unwrap()
            .property("text")
            .unwrap_or(Value::Empty)
            .as_text()
    }

    fn set_text(&mut self, field: ObjectId, text: &str) {
        self.runtime
            .execute(Command::SetProperty {
                object: field,
                property: "text".into(),
                value: Some(Value::text(text)),
            })
            .unwrap();
    }

    fn effects(&mut self) -> Vec<Effect> {
        self.runtime.take_effects()
    }

    fn answers(&mut self) -> Vec<String> {
        self.effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Answer { message } => Some(message),
                _ => None,
            })
            .collect()
    }
}

// -------------------------------------------------------- containers & flow

#[test]
fn a_script_can_write_to_a_field_and_read_it_back() {
    let mut fixture = Fixture::new();
    fixture.run(r#"put "Ada" into field "Name""#);
    assert_eq!(fixture.text_of(fixture.name_field), "Ada");

    fixture.run(r#"put field "Name" & " Lovelace" into field "Name""#);
    assert_eq!(fixture.text_of(fixture.name_field), "Ada Lovelace");
}

#[test]
fn before_and_after_leave_the_rest_of_the_text_alone() {
    let mut fixture = Fixture::new();
    fixture.set_text(fixture.name_field, "b");
    fixture.run(r#"put "a" before field "Name""#);
    fixture.run(r#"put "c" after field "Name""#);
    assert_eq!(fixture.text_of(fixture.name_field), "abc");
}

#[test]
fn variables_are_local_to_a_handler_and_it_is_set_by_get() {
    let mut fixture = Fixture::new();
    fixture.run(
        r#"put 1 into total
           add 2 to total
           get total * 10
           put it into field "Name""#,
    );
    assert_eq!(fixture.text_of(fixture.name_field), "30");
}

#[test]
fn an_unset_variable_stands_for_its_own_name() {
    let mut fixture = Fixture::new();
    fixture.run(r#"put somethingNeverSet into field "Name""#);
    assert_eq!(fixture.text_of(fixture.name_field), "somethingNeverSet");
}

#[test]
fn globals_outlive_the_handler_that_set_them() {
    let mut fixture = Fixture::new();
    fixture.run("global counter\nput 41 into counter");
    fixture.run("global counter\nadd 1 to counter");
    assert_eq!(
        fixture.runtime.global("counter"),
        Some(&Value::Number(42.0))
    );
}

#[test]
fn a_global_is_invisible_to_a_handler_that_did_not_declare_it() {
    let mut fixture = Fixture::new();
    fixture.run("global shared\nput \"yes\" into shared");
    fixture.run(r#"put shared into field "Name""#);
    assert_eq!(
        fixture.text_of(fixture.name_field),
        "shared",
        "without `global`, the name is just a word"
    );
}

#[test]
fn conditionals_choose_one_branch() {
    let mut fixture = Fixture::new();
    fixture.run(
        r#"put 5 into n
           if n > 10 then
             put "big" into field "Name"
           else if n > 3 then
             put "middling" into field "Name"
           else
             put "small" into field "Name"
           end if"#,
    );
    assert_eq!(fixture.text_of(fixture.name_field), "middling");
}

#[test]
fn every_repeat_form_counts_correctly() {
    let cases = [
        ("repeat 3 times\nadd 1 to n\nend repeat", "3"),
        ("repeat with i = 1 to 4\nadd i to n\nend repeat", "10"),
        ("repeat with i = 3 down to 1\nadd 1 to n\nend repeat", "3"),
        ("repeat while n < 5\nadd 1 to n\nend repeat", "5"),
        ("repeat until n = 2\nadd 1 to n\nend repeat", "2"),
        (
            "repeat\nadd 1 to n\nif n = 7 then exit repeat\nend repeat",
            "7",
        ),
    ];
    for (loop_source, expected) in cases {
        let mut fixture = Fixture::new();
        fixture.run(&format!(
            "put 0 into n\n{loop_source}\nput n into field \"Name\""
        ));
        assert_eq!(
            fixture.text_of(fixture.name_field),
            expected,
            "from:\n{loop_source}"
        );
    }
}

#[test]
fn next_repeat_skips_the_rest_of_the_turn() {
    let mut fixture = Fixture::new();
    fixture.run(
        r#"put 0 into total
           repeat with i = 1 to 5
             if i = 3 then next repeat
             add i to total
           end repeat
           put total into field "Name""#,
    );
    assert_eq!(fixture.text_of(fixture.name_field), "12");
}

#[test]
fn a_runaway_loop_is_stopped_rather_than_hanging() {
    let mut fixture = Fixture::new();
    let error = fixture
        .click("repeat\n  put 1 into n\nend repeat")
        .unwrap_err();
    assert!(error.message.contains("never going to stop"), "{error}");
}

// ------------------------------------------------------------------- chunks

#[test]
fn chunks_read_and_write_parts_of_a_field() {
    let mut fixture = Fixture::new();
    fixture.set_text(fixture.notes_field, "one two three");
    fixture.run(r#"put word 2 of field "Notes" into field "Name""#);
    assert_eq!(fixture.text_of(fixture.name_field), "two");

    fixture.run(r#"put "TWO" into word 2 of field "Notes""#);
    assert_eq!(fixture.text_of(fixture.notes_field), "one TWO three");
}

#[test]
fn nested_chunks_address_a_word_inside_a_line() {
    let mut fixture = Fixture::new();
    fixture.set_text(fixture.notes_field, "a b\nc d");
    fixture.run(r#"put "D" into word 2 of line 2 of field "Notes""#);
    assert_eq!(fixture.text_of(fixture.notes_field), "a b\nc D");
}

#[test]
fn writing_past_the_end_of_a_list_extends_it() {
    let mut fixture = Fixture::new();
    fixture.set_text(fixture.notes_field, "a,b");
    fixture.run(r#"put "c" into item 3 of field "Notes""#);
    assert_eq!(fixture.text_of(fixture.notes_field), "a,b,c");
}

// -------------------------------------------------------- objects & message

#[test]
fn me_is_the_object_whose_script_is_running() {
    let mut fixture = Fixture::new();
    fixture.run(r#"put the name of me into field "Name""#);
    assert_eq!(fixture.text_of(fixture.name_field), "Go");
}

#[test]
fn a_message_travels_out_to_the_card_when_the_button_ignores_it() {
    let mut fixture = Fixture::new();
    let card = ObjectId::new(ObjectKind::Card, fixture.first_card);
    fixture.script(
        card,
        r#"on mouseUp
             put "the card handled it" into field "Name"
           end mouseUp"#,
    );
    fixture.runtime.take_effects();
    fixture
        .runtime
        .send_message(&Message::new("mouseUp"), fixture.button)
        .unwrap();
    assert_eq!(fixture.text_of(fixture.name_field), "the card handled it");
}

#[test]
fn pass_hands_the_message_on_after_doing_something() {
    let mut fixture = Fixture::new();
    let card = ObjectId::new(ObjectKind::Card, fixture.first_card);
    fixture.script(
        card,
        r#"on mouseUp
             put field "Notes" & "card " into field "Notes"
           end mouseUp"#,
    );
    fixture.run(
        r#"put "button " into field "Notes"
           pass mouseUp"#,
    );
    assert_eq!(fixture.text_of(fixture.notes_field), "button card ");
}

#[test]
fn the_target_stays_the_object_that_was_clicked() {
    let mut fixture = Fixture::new();
    let card = ObjectId::new(ObjectKind::Card, fixture.first_card);
    fixture.script(
        card,
        r#"on mouseUp
             put the name of the target & "/" & the name of me into field "Name"
           end mouseUp"#,
    );
    fixture.runtime.take_effects();
    fixture
        .runtime
        .send_message(&Message::new("mouseUp"), fixture.button)
        .unwrap();
    assert_eq!(fixture.text_of(fixture.name_field), "Go/Card 1");
}

#[test]
fn send_delivers_a_message_to_another_object() {
    let mut fixture = Fixture::new();
    let card = ObjectId::new(ObjectKind::Card, fixture.first_card);
    fixture.script(
        card,
        r#"on shout
             put "heard" into field "Name"
           end shout"#,
    );
    fixture.run(r#"send "shout" to this card"#);
    assert_eq!(fixture.text_of(fixture.name_field), "heard");
}

#[test]
fn a_handler_elsewhere_in_the_path_can_be_called_like_a_command() {
    let mut fixture = Fixture::new();
    let stack = ObjectId::new(ObjectKind::Stack, fixture.runtime.stack().id());
    fixture.script(
        stack,
        r#"on greet who
             put "Hello, " & who into field "Name"
           end greet"#,
    );
    fixture.run(r#"greet "world""#);
    assert_eq!(fixture.text_of(fixture.name_field), "Hello, world");
}

#[test]
fn functions_defined_in_the_path_can_be_called() {
    let mut fixture = Fixture::new();
    let stack = ObjectId::new(ObjectKind::Stack, fixture.runtime.stack().id());
    fixture.script(stack, "function double n\n  return n * 2\nend double");
    fixture.run(r#"put double(21) into field "Name""#);
    assert_eq!(fixture.text_of(fixture.name_field), "42");
}

#[test]
fn a_user_handler_wins_over_a_builtin_of_the_same_name() {
    let mut fixture = Fixture::new();
    let stack = ObjectId::new(ObjectKind::Stack, fixture.runtime.stack().id());
    fixture.script(
        stack,
        r#"on beep
             put "quiet please" into field "Name"
           end beep"#,
    );
    fixture.run("beep");
    assert_eq!(fixture.text_of(fixture.name_field), "quiet please");
    assert!(
        !fixture.effects().contains(&Effect::Beep),
        "the built-in must not also run"
    );
}

// --------------------------------------------------------------- properties

#[test]
fn properties_can_be_read_and_written() {
    let mut fixture = Fixture::new();
    fixture.run(r#"set the width of button "Go" to 120"#);
    assert_eq!(
        fixture
            .runtime
            .object(fixture.button)
            .unwrap()
            .property("width"),
        Some(Value::Number(120.0))
    );

    fixture.run(r#"put the height of button "Go" into field "Name""#);
    assert_eq!(fixture.text_of(fixture.name_field), "20");
}

#[test]
fn hide_and_show_change_visibility() {
    let mut fixture = Fixture::new();
    fixture.run(r#"hide field "Notes""#);
    assert_eq!(
        fixture
            .runtime
            .object(fixture.notes_field)
            .unwrap()
            .property("visible"),
        Some(Value::Bool(false))
    );
    fixture.run(r#"show field "Notes""#);
    assert_eq!(
        fixture
            .runtime
            .object(fixture.notes_field)
            .unwrap()
            .property("visible"),
        Some(Value::Bool(true))
    );
}

#[test]
fn scripted_changes_can_be_undone_like_any_other() {
    let mut fixture = Fixture::new();
    fixture.set_text(fixture.name_field, "before");
    fixture.run(r#"put "after" into field "Name""#);
    assert_eq!(fixture.text_of(fixture.name_field), "after");

    assert!(fixture.runtime.undo().unwrap());
    assert_eq!(
        fixture.text_of(fixture.name_field),
        "before",
        "a script's edit belongs in the undo history"
    );
}

#[test]
fn counting_knows_about_cards_and_parts() {
    let mut fixture = Fixture::new();
    fixture.run(r#"put the number of cards & "/" & the number of fields into field "Name""#);
    assert_eq!(fixture.text_of(fixture.name_field), "2/2");
}

#[test]
fn existence_can_be_tested_before_use() {
    let mut fixture = Fixture::new();
    fixture.run(
        r#"if there is a field "Name" then
             put "yes" into field "Name"
           end if
           if there is no field "Missing" then
             put field "Name" & " and no" into field "Name"
           end if"#,
    );
    assert_eq!(fixture.text_of(fixture.name_field), "yes and no");
}

// --------------------------------------------------------------- navigation

#[test]
fn go_moves_between_cards() {
    let mut fixture = Fixture::new();
    fixture.run("go to next card");
    assert_eq!(fixture.runtime.current_card(), fixture.second_card);

    fixture.runtime.go_to_card(fixture.first_card).unwrap();
    fixture.run(r#"go to card "Second""#);
    assert_eq!(fixture.runtime.current_card(), fixture.second_card);
}

#[test]
fn navigation_sends_close_and_open_card() {
    let mut fixture = Fixture::new();
    let first = ObjectId::new(ObjectKind::Card, fixture.first_card);
    let second = ObjectId::new(ObjectKind::Card, fixture.second_card);
    fixture.script(
        first,
        "on closeCard\n  global trace\n  put trace & \"close \" into trace\nend closeCard",
    );
    fixture.script(
        second,
        "on openCard\n  global trace\n  put trace & \"open\" into trace\nend openCard",
    );
    fixture.runtime.set_global("trace", Value::text(""));

    fixture.runtime.go_to_card(fixture.second_card).unwrap();
    assert_eq!(
        fixture.runtime.global("trace"),
        Some(&Value::text("close open"))
    );
}

#[test]
fn go_back_returns_to_the_previous_card() {
    let mut fixture = Fixture::new();
    fixture.run("go to next card");
    fixture.run("go back");
    assert_eq!(fixture.runtime.current_card(), fixture.first_card);
}

// -------------------------------------------------------------- the outside

#[test]
fn answer_becomes_an_effect_the_shell_can_show() {
    let mut fixture = Fixture::new();
    fixture.run(r#"answer "Hello" "#);
    assert_eq!(fixture.answers(), vec!["Hello".to_string()]);
}

#[test]
fn ask_puts_the_hosts_answer_into_it() {
    struct Typist;
    impl Host for Typist {
        fn ask(&mut self, _prompt: &str, _default: &str) -> Option<String> {
            Some("Grace".into())
        }
    }

    let mut fixture = Fixture::with_host(Box::new(Typist));
    fixture.run(
        r#"ask "Name?" with "Ada"
                   put it into field "Name""#,
    );
    assert_eq!(fixture.text_of(fixture.name_field), "Grace");
}

#[test]
fn a_cancelled_question_says_so_in_the_result() {
    let mut fixture = Fixture::new();
    fixture.run(
        r#"ask "Name?"
           put the result into field "Name""#,
    );
    assert_eq!(fixture.text_of(fixture.name_field), "Cancel");
}

#[test]
fn wait_is_reported_rather_than_blocking_the_runtime() {
    let mut fixture = Fixture::new();
    fixture.run("wait 2 seconds");
    assert_eq!(fixture.effects(), vec![Effect::Wait { ticks: 120.0 }]);
}

// ------------------------------------------------------------------- errors

#[test]
fn an_unknown_command_says_what_it_did_not_understand() {
    let mut fixture = Fixture::new();
    let error = fixture.click("flumox 1").unwrap_err();
    assert!(error.message.contains("flumox"), "{error}");
}

#[test]
fn a_missing_field_is_reported_by_name() {
    let mut fixture = Fixture::new();
    let error = fixture
        .click(r#"put "x" into field "Nowhere""#)
        .unwrap_err();
    assert!(error.message.contains("Nowhere"), "{error}");
}

#[test]
fn errors_report_the_line_and_the_object() {
    let mut fixture = Fixture::new();
    let error = fixture.click("put 1 into x\nput 1 / 0 into y").unwrap_err();
    assert_eq!(error.line, Some(3), "line 1 is the `on mouseUp`");
    assert!(error.message.contains("divide by zero"), "{error}");
    assert!(error.message.contains("button \"Go\""), "{error}");
}

#[test]
fn a_script_that_does_not_parse_fails_before_it_runs() {
    let mut fixture = Fixture::new();
    fixture.set_text(fixture.name_field, "untouched");
    let error = fixture
        .click("put \"x\" into field \"Name\"\nrepeat")
        .unwrap_err();
    assert!(error.line.is_some(), "{error}");
    assert_eq!(
        fixture.text_of(fixture.name_field),
        "untouched",
        "nothing runs until the whole script parses"
    );
}

#[test]
fn exit_must_name_the_handler_it_is_leaving() {
    let mut fixture = Fixture::new();
    let error = fixture.click("exit somethingElse").unwrap_err();
    assert!(error.message.contains("mouseUp"), "{error}");
}

#[test]
fn an_unhandled_message_is_not_an_error() {
    let mut fixture = Fixture::new();
    let value = fixture
        .runtime
        .send_message(&Message::new("nobodyHandlesThis"), fixture.button)
        .unwrap();
    assert_eq!(value, Value::Empty);
}

#[test]
fn a_host_sees_dialogs_in_the_order_the_script_showed_them() {
    use std::sync::{Arc, Mutex};

    /// A host that writes down what it was asked, and answers.
    struct Recorder {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl Host for Recorder {
        fn answer(&mut self, message: &str) {
            self.seen.lock().unwrap().push(format!("answer: {message}"));
        }

        fn ask(&mut self, prompt: &str, default: &str) -> Option<String> {
            self.seen.lock().unwrap().push(format!("ask: {prompt}"));
            Some(default.to_string())
        }
    }

    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut fixture = Fixture::with_host(Box::new(Recorder {
        seen: Arc::clone(&seen),
    }));

    fixture.run(
        r#"answer "first"
           ask "second" with "typed"
           put it into field "Name"
           answer "third""#,
    );

    // The host is called as each statement runs, not replayed once the
    // handler has finished. That is what lets a dialog block until it is
    // answered, and what lets the answer reach the very next line.
    assert_eq!(
        *seen.lock().unwrap(),
        vec!["answer: first", "ask: second", "answer: third"]
    );
    assert_eq!(fixture.text_of(fixture.name_field), "typed");
}
