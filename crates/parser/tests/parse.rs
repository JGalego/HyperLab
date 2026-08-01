//! End-to-end parser tests: source in, AST out.

use hyperlab_parser::{
    ast::{
        ArithmeticCommand, BinaryOp, ChunkKind, Container, ContainerBase, CountTarget, Destination,
        ExitTarget, Expr, HandlerKind, Layer, ObjectRef, Ordinal, PartKind, Preposition,
        RepeatControl, Specifier, StatementKind, UnaryOp,
    },
    parse, parse_expression,
};

/// Parses a script whose single handler's statements are returned.
fn body(source: &str) -> Vec<StatementKind> {
    let script = parse(&format!("on test\n{source}\nend test")).unwrap_or_else(|error| {
        panic!("failed to parse:\n{source}\n{error}");
    });
    script.handlers[0]
        .body
        .iter()
        .map(|statement| statement.kind.clone())
        .collect()
}

/// Parses a script whose single handler has exactly one statement.
fn statement(source: &str) -> StatementKind {
    let mut statements = body(source);
    assert_eq!(statements.len(), 1, "expected one statement from: {source}");
    statements.remove(0)
}

fn expression(source: &str) -> Expr {
    parse_expression(source).unwrap_or_else(|error| panic!("failed to parse {source}: {error}"))
}

// ------------------------------------------------------------------ handlers

#[test]
fn a_script_is_a_list_of_handlers() {
    let script =
        parse("on mouseUp\n  beep\nend mouseUp\n\nfunction double n\n  return n * 2\nend double\n")
            .unwrap();
    assert_eq!(script.handlers.len(), 2);
    assert_eq!(script.handlers[0].kind, HandlerKind::Message);
    assert_eq!(script.handlers[1].kind, HandlerKind::Function);
    assert_eq!(script.handlers[1].parameters, vec!["n"]);
}

#[test]
fn handler_names_are_recorded_as_written_but_matched_loosely() {
    let script = parse("on MouseUp\nend mouseup\n").unwrap();
    assert_eq!(script.handlers[0].name, "MouseUp");
    assert!(script.handler(HandlerKind::Message, "mouseUp").is_some());
}

#[test]
fn handlers_report_their_line() {
    let script = parse("\n\non mouseUp\nend mouseUp\n").unwrap();
    assert_eq!(script.handlers[0].line, 3);
}

#[test]
fn multiple_parameters_are_separated_by_commas() {
    let script = parse("on greet first, last\nend greet\n").unwrap();
    assert_eq!(script.handlers[0].parameters, vec!["first", "last"]);
}

#[test]
fn statements_outside_a_handler_are_rejected() {
    let error = parse("beep\n").unwrap_err();
    assert!(error.message.contains("only handlers"), "{}", error.message);
}

#[test]
fn a_mismatched_end_is_reported() {
    let error = parse("on mouseUp\nend mouseDown\n").unwrap_err();
    assert!(error.message.contains("ends with"), "{}", error.message);
}

#[test]
fn statements_remember_their_line() {
    let script = parse("on test\n  beep\n  beep\nend test").unwrap();
    let lines: Vec<u32> = script.handlers[0]
        .body
        .iter()
        .map(|statement| statement.line)
        .collect();
    assert_eq!(lines, vec![2, 3]);
}

// ---------------------------------------------------------------- statements

