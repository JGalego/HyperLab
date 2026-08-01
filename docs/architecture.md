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

AI is implemented through providers, and HyperLab does not pick one.

```
                    AiProvider  (crates/ai — the interface)
                         ▲
        ┌────────────────┼────────────────┐
   MockProvider    OpenAiProvider   AnthropicProvider
   (crates/ai)     └──── crates/ai-providers ────┘
```

All providers implement the same interface:

```rust
pub trait AiProvider: Send + Sync {
    fn name(&self) -> &str;
    fn capabilities(&self) -> Capabilities;
    fn complete<'a>(&'a self, request: CompletionRequest) -> BoxFuture<'a, AiResult<Completion>>;
    fn embed<'a>(&'a self, texts: Vec<String>) -> BoxFuture<'a, AiResult<Vec<Embedding>>>;
}
```

The split between the two crates is the whole design.

`hyperlab-ai` holds the interface, the message types and the context builder.
It has no HTTP client, no vendor SDK and no knowledge that OpenAI or Anthropic
exist. `ProviderKind` is a list of names for settings files, and nothing
switches on it.

`hyperlab-ai-providers` holds the clients, and is the only crate allowed to
know a vendor. It ships two, which between them cover most of the field:

| Client | Speaks | Also serves |
| --- | --- | --- |
| `OpenAiProvider` | chat completions | OpenRouter, Ollama, LM Studio, llama.cpp, vLLM — anything with a `baseUrl` |
| `AnthropicProvider` | the Messages API | — |

Adding a third means adding a module and one arm of `build`. Nothing else
changes, because nothing else knows.

Where the two protocols differ, the difference is handled at the edge and
never leaks inward: Anthropic hoists system messages into a field of their
own, carries content as typed blocks, puts a tool result inside a *user* turn,
and requires `max_tokens`. A `CompletionRequest` knows none of that.

## Keys

A `ProviderConfig` names the environment variable that holds a key. It never
holds the key, so a settings file can be copied into a bug report. A provider
can also be handed a key directly — `with_api_key` — for an embedder that
reads the operating system's keychain instead.

A variable that is named but unset is an error when the provider is built,
not a puzzling refusal later.

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

AI assistants interact through these tools.

Never allow direct mutation.

## Transport

What a tool *is* and how it is *delivered* stay separate, which is why the
tool table needed no changes to gain a transport.

```
    another MCP client ──► Server ──► Policy ──► ToolRegistry ──► Command
                                        │
    a stack ──────────────► Client ─────┘──► somebody else's MCP server
```

`Server` reads from any `BufRead` and writes to any `Write`, so the stdio
binary is four lines and the tests run over a pair of in-memory buffers.
`Client` is the same protocol pointed outwards.

Everything crosses `Policy`, and that is the point of it being a type rather
than a few `if`s: a caller at the far end of a pipe is not the user. It
decides three separate things, because they fail differently — which stacks a
connection may touch, which tools it may use, and whether a person was asked.
Each tool declares its own `Access`, so "read only" is checked against the
tool table and cannot drift out of step with a list of names.

## What an external server is assumed to be

A program someone else wrote, which may be a mistake or worse:

| Risk | What prevents it |
| --- | --- |
| A stack builds a command line | There is no command line. A program and its arguments are separate fields, handed to the operating system as they stand, so no shell ever sees them |
| A server hangs | Replies are read on their own thread, with a timeout; one call is lost, not the application |
| A server floods the pipe | A line over 16 MiB ends the conversation instead of being buffered |
| A server reads API keys | It inherits this process's environment only if it is not given a clean one |
| A tool description gives instructions | Descriptions and results are carried as data. They are never merged into HyperLab's own tool table, and `ExternalTool` keeps the server's name attached, so it is always clear whose tool is whose |

A server is told the session is over by closing the pipe, and then left alone
for a few seconds before it is killed — long enough to save what it was doing.

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

# AI-Native HyperTalk

The syntax the architecture was shaped for, and which it now accepts:

```hypertalk
put ai("Summarize this card") into field "Summary"

ask assistant "Generate three cards"

ask assistant "Explain this script"

if ai("Should this customer receive a discount?") is "yes" then
    ...
end if
```

It required no new runtime, and almost no new grammar. `ai("…")` was already
a well-formed call — `Expr::Call` — so it is one arm in `call_function`,
placed there rather than in the built-in table because everything in that
table is a pure function of its arguments and this one has to reach the
outside world. `ask assistant` is the single two-word command name in the
language, joined in the parser so that `assistant` is not read as a variable.

The seam is `Host::ai`, which sits beside `answer` and `ask` and blocks the
same way for the same reason: the value has to reach the next line. What
crosses it is an `AiRequest` — the author's words, and whether answering may
change the stack. The runtime adds nothing to those words. It cannot: it does
not depend on `hyperlab-ai`, and the arrow must keep pointing the way it does.

| | `ai("…")` | `ask assistant "…"` |
| --- | --- | --- |
| Is a | function | command |
| May change the stack | no | yes, through commands |
| Answer lands in | the expression | `it` |
| When nothing is set up | the handler stops | `the result` says why |

The last row is the difference that matters. A command can report a failure
and let the handler carry on, exactly as a cancelled `ask` does, so a stack
that uses the assistant still runs where no model is configured. An
expression in the middle of a line has no honest value to be.

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
    ai-providers/   OpenAI and Anthropic
                    clients               — how they are reached
    mcp/            tools, MCP server
                    and client            — what may be done

apps/desktop/
    src-tauri/      the shell: state, commands, view model
    src/            React: renderer, inspector, editors, theme
```

The dependency graph runs one way, and is enforced by Cargo:

```
        stack ←── persistence
          ↑  ↖
          │    ai ←── ai-providers
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
| No provider is special | `hyperlab-ai` does not depend on `hyperlab-ai-providers`; the arrow runs the other way |
| A key is never written to disk | `ProviderConfig` has no field for one — only the name of an environment variable |
| A caller over a pipe is not the user | Every MCP call crosses `Policy`, which is read-only until told otherwise |
| A stack cannot build a shell command | `Launch` keeps the program and its arguments in separate fields; no shell is involved |

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
