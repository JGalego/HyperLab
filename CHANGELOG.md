# Changelog

HyperLab follows [semantic versioning](https://semver.org). While the major
number is 0 the stack format and the language may still change; when either
does, the entry says so and says what to do about it.

Each entry is written for the person deciding whether to upgrade, so it
describes what is now possible rather than which files moved.

## 0.1.0 — 2026-08-01

The first release. Everything below is new, because nothing came before it.

### Cards and stacks

- Stacks, backgrounds, cards, buttons, fields and pictures, all sharing one
  object core and one open-ended property system, so a property nobody
  anticipated needs no schema change to store.
- Every mutation is a `Command` that returns the command reversing it, which
  is why undo and redo cannot drift out of step with the model.
- HyperCard-style message passing out along `button → card → background →
  stack`, with `pass` and `exit` doing what a HyperCard author expects.
- `.hl` bundles: a directory, one JSON file per card, scripts kept as plain
  `.hypertalk` files and images as real `.png` or `.svg`. A stack diffs,
  merges and greps like source code, and a card that goes wrong can be
  attached to a bug report on its own.

### The language

A working subset of HyperTalk, documented in
[`docs/hypertalk.md`](docs/hypertalk.md): handlers, functions, variables and
globals, `if`, every `repeat` form, chunk expressions, object references,
properties, and the commands and functions listed there. Scripts take effect
the moment they are saved — there is no build step inside a stack.

What is missing is named in that document rather than left to be discovered:
`find`, `sort`, `the selection`, `visual effect`, `do`, `the clickLoc`, the
type tests, and painting of any kind.

### The application

A Tauri shell around a React renderer, in the Neo Classic theme — drawn from
scratch in the spirit of 1987, containing no Apple code or artwork.

- Browse and edit tools, drag to move, object inspector, property editor,
  script editor, card navigation and the message box.
- Open, save and save-as.
- Modal `answer` and `ask`: a script stops until the dialog is dismissed and
  the answer reaches the next line. Commands run off the message loop, so the
  window stays responsive while a script waits — or loops.
- **Go ▸ Map** draws the stack as a graph and names the three things a stack
  cannot tell you about itself: cards nothing leads to, cards with no way
  out, and links pointing at a card that is not there.

### Asking a language model

AI is part of the programming model rather than a panel beside it, and it
arrives three ways over one implementation:

- **In a script** — `put ai("Summarize this card") into field "Summary"` and
  `ask assistant "…"`. The seam is `Host::ai`, alongside `answer` and `ask`;
  the runtime passes the author's words through untouched and does not depend
  on any AI crate.
- **In the sidebar** — *explain this script*, *add a search button*. It edits
  only through the MCP tools, so an assistant's change lands in the undo
  history and is indistinguishable from one a person made. A switch says
  whether it may edit at all.
- **Over MCP** — `hyperlab-mcp --stack Todo.hl` hands the same tools to any
  client that speaks the protocol, read-only until told otherwise.

No provider is special: a provider implements `AiProvider`, and nothing in
HyperLab switches on which one it is. Two clients ship — Anthropic's Messages
API, and the OpenAI chat-completions protocol, which OpenRouter, Ollama, LM
Studio, llama.cpp and vLLM all accept, so pointing a `baseUrl` at a local
server runs with no network at all.

HyperLab works with no provider configured, and is meant to keep working that
way for ever. A key goes to the keychain the operating system already runs, or
is named as an environment variable; the settings file records the *place* and
has nowhere to put the value. Every question in the transcript carries the
exact text that was sent, and field contents are left out unless a second
switch is on.

### MCP, in both directions

- **Out.** The server speaks MCP over stdin and stdout.
- **In.** A client starts an external server and calls its tools. It is
  treated as hostile on purpose: no shell to escape from, a timeout on every
  reply, over-long lines end the conversation rather than filling memory, and
  a server can be handed a fresh environment.
- **Permissions** on every call — which stacks, which tools, and whether a
  person was actually asked. Read-only unless told otherwise, because the
  server is usually started by other software with nobody watching.

### Command-line tools

Both ship as plain executables, installable with the one-liners in the
[README](README.md#getting-started):

- `hyperlab-mcp` — serves a stack over MCP.
- `hyperlab-graph` — writes a stack as Graphviz, as JSON, or as a report that
  exits non-zero, which is enough to fail a build on a broken link.

### Examples

Six stacks, which are also tests: **Cluedo**, **Myst**, **LLMs for n00bs**,
**Address Book**, **Recipe Box** and **Todo**. CI regenerates them and
compares byte for byte, so an example cannot quietly stop being what the
generator produces.

### Not in this release

[Phase 7 and Phase 8](docs/roadmap.md) — loading plugins at run time and
sharing the command log between two people — are designed for and not built.
Also still to come: a HyperTalk debugger, streaming answers, and choosing
external MCP servers from the interface rather than in code.