#[test]
fn put_defaults_to_the_message_box() {
    assert_eq!(
        statement(r#"put "hi""#),
        StatementKind::Put {
            value: Expr::Text("hi".into()),
            target: None,
            preposition: Preposition::Into,
        }
    );
}

#[test]
fn put_into_a_variable() {
    let StatementKind::Put {
        target: Some(container),
        preposition,
        ..
    } = statement("put 1 into total")
    else {
        panic!("expected a put statement");
    };
    assert_eq!(preposition, Preposition::Into);
    assert_eq!(container.base, ContainerBase::Variable("total".into()));
    assert!(container.chunks.is_empty());
}

#[test]
fn put_before_and_after_are_distinguished() {
    let kinds: Vec<Preposition> = ["put 1 before x", "put 1 after x"]
        .iter()
        .map(|source| match statement(source) {
            StatementKind::Put { preposition, .. } => preposition,
            _ => panic!("expected a put statement"),
        })
        .collect();
    assert_eq!(kinds, vec![Preposition::Before, Preposition::After]);
}

#[test]
fn put_into_a_field_targets_an_object() {
    let StatementKind::Put {
        target:
            Some(Container {
                base: ContainerBase::Object(ObjectRef::Part { kind, layer, .. }),
                ..
            }),
        ..
    } = statement(r#"put "x" into field "Notes""#)
    else {
        panic!("expected a put into a field");
    };
    assert_eq!(kind, PartKind::Field);
    assert_eq!(layer, Layer::Unspecified);
}

#[test]
fn put_into_a_chunk_of_a_container() {
    let StatementKind::Put {
        target: Some(container),
        ..
    } = statement(r#"put "x" into word 2 of line 3 of field "Notes""#)
    else {
        panic!("expected a put statement");
    };
    assert_eq!(container.chunks.len(), 2, "chunks are kept outermost first");
    assert_eq!(container.chunks[0].kind, ChunkKind::Word);
    assert_eq!(container.chunks[1].kind, ChunkKind::Line);
}

#[test]
fn set_reads_a_property_and_an_optional_object() {
    let StatementKind::Set {
        property, object, ..
    } = statement(r#"set the visible of button "Go" to false"#)
    else {
        panic!("expected a set statement");
    };
    assert_eq!(property, "visible");
    assert!(matches!(object, Some(ObjectRef::Part { .. })));
}

#[test]
fn set_without_an_object_leaves_it_to_the_runtime() {
    let StatementKind::Set { object, .. } = statement("set the name to \"x\"") else {
        panic!("expected a set statement");
    };
    assert!(object.is_none());
}

#[test]
fn get_is_shorthand_for_putting_into_it() {
    assert_eq!(statement("get 1 + 1"), {
        StatementKind::Get(Expr::Binary {
            operator: BinaryOp::Add,
            left: Box::new(Expr::Number(1.0)),
            right: Box::new(Expr::Number(1.0)),
        })
    });
}

#[test]
fn the_four_arithmetic_commands_agree_on_their_operands() {
    let cases = [
        ("add 1 to total", ArithmeticCommand::Add),
        ("subtract 1 from total", ArithmeticCommand::Subtract),
        ("multiply total by 1", ArithmeticCommand::Multiply),
        ("divide total by 1", ArithmeticCommand::Divide),
    ];
    for (source, expected) in cases {
        let StatementKind::Arithmetic {
            operator,
            value,
            target,
        } = statement(source)
        else {
            panic!("expected arithmetic from {source}");
        };
        assert_eq!(operator, expected);
        assert_eq!(value, Expr::Number(1.0));
        assert_eq!(target.base, ContainerBase::Variable("total".into()));
    }
}

#[test]
fn a_single_line_if_needs_no_end() {
    let StatementKind::If {
        branches,
        otherwise,
    } = statement("if x then beep")
    else {
        panic!("expected an if");
    };
    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0].body.len(), 1);
    assert!(otherwise.is_none());
}

#[test]
fn a_single_line_if_may_have_a_single_line_else() {
    let StatementKind::If { otherwise, .. } = statement("if x then beep else beep") else {
        panic!("expected an if");
    };
    assert_eq!(otherwise.unwrap().len(), 1);
}

#[test]
fn else_if_chains_become_a_list_of_branches() {
    let StatementKind::If {
        branches,
        otherwise,
    } = statement(
        "if x = 1 then\n\
           beep\n\
         else if x = 2 then\n\
           beep\n\
         else\n\
           beep\n\
         end if",
    )
    else {
        panic!("expected an if");
    };
    assert_eq!(branches.len(), 2);
    assert_eq!(otherwise.unwrap().len(), 1);
}

#[test]
fn then_may_sit_on_the_next_line() {
    let StatementKind::If { branches, .. } = statement("if x\nthen\n  beep\nend if") else {
        panic!("expected an if");
    };
    assert_eq!(branches.len(), 1);
}

#[test]
fn every_repeat_form_is_understood() {
    let cases = [
        ("repeat\nbeep\nend repeat", RepeatControl::Forever),
        ("repeat forever\nbeep\nend repeat", RepeatControl::Forever),
        (
            "repeat 3 times\nbeep\nend repeat",
            RepeatControl::Times(Expr::Number(3.0)),
        ),
        (
            "repeat for 3\nbeep\nend repeat",
            RepeatControl::Times(Expr::Number(3.0)),
        ),
        (
            "repeat while x\nbeep\nend repeat",
            RepeatControl::While(Expr::Variable("x".into())),
        ),
        (
            "repeat until x\nbeep\nend repeat",
            RepeatControl::Until(Expr::Variable("x".into())),
        ),
    ];
    for (source, expected) in cases {
        let StatementKind::Repeat { control, body } = statement(source) else {
            panic!("expected a repeat from {source}");
        };
        assert_eq!(control, expected, "from {source}");
        assert_eq!(body.len(), 1);
    }
}

#[test]
fn repeat_with_counts_up_or_down() {
    let StatementKind::Repeat {
        control:
            RepeatControl::With {
                variable,
                from,
                to,
                down,
            },
        ..
    } = statement("repeat with i = 10 down to 1\nbeep\nend repeat")
    else {
        panic!("expected a counted repeat");
    };
    assert_eq!(variable, "i");
    assert_eq!(from, Expr::Number(10.0));
    assert_eq!(to, Expr::Number(1.0));
    assert!(down);
}

#[test]
fn loops_can_be_left_early() {
    assert_eq!(
        body("repeat\nexit repeat\nnext repeat\nend repeat"),
        vec![StatementKind::Repeat {
            control: RepeatControl::Forever,
            body: vec![
                hyperlab_parser::ast::Statement::new(StatementKind::Exit(ExitTarget::Repeat), 3),
                hyperlab_parser::ast::Statement::new(StatementKind::NextRepeat, 4),
            ],
        }]
    );
}

#[test]
fn exit_leaves_a_handler_or_everything() {
    assert_eq!(
        statement("exit test"),
        StatementKind::Exit(ExitTarget::Handler("test".into()))
    );
    assert_eq!(
        statement("exit to HyperLab"),
        StatementKind::Exit(ExitTarget::Everything)
    );
}

#[test]
fn return_may_carry_a_value_or_not() {
    assert_eq!(statement("return"), StatementKind::Return(None));
    assert_eq!(
        statement("return 1"),
        StatementKind::Return(Some(Expr::Number(1.0)))
    );
}

#[test]
fn globals_are_declared_in_a_list() {
    assert_eq!(
        statement("global counter, name"),
        StatementKind::Global(vec!["counter".into(), "name".into()])
    );
}

#[test]
fn go_understands_the_usual_destinations() {
    let cases = [
        ("go back", Destination::Back),
        (
            "go next",
            Destination::Card(Specifier::Ordinal(Ordinal::Next)),
        ),
        (
            "go to next card",
            Destination::Card(Specifier::Ordinal(Ordinal::Next)),
        ),
        (
            "go to first card",
            Destination::Card(Specifier::Ordinal(Ordinal::First)),
        ),
        (
            "go to card 3",
            Destination::Card(Specifier::Value(Expr::Number(3.0))),
        ),
        (
            "go to card id 12",
            Destination::Card(Specifier::Id(Expr::Number(12.0))),
        ),
        (
            r#"go to card "Home""#,
            Destination::Card(Specifier::Value(Expr::Text("Home".into()))),
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(
            statement(source),
            StatementKind::Go(expected),
            "from {source}"
        );
    }
}

#[test]
fn send_names_a_message_and_a_target() {
    let StatementKind::Send { message, target } = statement(r#"send "mouseUp" to button 1"#) else {
        panic!("expected a send");
    };
    assert_eq!(message, Expr::Text("mouseUp".into()));
    assert!(matches!(target, ObjectRef::Part { .. }));
}

#[test]
fn anything_unrecognised_becomes_a_command() {
    assert_eq!(
        statement(r#"answer "Hello""#),
        StatementKind::Command {
            name: "answer".into(),
            arguments: vec![Expr::Text("Hello".into())],
        }
    );
    assert_eq!(
        statement("beep"),
        StatementKind::Command {
            name: "beep".into(),
            arguments: vec![],
        }
    );
}

#[test]
fn commands_accept_comma_separated_and_parenthesised_arguments() {
    let expected = vec![Expr::Number(1.0), Expr::Number(2.0)];
    for source in ["doThing 1, 2", "doThing(1, 2)"] {
        let StatementKind::Command { arguments, .. } = statement(source) else {
            panic!("expected a command from {source}");
        };
        assert_eq!(arguments, expected, "from {source}");
    }
}

#[test]
fn a_parenthesised_first_argument_is_not_mistaken_for_an_argument_list() {
    let StatementKind::Command { arguments, .. } = statement(r#"answer (1 + 2) & "!""#) else {
        panic!("expected a command");
    };
    assert_eq!(arguments.len(), 1);
    assert!(matches!(
        arguments[0],
        Expr::Binary {
            operator: BinaryOp::Concat,
            ..
        }
    ));
}

#[test]
fn ask_takes_a_trailing_with_argument() {
    let StatementKind::Command { arguments, .. } = statement(r#"ask "Name?" with "Bob""#) else {
        panic!("expected a command");
    };
    assert_eq!(
        arguments,
        vec![Expr::Text("Name?".into()), Expr::Text("Bob".into())]
    );
}

// --------------------------------------------------------------- expressions

#[test]
fn arithmetic_binds_tighter_than_comparison() {
    assert_eq!(
        expression("1 + 2 * 3 > 4"),
        Expr::Binary {
            operator: BinaryOp::Greater,
            left: Box::new(Expr::Binary {
                operator: BinaryOp::Add,
                left: Box::new(Expr::Number(1.0)),
                right: Box::new(Expr::Binary {
                    operator: BinaryOp::Multiply,
                    left: Box::new(Expr::Number(2.0)),
                    right: Box::new(Expr::Number(3.0)),
                }),
            }),
            right: Box::new(Expr::Number(4.0)),
        }
    );
}

#[test]
fn concatenation_binds_looser_than_arithmetic() {
    let Expr::Binary { operator, left, .. } = expression(r#"1 + 2 & "x""#) else {
        panic!("expected a binary expression");
    };
    assert_eq!(operator, BinaryOp::Concat);
    assert!(matches!(
        *left,
        Expr::Binary {
            operator: BinaryOp::Add,
            ..
        }
    ));
}

#[test]
fn and_binds_tighter_than_or() {
    let Expr::Binary {
        operator, right, ..
    } = expression("a or b and c")
    else {
        panic!("expected a binary expression");
    };
    assert_eq!(operator, BinaryOp::Or);
    assert!(matches!(
        *right,
        Expr::Binary {
            operator: BinaryOp::And,
            ..
        }
    ));
}

#[test]
fn power_is_right_associative() {
    let Expr::Binary { right, .. } = expression("2 ^ 3 ^ 2") else {
        panic!("expected a binary expression");
    };
    assert!(matches!(
        *right,
        Expr::Binary {
            operator: BinaryOp::Power,
            ..
        }
    ));
}

#[test]
fn parentheses_override_precedence() {
    let Expr::Binary { operator, .. } = expression("(1 + 2) * 3") else {
        panic!("expected a binary expression");
    };
    assert_eq!(operator, BinaryOp::Multiply);
}

#[test]
fn the_word_operators_are_understood() {
    let cases = [
        ("a is b", BinaryOp::Equal),
        ("a is not b", BinaryOp::NotEqual),
        ("a contains b", BinaryOp::Contains),
        ("a is in b", BinaryOp::IsIn),
        ("a starts with b", BinaryOp::StartsWith),
        ("a ends with b", BinaryOp::EndsWith),
        ("a div b", BinaryOp::IntegerDivide),
        ("a mod b", BinaryOp::Modulo),
    ];
    for (source, expected) in cases {
        let Expr::Binary { operator, .. } = expression(source) else {
            panic!("expected a binary expression from {source}");
        };
        assert_eq!(operator, expected, "from {source}");
    }
}

#[test]
fn is_not_in_negates_the_whole_comparison() {
    let Expr::Unary { operator, operand } = expression("a is not in b") else {
        panic!("expected a negation");
    };
    assert_eq!(operator, UnaryOp::Not);
    assert!(matches!(
        *operand,
        Expr::Binary {
            operator: BinaryOp::IsIn,
            ..
        }
    ));
}

#[test]
fn constants_are_not_variables() {
    assert_eq!(expression("empty"), Expr::Constant("empty".into()));
    assert_eq!(expression("TRUE"), Expr::Constant("true".into()));
    assert_eq!(
        expression("notAConstant"),
        Expr::Variable("notAConstant".into())
    );
}

#[test]
fn the_something_of_something_is_left_for_the_runtime_to_interpret() {
    let Expr::Of { name, operand } = expression(r#"the visible of button "Go""#) else {
        panic!("expected an `of` expression");
    };
    assert_eq!(name, "visible");
    assert!(matches!(*operand, Expr::Object(ObjectRef::Part { .. })));

    let Expr::Of { name, .. } = expression("the length of x") else {
        panic!("expected an `of` expression");
    };
    assert_eq!(
        name, "length",
        "the parser must not decide whether this is a property or a function"
    );
}

#[test]
fn qualified_function_names_are_kept_together() {
    assert_eq!(expression("the long date"), Expr::The("long date".into()));
    assert_eq!(expression("the date"), Expr::The("date".into()));
}

#[test]
fn functions_can_be_called_with_parentheses() {
    assert_eq!(
        expression("random(10)"),
        Expr::Call {
            name: "random".into(),
            arguments: vec![Expr::Number(10.0)],
        }
    );
}

#[test]
fn counting_knows_what_it_counts() {
    assert_eq!(
        expression("the number of cards"),
        Expr::Count(Box::new(CountTarget::Cards))
    );
    let Expr::Count(target) = expression("the number of background buttons") else {
        panic!("expected a count");
    };
    assert_eq!(
        *target,
        CountTarget::Parts {
            kind: PartKind::Button,
            layer: Layer::Background,
            owner: None,
        }
    );
    let Expr::Count(target) = expression(r#"the number of words of field "Notes""#) else {
        panic!("expected a count");
    };
    assert!(matches!(
        *target,
        CountTarget::Chunks {
            kind: ChunkKind::Word,
            ..
        }
    ));
}

#[test]
fn chunks_bind_tighter_than_operators() {
    let Expr::Binary { operator, left, .. } = expression(r#"word 1 of x & "!""#) else {
        panic!("expected a binary expression");
    };
    assert_eq!(operator, BinaryOp::Concat);
    assert!(matches!(*left, Expr::Chunk { .. }));
}

#[test]
fn chunk_ranges_record_both_ends() {
    let Expr::Chunk { chunks, .. } = expression("char 2 to 5 of x") else {
        panic!("expected a chunk");
    };
    assert_eq!(chunks[0].kind, ChunkKind::Char);
    assert_eq!(*chunks[0].start, Expr::Number(2.0));
    assert_eq!(chunks[0].end.as_deref(), Some(&Expr::Number(5.0)));
}

#[test]
fn object_references_carry_their_layer_and_owner() {
    let Expr::Object(ObjectRef::Part {
        kind,
        layer,
        specifier,
        owner,
    }) = expression(r#"card field "Name" of card 2"#)
    else {
        panic!("expected a field reference");
    };
    assert_eq!(kind, PartKind::Field);
    assert_eq!(layer, Layer::Card);
    assert_eq!(*specifier, Specifier::Value(Expr::Text("Name".into())));
    assert!(matches!(owner.as_deref(), Some(ObjectRef::Card(_))));
}

#[test]
fn the_short_forms_mean_the_same_as_the_long_ones() {
    assert_eq!(expression("btn 1"), expression("button 1"));
    assert_eq!(expression("bg fld 1"), expression("background field 1"));
    assert_eq!(expression("cd 1"), expression("card 1"));
}

#[test]
fn me_and_the_target_are_object_references() {
    assert_eq!(expression("me"), Expr::Object(ObjectRef::Me));
    assert_eq!(expression("the target"), Expr::Object(ObjectRef::Target));
    assert_eq!(
        expression("this card"),
        Expr::Object(ObjectRef::Card(Box::new(Specifier::Current)))
    );
}

#[test]
fn existence_can_be_tested() {
    let Expr::Exists { negated, .. } = expression(r#"there is a card "Home""#) else {
        panic!("expected an existence test");
    };
    assert!(!negated);
    let Expr::Exists { negated, .. } = expression(r#"there is no card "Home""#) else {
        panic!("expected an existence test");
    };
    assert!(negated);
}

// -------------------------------------------------------------------- errors

#[test]
fn errors_point_at_the_offending_line() {
    let error = parse("on test\n  put 1 into\nend test").unwrap_err();
    assert_eq!(error.line, 2);
}

#[test]
fn an_unclosed_repeat_is_reported() {
    let error = parse("on test\n  repeat\n    beep\nend test").unwrap_err();
    assert!(
        error.message.contains("repeat"),
        "unhelpful message: {}",
        error.message
    );
}

#[test]
fn trailing_rubbish_after_a_statement_is_rejected() {
    let error = parse("on test\n  beep 1 2\nend test").unwrap_err();
    assert!(
        error.message.contains("end of the line"),
        "unhelpful message: {}",
        error.message
    );
}

#[test]
fn a_trailing_unit_is_passed_through_as_an_argument() {
    let StatementKind::Command { name, arguments } = statement("wait 2 seconds") else {
        panic!("expected a command");
    };
    assert_eq!(name, "wait");
    assert_eq!(
        arguments,
        vec![Expr::Number(2.0), Expr::Variable("seconds".into())]
    );
}

#[test]
fn an_object_specifier_stops_before_an_operator() {
    let Expr::Binary { operator, left, .. } = expression(r#"field "Name" & "!""#) else {
        panic!("expected a concatenation, not a field with a strange name");
    };
    assert_eq!(operator, BinaryOp::Concat);
    assert!(matches!(*left, Expr::Object(ObjectRef::Part { .. })));
}

#[test]
fn a_bracketed_specifier_may_be_as_complicated_as_it_likes() {
    let Expr::Object(ObjectRef::Part { specifier, .. }) = expression(r#"field ("a" & "b")"#) else {
        panic!("expected a field reference");
    };
    assert!(matches!(
        *specifier,
        Specifier::Value(Expr::Binary {
            operator: BinaryOp::Concat,
            ..
        })
    ));
}
