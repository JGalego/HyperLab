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

## Graph

Responsible for:

- reading every `go` out of every script
- resolving each one to a card, where that can be known
- reachability, dead ends and broken links
- DOT output

`hyperlab-graph` is a pure function of a `Stack`: it takes a borrow, runs
nothing, and holds no state, so its answer can never be stale and there is no
cache to invalidate. It depends on the parser and the stack model and on
nothing else — not the runtime, not the interpreter — which is what keeps a
static reading static.

The limit is the point rather than a gap. A destination the script computes
is reported as unresolved, and a destination naming a card that is not there
is reported as missing. Guessing at either would make the drawing a
liability: an author who trusts a map that invents routes is worse off than
one with no map.

---

# Core Runtime Objects

```
Application
    StackManager

Stack
    Backgrounds
    Cards
    Images          the pictures it carries, by name

Background
    Parts

Card
    Parts

Part
    Button
    Field
    Image           draws one of the stack's pictures

Script
```

Everything derives from Object.

A picture is a *kind of part*, not a kind of object, and that is why it can
be moved, hidden, scripted, clicked and undone without any of it being
written twice. The bytes belong to the stack rather than to the part, so two
cards can draw the same picture and a bundle holds one copy.

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

## Getting out

Two exports, made in two places, and the split says something about where
things live.

A **PDF of the cards** is built by [`hyperlab-export`](../crates/export) from
the object model: one page per card, the size of the card. Every picture takes
the same route whatever format it arrived in — an SVG goes to `svg2pdf` as it
stands, a PNG is wrapped in a one-element SVG first — so raster and vector
artwork land on the page by one code path. Text is drawn as text in Helvetica,
one of the fourteen fonts a reader supplies itself, so nothing is embedded and
the result can be searched. Nothing there can measure a font, so the line
breaks come from Adobe's own Helvetica metrics.

A **PNG of the map** is made by the window. The map's shape is a force-directed
layout the renderer worked out from the graph; the core has never seen it, and
rebuilding it in Rust would mean two layouts that drift. So the window
serialises its own `<svg>`, carrying the stylesheet rules that dress it, draws
it to a canvas at twice size, and the shell only writes the bytes — after
checking they really are a PNG.

The two do not agree to the pixel and are not meant to. The desktop draws a
card with a browser's idea of a line break and the exporter with a PDF's; they
agree on what is on the card and where, and differ on where a long line wraps.

## Going somewhere else

