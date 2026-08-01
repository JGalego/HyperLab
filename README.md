<p align="center">
  <img src="docs/banner.svg" alt="HyperLab: an open-source, AI-native successor to HyperCard" width="100%">
</p>

<p align="center">
  <a href="https://github.com/JGalego/HyperLab/actions/workflows/ci.yml"><img src="https://github.com/JGalego/HyperLab/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI status"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/licence-MIT-000000?style=flat-square" alt="Licence: MIT"></a>
  <img src="https://img.shields.io/badge/rust-1.85%2B-000000?style=flat-square&logo=rust&logoColor=white" alt="Rust 1.85+">
  <img src="https://img.shields.io/badge/tauri-2-000000?style=flat-square&logo=tauri&logoColor=white" alt="Tauri 2">
  <a href="CONTRIBUTING.md"><img src="https://img.shields.io/badge/PRs-welcome-000000?style=flat-square" alt="PRs welcome"></a>
</p>

Build a desktop application the way you would build a stack of index cards:
draw a card, put a button on it, write two lines of script, and you have
something that runs. HyperCard let a generation of people who did not call
themselves programmers build real software. HyperLab is an attempt at what
that idea looks like now.

It is not a clone. It keeps what made HyperCard work — cards, stacks, fields,
buttons, scripts, message passing, everything inspectable and everything
changeable while it runs — and adds language models as a first-class part of
the programming model rather than a panel bolted on the side.

![HyperLab: a card with a button and a field, the object inspector showing
the button's script](docs/screenshot.png)

---

## 📑 Contents

- [🧩 What It Looks Like to Use](#what-it-looks-like-to-use)
- [🚀 Getting Started](#getting-started)
- [🏗️ How It Is Put Together](#how-it-is-put-together)
- [📂 Stacks Are Files You Can Read](#stacks-are-files-you-can-read)
- [🤖 Where the AI Goes](#where-the-ai-goes)
- [🤝 Contributing](#contributing)
- [📄 Licence](#licence)

---

<h2 id="what-it-looks-like-to-use">🧩 What It Looks Like to Use</h2>

```hypertalk
on mouseUp
  ask "What needs doing?" with ""
  if it is not empty then
    put it & return after field "Items"
  end if
end mouseUp
```

That is the whole of the *Add* button in [the Todo example](examples). Click
it, and it runs. Change it, and the change takes effect immediately — there is
no build step inside a stack.

---

<h2 id="getting-started">🚀 Getting Started</h2>

You will need [Rust](https://rustup.rs) 1.85 or newer and Node 20 or newer,
plus [Tauri's system dependencies](https://tauri.app/start/prerequisites/) for
your platform.

Clone the repository:

```sh
git clone https://github.com/JGalego/hyperlab
```

```sh
cd hyperlab
```

Run the core — runtime, language, persistence. No GUI toolchain needed:

```sh
cargo test
```

Run the desktop application:

```sh
cd apps/desktop
```

```sh
npm install
```

```sh
npm run tauri dev
```

Then open one of the stacks in [`examples/`](examples) — *Todo* is the
smallest, *Address Book* shows what backgrounds are for, and *Recipe Box*
shows chunk expressions doing real work.

---

<h2 id="how-it-is-put-together">🏗️ How It Is Put Together</h2>

```
UI  →  Commands  →  Runtime  →  Model  →  Renderer
```

One rule holds the design up: **nothing changes a stack except a command.**
Not the interface, not a script, not an AI assistant. Because there is one
path in, undo, scripting, automation, testing and AI all work the same way,
and a change made by any of them is indistinguishable from a change made by
any other.

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
    mcp/            tools                 — what may be done

apps/desktop/       the Tauri shell and the React renderer
docs/               architecture, roadmap, HyperTalk reference
examples/           three stacks, which are also tests
```

`hyperlab-parser` depends on nothing at all, so the language can be tested and
reused on its own. The desktop shell contains no logic worth the name.

Read [`docs/architecture.md`](docs/architecture.md) for the reasoning, and
[`docs/hypertalk.md`](docs/hypertalk.md) for the language.

---

<h2 id="stacks-are-files-you-can-read">📂 Stacks Are Files You Can Read</h2>

A stack is a directory, not a blob:

```
Todo.hl/
    metadata.json
    stack.json
    backgrounds/2.json
    cards/3.json
    scripts/button-7.hypertalk
    assets/
```

One file per card, and scripts kept as plain HyperTalk rather than escaped
inside JSON — so a stack diffs, merges and greps like source code, because
that is what it is.

---

<h2 id="where-the-ai-goes">🤖 Where the AI Goes</h2>

Enforced by the way the code is arranged, not by good intentions:

- **No provider is special.** A provider implements
  [`AiProvider`](crates/ai/src/provider.rs) and nothing in HyperLab switches
  on which one it is. OpenAI, Anthropic, Google, Ollama, OpenRouter and local
  models are names in a settings file, not branches in the runtime. The
  clients live in [their own crate](crates/ai-providers), which the rest of
  HyperLab does not depend on; one of them speaks the OpenAI chat-completions
  protocol, so pointing a `baseUrl` at Ollama or a local server is all it
  takes to run with no network at all. A key is read from an environment
  variable you name, and is never written to a settings file.
- **An assistant can do exactly what you can do.** It works through
  [MCP tools](crates/mcp) that wrap runtime commands, so everything it does
  is undoable and visible. There is no private back door into a stack.
- **Nothing leaves without a reason.** HyperLab is local-first, works with no
  provider configured, and the [context builder](crates/ai/src/context.rs)
  omits your field contents unless a question actually needs them.

---

<h2 id="contributing">🤝 Contributing</h2>

Please do — see [CONTRIBUTING.md](CONTRIBUTING.md) and the
[Code of Conduct](CODE_OF_CONDUCT.md). Good places to start are listed in the
roadmap: a debugger, the AI sidebar, `find` and `sort`, a second theme.

<h2 id="licence">📄 Licence</h2>

[MIT](LICENSE).

HyperLab is an original work inspired by HyperCard. It contains no Apple code
and no Apple artwork, and the Neo Classic theme is drawn from scratch in the
spirit of the era.
