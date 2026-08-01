# Roadmap

> Where HyperLab is, and where it is going.
>
> Each phase ends with something usable. Nothing here is a promise about
> dates; the order matters more than the timing.

---

## Phase 1 — Core runtime ✅

The object model, the command bus and message dispatch.

- Stacks, backgrounds, cards, buttons and fields, all sharing one object core
  and one open-ended property system.
- Every mutation is a command that knows how to reverse itself, so undo and
  redo came free and cannot drift.
- HyperCard-style message passing along `button → card → background → stack`.
- `.hl` bundles: one JSON file per card, one `.hypertalk` file per script.

## Phase 2 — Desktop editor ✅

- Tauri shell, React renderer, the Neo Classic theme.
- Browse and edit tools, drag to move, object inspector, property editor,
  script editor, card navigation, message box.
- Open, save and save-as.
- Modal `answer` and `ask`: a script stops until the dialog is dismissed, and
  the answer reaches the next line. Commands run off the message loop, so the
  window stays responsive while a script waits — or while it loops.

## Phase 3 — HyperTalk ✅ *(a working subset)*

Handlers, functions, variables, globals, `if`, every `repeat` form, chunk
expressions, object references, properties, and the commands and functions
listed in the [reference](hypertalk.md). What is missing is named there too —
`find`, `sort`, the type tests, and painting.

Still to come in this phase:

- A debugger: step, breakpoints, and a look at the frame you are in.
- Caching parsed scripts, once a stack is big enough to notice.
- Grouping a script's edits into one undo step.

## Phase 4 — AI sidebar ✅

The sidebar answers *explain this script*, *add a search button*, *what is
this card for*, and it does so under three rules that are structural rather
than promised.

**It edits through the tools, or not at all.** Every change goes through
[`hyperlab-mcp`](../crates/mcp), which wraps runtime commands, so an
assistant's edit lands in the undo history and is indistinguishable from one
a person made. A switch in the panel says whether it may edit; turning it off
makes the policy read-only, and a refused tool is reported to the model so it
can say why rather than pretending.

**It shows what was sent.** Every question in the transcript carries a *What
was sent* disclosure holding the exact text the model received. It is the
same string, not a summary of it — the panel and the request read one field,
so they cannot drift apart. Field contents are left out unless a second
switch is on, and the disclosure says which.

**It never holds a key.** A key typed into the settings panel goes to the
keychain the operating system already runs; a key already exported is named
by its variable. Either way the settings file records the *place* and has
nowhere to put the value, so it can be copied into a bug report — and nothing
reads a key back out to the interface that took it.

The conversation itself lives in
[`hyperlab-assistant`](../crates/assistant) — the AI layer the architecture
named — and is split so that asking a model and running a tool are separate
steps. That is not tidiness: the session lock is held for the tool calls and
dropped for the network, so a slow model cannot stall the window or the
dialog a script is waiting on.

Still to come in this phase:

- Streaming, so an answer appears as it is written.
- More than one conversation at a time.
- Choosing a provider per question rather than per session.

## Phase 5 — MCP ✅

The tools ([`hyperlab-mcp`](../crates/mcp)) now have a transport, in both
directions, with one thing standing between them.

- **Out.** `hyperlab-mcp --stack Todo.hl` speaks MCP over stdin and stdout,
  so anything that talks to an MCP server can drive HyperLab. The server
  itself reads and writes ordinary streams, which is why it can be tested
  over a pair of buffers as well as over a pipe.
- **In.** `Client` starts an external server and calls its tools, so a stack
  can reach the rest of the world. It is treated as hostile on purpose: the
  program and its arguments are handed to the operating system separately so
  there is no shell to escape from, every reply is read on its own thread
  with a timeout, over-long lines end the conversation rather than filling
  memory, and a server that has no business reading this process's
  environment can be given a fresh one.
- **Permissions**, which every call goes through:
  - *Which stacks* — a policy can name the stacks it covers.
  - *Which tools* — an allowlist, plus a read-only mode enforced by each
    tool's own declared `Access` rather than by a list of names kept in step
    by hand. A tool a caller may never use is not offered to it either.
  - *What the user was asked* — consent is recorded as the decision it is:
    every `Decision` says whether a person was actually consulted and what
    they said, and the whole sequence can be shown afterwards.

Read-only unless told otherwise. The server is started by other software,
usually with nobody watching, and nobody can be asked.

Still to come in this phase:

- Choosing external servers from the interface rather than in code.
- Serving more than one stack from a single connection.

## Phase 6 — AI-native HyperTalk ✅

The language extensions the architecture was shaped for:

```hypertalk
put ai("Summarize this card") into field "Summary"

ask assistant "Generate five cards"

if ai("Should this customer receive a discount?") is "yes" then
  …
end if
```

It did not need a new runtime, and it needed less grammar than expected.
`ai("…")` already parsed as a function call, so it is an arm in the
interpreter and nothing else; `ask assistant` is two words joined in the
parser so that `assistant` is not read as a variable, and is otherwise an
ordinary command.

The seam is [`Host::ai`](../crates/runtime/src/host.rs), alongside `answer`
and `ask` — a blocking call the runtime makes while a script runs. The
runtime passes the author's words through untouched and hands back the reply;
it does not know what a prompt looks like, and cannot, because it does not
depend on any AI crate. Every question is recorded as an `Effect`, whether or
not anything answered.

The two differ in one way: `ai(…)` answers in words, and `ask assistant` asks
for something to be done about it. What a host can actually do about that is
the host's business — in the desktop, both answer in words, because a script
is already inside the runtime and letting an assistant restructure a stack
between two statements would pull the ground out from under the interpreter.
Edits belong in the sidebar, where nothing is mid-handler.

## Pictures ✅

HyperLab could draw buttons and fields and nothing else, which ruled out
most of what people actually made with HyperCard.

A picture is a [`PartKind::Image`](../crates/stack/src/part.rs), not a new
kind of object, and the feature is mostly the dividend of that: `hide image
"Rope"`, `set the source of image "Board"`, dragging one in edit mode,
clicking one to run its script, undo. None of it is written anywhere,
because a part already does it. The parser needed one word and its plural.

The bytes belong to the stack rather than the part, in an ordered library
saved as real files in the bundle's `images/`. A `.png` opens in an image
viewer; an `.svg` diffs like the text it is.

Two locks on the door, because a picture ends up in a web view and SVG is a
document that can carry script: the renderer only ever draws through
`<img>`, where a browser runs no script and fetches nothing, and the model
refuses bytes that are not the format the name claims — including on the
way *in* from JSON, which is the input that did not come from this program.

[Cluedo](../examples) is the example: a drawn board with transparent
buttons over its rooms, and portraits that are themselves the buttons.

## The map ✅

Not a phase — a thing that fell out of one. Once the parser is in the same
process as the stack, every `go` in every script can be read back without
running it, and a stack stops being a pile of cards and becomes a graph you
can look at.

[`crates/graph`](../crates/graph) does the reading, **Go ▸ Map** draws it,
and `hyperlab-graph` writes it as Graphviz, as JSON, or as a report that
exits non-zero. The three things it finds are the ones a stack cannot tell
you itself: cards nothing leads to, cards with no way out, and links naming a
card that is not there.

Nothing is run, so nothing is guessed. `go to next card` is certain once you
know which card you are standing on; `go to card whicheverOneTheyPicked` is
not, and says so. Prior art: [a graph of Myst][myst], drawn from the outside
with `stackimport` and a hand-written parser.

[myst]: https://glthr.com/myst-graph-1

## Getting out ✅

A stack that cannot leave HyperLab is a stack you cannot show anyone.

**File ▸ Export as PDF…** writes a page per card. The pictures go in as vector
graphics and the words as words, so a printed card is as sharp as the paper
and a reader can search it. **Go ▸ Map ▸ Save as PNG…** saves the drawing.

**File ▸ Export as a Web Page…** goes further and translates the stack into
[_hyperscript](https://hyperscript.org), which is what became of HyperTalk on
the web. The result is one HTML file that runs the stack — buttons, fields,
pictures, scripts and all — with nothing of HyperLab underneath it.

Still to come: one card rather than all of them, a page size that is a page
rather than the card, and a copy of _hyperscript in the file so an exported
page works with no network.

## Phase 7 — Plugins

Renderers, themes, providers, persistence formats, importers, exporters and
inspector panels are already behind traits or plain data. This phase is about
loading them at run time, and about a manifest that says what a plugin may do.

## Phase 8 — Collaborative stacks

Commands are already small, ordered and reversible, which is most of what a
collaborative editor needs. This phase turns the command bus into a log that
two people can share.

---

## Things we are not going to do

Worth writing down, because a roadmap that only says yes is not a plan.

- **A HyperCard file importer** is a maybe, not a plan: the format is
  undocumented, and the effort is better spent on the language.
- **Pixel-exact reproduction of HyperCard's look.** Neo Classic is inspired by
  the era, not copied from Apple, and it will keep evolving.
- **A cloud account.** HyperLab is local-first. Stacks are files.
- **A required AI provider.** Everything must work with none configured, for
  ever.