[`hyperlab-hyperscript`](../crates/hyperscript) writes a stack as one HTML
file driven by [_hyperscript](https://hyperscript.org). Carson Gross wrote
that language for the web after the same HyperTalk this reimplements, so most
of the translation is a change of address: `put x into y`, `set x to y`,
`if … then … end`, `repeat while`, `is not`, `starts with` and `contains` all
survive the trip unaltered.

The interesting part is where they differ, and every difference below was
found by loading a generated page in a browser rather than by reading the
documentation — each one parses, or runs, and is quietly wrong.

| HyperTalk | Why it cannot go straight across |
| --- | --- |
| `it` | `it` is the previous command's result. `set it to …` is accepted and yields `null`, so it becomes `hlIt`. |
| `repeat with i = 1 to n` | Not a form _hyperscript has. Counting is left to `repeat … times index`, so `next repeat` cannot skip an increment and loop for ever. |
| `repeat (a + b) times` | The count has to be a name, not an expression. |
| `word 2 of x` | Splitting on whitespace needs a regular expression, and a literal one is a parse error inside a handler. |
| `ask assistant` | Two words, and a page has no model. |

What is left over is a dozen lines of glue at the foot of the page: which card
is showing, how to reach another, the two card messages HyperLab sends, and
three helpers for the text operations above. A script calls into them the way
it calls anything else.

Anything with no equivalent becomes a comment where it belonged **and** a line
in `Translation::notes`, so a partial translation cannot be mistaken for a
whole one. Five of the six examples come across with no notes at all; the
sixth asks a language model on its last card.

## Going somewhere older

[`hyperlab-decker`](../crates/decker) writes a stack as a
[Decker](https://beyondloom.com/decker/) deck. John Earnest's platform is
another descendant of HyperCard, and where the web page was a change of
address, this is a translation: Decker's language is Lil, and Lil is nothing
like a HyperTalk. Assignment is `x:1`, a call is `f[a b]`, `if` takes no
`then`, and expressions evaluate right to left with no precedence at all.

Same discipline as the page — every line below was found by opening a
generated deck in Decker rather than by reading about it.

| HyperTalk | What it takes to get there |
| --- | --- |
| `field "Notes"` | `notes.text`. A field's contents are `value` in the file and `.text` in a script. |
| `field "X" of card "Y"` | `deck.cards.y.widgets.x.text`. |
| `line 2 of x` | Lil has no chunks: split, slice, and join again. |
| `a mod b` | Lil's `%` divides its *right* operand by its left. |
| `if x then next repeat` | Nothing in Lil jumps, so the guard is turned inside out and the rest of the loop becomes its `else`. The same rewrite handles `exit mouseUp`. |
| a picture | Drawn into the card's bitmap, which must be the size of the whole deck. A picture with a script keeps an invisible button over it. |

Artwork becomes an IMG0 record: two big-endian 16-bit numbers, then one bit a
pixel, then base64 — with every forward slash escaped, because a slash begins
a comment in the deck format and one left bare truncates the picture without
a word.

Two things have no room at all. A handler that asks a language model, and the
moment a stack opens: a deck starts on its first card already, and its own
`view` fires on every card's arrival rather than the deck's, so an `openStack`
handler moved there would run again on every move.

## Keys

A `ProviderConfig` names a `KeySource` — an environment variable, or the
operating system's keychain — and never the key itself. There is no field
that could hold one, so a settings file can be copied into a bug report.

`hyperlab-ai` cannot open a keychain and does not try. It declares the
`Keychain` trait; the desktop shell implements it over Keychain Services, the
Credential Manager or the Secret Service, in
[`keys.rs`](../apps/desktop/src-tauri/src/keys.rs) — the one module in
HyperLab that ever holds a key. `build` is given one and hands what it finds
to `with_api_key`, which is the only way a client ever learns a secret.

The key travels one way. `keys::holds` answers whether a provider has one;
nothing returns it to the interface, so the settings panel can show that a key
is saved and cannot show what it is.

A place that is named and found empty is an error when the provider is built,
not a puzzling refusal later.

---

# The Assistant

`hyperlab-assistant` is the AI Layer named at the top of this document:
prompt assembly, tool execution and the conversation. It is the only crate
that knows what a prompt looks like.

A turn is deliberately in two halves, and the reason is the session lock:

```
  lock    Briefing::about, Conversation::ask     reads the stack
  unlock  provider.complete(...).await           slow, needs no stack
  lock    tools::run, record_tool                writes the stack
          … repeat while the model asks for tools
```

Holding the lock across the network request would stall every other command,
including `dialog_reply` — which is how a script blocked on `ask` gets
unstuck. So `Conversation` holds the state between the halves, and neither
half has to know the other exists.

Three properties fall out of the arrangement rather than being promised:

| | Held up by |
| --- | --- |
| An assistant can do only what a person can | Tools are `hyperlab-mcp`, which wraps commands. There is no other write path |
| Nothing runs that the user did not allow | Every call crosses a `Policy`; the sidebar's switch is what sets it |
| What is shown is what was sent | `Briefing` is one string, used both as the message and as the disclosure |

The system prompt says that a stack's contents are data rather than
instructions. That does not make prompt injection harmless on its own — the
policy is what actually stops it — but it is the difference between an
assistant that asks and one that complies.

## What a script gets, and why it is less

`ai("…")` and `ask assistant "…"` reach the same provider without tools and
without a briefing. Both limits are deliberate. A script is already inside
the runtime, mid-handler, holding the lock: an assistant restructuring the
stack between two statements would invalidate the references the interpreter
is standing on. And no briefing is needed, because the script says what to
send — `ai("Summarize: " & field "Notes")` sends that field and nothing else.

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
    assistant/      prompts, the tool
                    loop, the transcript  — what is actually said
    mcp/            tools, MCP server
                    and client            — what may be done
    graph/          the routes between
                    cards                 — where it all leads
    export/         a stack as a PDF      — how it leaves
    hyperscript/    HyperTalk as
                    _hyperscript          — where it goes next
    decker/         a stack as a Decker
                    deck                  — where else it goes

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
| A key is never written to disk | `ProviderConfig` holds a `KeySource` — a place to look — and has no field for a key |
| The interface cannot read a key back | Nothing in `keys.rs` returns one except the `Keychain` impl the provider layer calls |
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
