# Changelog

HyperLab follows [semantic versioning](https://semver.org). While the major
number is 0 the stack format and the language may still change; when either
does, the entry says so and says what to do about it.

Each entry is written for the person deciding whether to upgrade, so it
describes what is now possible rather than which files moved.

## Unreleased

HyperLab now runs in a browser, whole:
**[jgalego.github.io/HyperLab](https://jgalego.github.io/HyperLab/)**.

- **The website.** A retro landing page with a short history of HyperCard,
  and a playground that is the full application — the same renderer the
  desktop draws, over the same runtime compiled to WebAssembly. The six
  examples are one click away, scripts block on real dialogs, and the map,
  the message box, the inspector and undo all work as they do at home.
- **Files travel.** The playground opens and downloads stacks as the
  single-file JSON the desktop already reads and writes, so a stack moves
  between the two by moving one file.
- **Bring your own model, or none.** AI ▸ Settings in the browser takes
  Anthropic, OpenAI, OpenRouter, anything OpenAI-compatible, or the built-in
  mock. A key is kept in the browser's own storage, is never readable back,
  and travels only to the provider it belongs to — the site is static files,
  with no server of its own.
- The Todo example's list now ends with a return, so the first item anyone
  adds lands on its own line instead of gluing itself to the last one.

For integrators: `hyperlab-persistence` gained in-memory single-file
functions (`single_file_string`, `stack_from_single_file`);
`hyperlab-ai-providers` gained a default-on `native` feature — without it
the crate is only the wire protocol, for hosts that bring their own
transport; and `hyperlab_stack::set_clock` lets a host name the clock on
platforms where the standard one traps, which WebAssembly in a browser is.

## 0.2.0 — 2026-08-01

A stack that cannot leave HyperLab is a stack you cannot show anyone. This
release is four ways out, and nothing you had before has changed.

- **File ▸ Export as PDF…** — the cards as a document, one page each.
- **File ▸ Export as a Web Page…** — the whole stack as one HTML file,
  running on [_hyperscript](https://hyperscript.org).
- **File ▸ Export as a Decker Deck…** — the whole stack as a
  [Decker](https://beyondloom.com/decker/) deck, scripts translated into Lil.
- **Go ▸ Map ▸ Save as PNG…** — the map as a picture.
- Every example is checked in as a PDF, so the exporter's output can be read
  without building anything.
- A release can now be cut from a branch: the pipeline reads the version out
  of the bundle and makes the tag itself.

### Getting a stack out of HyperLab

**File ▸ Export as PDF…** writes the whole stack as a document, one page per
card, the size of the card. The artwork goes in as vector graphics rather than
a photograph of itself, and the words go in as words — Helvetica, which every
reader already has — so an exported stack can be searched, copied out of, and
printed at any size without going soft.

**Go ▸ Map ▸ Save as PNG…** saves the map, at twice the size it is shown, with
the same greyed dead ends and bold current card.

**File ▸ Export as a Web Page…** translates the stack into
[_hyperscript](https://hyperscript.org), HyperTalk's descendant on the web,
and writes one HTML file that runs on its own. Five of the six examples come
across with nothing left behind — Cluedo is playable in a browser, Recipe
Box still doubles its ingredients.

**File ▸ Export as a Decker Deck…** writes the stack for
[Decker](https://beyondloom.com/decker/), John Earnest's platform and another
descendant of HyperCard. Cards become cards, buttons and fields become
widgets, and the artwork is redrawn in one bit — which suits it. The scripts
are translated into Lil, which shares nothing with HyperTalk but its ancestry,
so all six examples were opened in Decker and played: Cluedo names a suspect
across cards, Todo counts what is left, Recipe Box doubles its ingredients.

Each of the four says what it could not carry. A page has no language model
and a deck has neither that nor a moment of opening, so those become a
comment where they belonged and a line in the message — a partial translation
is never dressed as a whole one.

### In memoriam

This release is dedicated to Bill Atkinson, 1951–2025, who wrote HyperCard.

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
