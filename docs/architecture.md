# HyperLab Architecture

> **Version:** 0.1
>
> This document defines the architecture of HyperLab.
>
> These architectural decisions take precedence over implementation convenience.
>
> The primary goal is a system that remains understandable, extensible, and maintainable for decades.

---

# Guiding Philosophy

HyperLab is built around five principles.

1. Everything is an object.
2. Everything communicates through messages.
3. Everything is live.
4. Everything is scriptable.
5. AI is another runtime capability—not a special case.

---

# High-Level Architecture

```
                +----------------------+
                |    React Frontend    |
                +----------+-----------+
                           |
                    Command Bus
                           |
                +----------v-----------+
                |    Runtime Engine    |
                +----------+-----------+
                           |
          +----------------+----------------+
          |                |                |
      Stack Engine     Script Engine     AI Layer
          |                |                |
          +----------------+----------------+
                           |
                     Persistence
```

No UI component is permitted to directly manipulate stack data.

Everything flows through runtime commands.

---

# Layer Responsibilities

## UI

Responsible for:

- rendering
- mouse events
- keyboard events
- drag/drop
- menus
- inspectors

The UI should contain almost no business logic.

---

## Runtime

Responsible for:

- object lifecycle
- message dispatch
- event routing
- command execution
- undo
- redo

The runtime owns all mutable state.

---

## Script Engine

Responsible for:

- parsing HyperTalk
- generating AST
- executing handlers
- maintaining execution context
- variables
- message passing

---

## AI Layer

Responsible for:

- LLM providers
- MCP
- prompt assembly
- tool execution
- context windows

The runtime should not know which provider is being used.

---

## Persistence

Responsible for:

- loading stacks
- saving stacks
- migrations
- versioning

Persistence should never contain runtime logic.

---

# Core Runtime Objects

```
Application
    StackManager

Stack
    Backgrounds
    Cards

Background
    Buttons
    Fields

Card
    Buttons
    Fields

Button

Field

Script
```

Everything derives from Object.

---

# Base Object

Every object contains:

```rust
id

name

parent

properties

script

created_at

updated_at
```

Properties should be extensible.

Avoid hardcoding UI assumptions into objects.

---

# Property System

Properties are key/value pairs.

Example:

```
visible

enabled

width

height

left

top

text

style

font

color
```

Future versions may allow custom properties.

---

# Runtime Commands

The runtime is command-driven.

Example:

```
CreateCard

DeleteCard

RenameCard

MoveObject

ResizeObject

EditScript

NavigateCard

SetProperty
```

Every command should support:

- execute
- undo
- redo

---

# Event Pipeline

Events flow like this.

```
Mouse Click

↓

UI Event

↓

Runtime Event

↓

Message

↓

HyperTalk Handler
```

Example

```
mouseUp

↓

button

↓

card

↓

background

↓

stack
```

This mirrors HyperCard's message-passing model.

---

# HyperTalk Compiler

Pipeline

```
Source

↓

Lexer

↓

Parser

↓

AST

↓

Interpreter
```

No execution should occur during parsing.

---

# AST

Example

```
Program

Handler

Statement

Expression

Literal

Identifier

Assignment

If

Repeat

FunctionCall

MessageSend

Return
```

The AST should be immutable.

---

# Runtime Context

Execution context contains:

```
current stack

current card

current background

current target

local variables

global variables

call stack
```

---

# Message Dispatch

Messages should bubble naturally.

```
Button

↓

Card

↓

Background

↓

Stack

↓

Application
```

If nobody handles a message,
it silently ends.

---

# Future Message Types

```
mouseDown

mouseUp

mouseEnter

mouseLeave

openCard

closeCard

openStack

closeStack

idle

selectionChanged

propertyChanged

fieldChanged
```

New messages should require minimal runtime changes.

---

# AI Architecture

AI is implemented through providers.

```
AIProvider

OpenAIProvider

AnthropicProvider

GeminiProvider

OllamaProvider

MockProvider
```

All providers implement the same interface.

Example

```rust
trait AIProvider {

    fn complete(...)

    fn tools(...)

    fn embeddings(...)

}
```

---

# AI Context Builder

The AI layer should assemble context from runtime objects.

Example

```
Current Card

Current Stack

Selected Objects

Scripts

Properties

Conversation History
```

The runtime should not know prompt formats.

---

# MCP

HyperLab should expose runtime capabilities as tools.

Examples

```
current_card()

read_field()

write_field()

create_button()

create_card()

run_script()

search_stack()
```

Future AI assistants interact through these tools.

Never allow direct mutation.

---

# Plugin System

Everything should eventually become a plugin.

```
Renderer

Theme

AI Provider

Persistence

Exporter

Importer

Inspector Panels
```

