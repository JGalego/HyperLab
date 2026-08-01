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

## Phase 4 — AI sidebar

The interfaces exist ([`hyperlab-ai`](../crates/ai)), and two providers
implement them ([`hyperlab-ai-providers`](../crates/ai-providers)): a client
for the OpenAI chat-completions protocol — which also reaches OpenRouter,
Ollama and any local server that speaks it — and one for Anthropic's Messages
API. Writing the second one was the point: it is what keeps the interface
honest.

Still to come in this phase:

- A sidebar that can answer *explain this script*, *refactor this*, *add a
  search button*, *make this prettier*.
- It edits the stack through MCP tools, never by reaching into it — so
  everything it does is undoable, and shows up like anyone else's change.
- A visible, reviewable account of what is sent: the context builder already
  leaves field contents out unless asked.

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

The two differ in one way, and it is the one that matters: `ai(…)` answers in
words, and `ask assistant` may change the stack — through commands, so its
edits are undoable like anyone else's.

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
