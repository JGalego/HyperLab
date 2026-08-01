# HyperLab

**An open-source, AI-native successor to HyperCard.**

Build a desktop application the way you would build a stack of index cards:
draw a card, put a button on it, write two lines of script, and you have
something that runs. HyperCard let a generation of people who did not call
themselves programmers build real software. HyperLab is an attempt at what
that idea looks like now.

It is not a clone. It keeps what made HyperCard work — cards, stacks, fields,
buttons, scripts, message passing, everything inspectable and everything
changeable while it runs — and adds language models as a first-class part of
the programming model rather than a panel bolted on the side.

> **Status: 0.1, and honest about it.** The runtime, the language and the
> editor work and are covered by 240-odd tests. The AI layer is interfaces and
> a mock: no provider ships yet. See the [roadmap](docs/roadmap.md).

---

## What it looks like to use

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

## Getting started

You will need [Rust](https://rustup.rs) 1.85 or newer and Node 20 or newer,
plus [Tauri's system dependencies](https://tauri.app/start/prerequisites/) for
your platform.

```sh
git clone https://github.com/JGalego/hyperlab
cd hyperlab

# The core: runtime, language, persistence. No GUI toolchain needed.
cargo test

# The application.
cd apps/desktop
npm install
npm run tauri dev
```

Then open one of the stacks in [`examples/`](examples) — *Todo* is the
smallest, *Address Book* shows what backgrounds are for, and *Recipe Box*
shows chunk expressions doing real work.

---

## How it is put together

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

## Stacks are files you can read

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

## Where the AI goes

Three commitments, enforced by the way the code is arranged rather than by
good intentions:

1. **No provider is special.** A provider implements
   [`AiProvider`](crates/ai/src/provider.rs) and nothing in HyperLab switches
   on which one it is. OpenAI, Anthropic, Google, Ollama, OpenRouter and local
   models are names in a settings file, not branches in the runtime.
2. **An assistant can do exactly what you can do.** It works through
   [MCP tools](crates/mcp) that wrap runtime commands, so everything it does
   is undoable and visible. There is no private back door into a stack.
3. **Nothing leaves without a reason.** HyperLab is local-first, works with no
   provider configured, and the [context builder](crates/ai/src/context.rs)
   omits your field contents unless a question actually needs them.

---

## Contributing

Please do — see [CONTRIBUTING.md](CONTRIBUTING.md) and the
[Code of Conduct](CODE_OF_CONDUCT.md). Good places to start are listed in the
roadmap: a debugger, a real provider, `find` and `sort`, a second theme.

## Licence

[MIT](LICENSE).

HyperLab is an original work inspired by HyperCard. It contains no Apple code
and no Apple artwork, and the Neo Classic theme is drawn from scratch in the
spirit of the era.