Avoid singleton objects.

---

# Persistence Format

Directory layout

```
stack.hl/

    stack.json

    cards/

    backgrounds/

    assets/

    scripts/

    metadata.json
```

Future binary formats can be added later.

---

# Renderer

Renderer receives immutable runtime state.

It should never mutate data.

```
Runtime

↓

View Model

↓

Renderer
```

---

# UI Themes

Separate logic from appearance.

Theme package

```
fonts

icons

buttons

colors

spacing

animations
```

Classic and Dark themes should share identical runtime behavior.

---

# Inspector

Inspector observes runtime state.

It does not edit objects directly.

Property changes generate commands.

---

# Undo System

Every mutation must be a command.

```
Command

↓

Execute

↓

History

↓

Undo

↓

Redo
```

Avoid hidden mutations.

---

# Testing Strategy

Three layers.

## Unit Tests

Parser

Lexer

Interpreter

Utilities

---

## Integration Tests

Runtime

Persistence

Commands

Undo

AI interfaces

---

## UI Tests

Rendering

Selection

Inspector

Navigation

---

# Performance

Optimize for correctness first.

When optimization becomes necessary:

- profile first
- optimize second

Never sacrifice readability for hypothetical performance.

---

# Coding Style

Prefer

- composition
- immutable values
- explicit APIs
- small modules
- descriptive names

Avoid

- deep inheritance
- global mutable state
- hidden side effects
- giant files
- magic behavior

---

# Public APIs

Every subsystem should expose a clean API.

Example

```rust
Runtime

StackManager

ScriptEngine

Persistence

AIManager

CommandBus
```

Avoid leaking implementation details.

---

# Future AI-Native HyperTalk

The architecture should allow future syntax such as:

```hypertalk
put ai("Summarize this card") into field "Summary"

ask assistant
    "Generate three cards"

ask assistant
    "Explain this script"

if ai("Should this customer receive a discount?") then
    ...
end if
```

This should require adding parser nodes—not rewriting the runtime.

---

# Long-Term Vision

HyperLab should become a platform for building applications through:

- visual editing
- live programming
- scripting
- AI collaboration

Developers should feel like they are working inside a living system rather than editing source files.

Every design decision should answer one question:

        > **Would this make HyperCard's original creators smile if they were designing it today?**

---

# Implementation Map

> Everything above is the design. This section says where it actually lives,
> and is kept honest by the fact that it names files you can open.

```
crates/
    stack/          the object model      — what a stack is
    parser/         lexer, parser, AST    — what the author wrote
    runtime/        commands, dispatch,
                    the interpreter       — what it means
    persistence/    the .hl bundle        — where it is kept
    ai/             provider interfaces   — who is asked
    mcp/            tools                 — what may be done

apps/desktop/
    src-tauri/      the shell: state, commands, view model
    src/            React: renderer, inspector, editors, theme
```

The dependency graph runs one way, and is enforced by Cargo:

```
        stack ←── persistence
          ↑  ↖
          │    ai
          │     ↑
   parser │     │
       ↘  │     │
        runtime ←── mcp
           ↑
        desktop
```

`parser` depends on nothing at all — not even on the object model — so the
language can be tested, forked or reused on its own.

## Where each rule is enforced

| Rule | Enforced by |
| --- | --- |
| The UI never touches stack data | `Runtime` exposes `stack()` but no `stack_mut()`; the only way in is `execute` |
| Every mutation is undoable | `Command::apply` returns the command that reverses it; there is no other write path |
| Scripts and people take the same path | The interpreter calls `Runtime::execute`, exactly as the menus do |
| AI can do no more than a person | `hyperlab-mcp` tools are wrappers around commands |
| Nothing is evaluated while parsing | `hyperlab-parser` has no dependency it could evaluate against |
| The renderer cannot mutate | It is given a `StackView`, a serialized snapshot, not an object |
| A script can wait for a person | Commands run off the message loop, so `Host::ask` may block while the window stays alive |

## What is deliberately not built yet

Naming these is part of the design: an architecture is as much about the
seams left open as about the code written.

- **Interrupting a running script.** A loop that will not stop can only be
  ended by the million-iteration limit. Stopping one on request needs a flag
  the interpreter checks between statements — the same place the future
  debugger will pause.
- **Parsed-script caching.** Scripts are parsed on every dispatch. This is
  fast enough for stacks of any size a person will write by hand, and the
  moment it is not, the cache belongs in `Runtime` behind `script_of`.
- **Grouped undo.** A script that changes twenty fields leaves twenty entries
  in the history. Grouping them is a change to `History`, not to any command.
- **Plugins.** Renderers, themes, providers and exporters are all already
  behind traits or data. Nothing loads code at run time yet.
