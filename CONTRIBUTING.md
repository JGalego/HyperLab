# Contributing to HyperLab

Thank you for being here. HyperLab is meant to last, which means it is meant
to be worked on by people who did not write it — including you, and including
whoever is reading the code in thirty years.

## Getting set up

```sh
cargo test                      # the core crates: no GUI toolchain needed
cd apps/desktop && npm install
npm run tauri dev               # the application
```

The core crates deliberately do not depend on Tauri, so `cargo test` at the
repository root works on any machine. If you are only touching the runtime,
the parser or persistence, you never need the desktop toolchain.

## Before you open a pull request

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd apps/desktop
npm run typecheck && npm run lint && npm run format:check
```

CI runs exactly these, so if they pass locally they will pass there.

## What good looks like here

**Say why, not what.** The code says what it does. Comments are for the
decision behind it — why a chunk list is stored outermost-first, why the
parser refuses to decide whether `the length of x` is a property. If a comment
would only restate the line below it, delete it.

**Tests describe behaviour.** `a_message_travels_out_to_the_card_when_the_button_ignores_it`
tells you what broke before you have opened the file. `test_dispatch_2` does
not. Prefer a test that would fail for a reason a user would notice.

**Small modules, one idea each.** If a file has grown past a few hundred
lines, it is usually two files.

**Minimal dependencies.** Every crate you add is something a future maintainer
must understand, audit and keep. A type alias for `BoxFuture` is cheaper than
a futures crate; fifteen lines of calendar arithmetic are cheaper than a date
library. Adding a dependency is fine — adding one without weighing it is not.

**Do not route around the architecture.** In particular:

- The interface must not change a stack except through a `Command`.
- A command must return the command that reverses it.
- The parser must not know what a property is.
- The runtime must not know what a prompt looks like.

These are not style preferences; they are the reason undo, scripting,
automation and AI can share one implementation. If a change seems to need an
exception, that is worth discussing in an issue before it is worth writing.

## Where to start

The [roadmap](docs/roadmap.md) lists what is missing and where it belongs.
Self-contained places to begin:

- **A HyperTalk debugger** — step, breakpoints, and a look at the current
  frame. The interpreter already has frames and per-statement line numbers.
- **`find` and `sort`**, the two commands classic stacks miss most.
- **A second theme.** Every value in the Neo Classic theme is a custom
  property, and the dark variant is already a five-line override.
- **A third AI provider.** Adding one is a module and one arm of `build` in
  [`hyperlab-ai-providers`](crates/ai-providers); nothing else knows.
- **Streaming**, so an answer appears in the sidebar as it is written. This
  one is a new method on [`AiProvider`](crates/ai/src/provider.rs), so it is
  worth an issue first.

Adding a language feature usually means: a token or two, an AST node, a parser
branch, an interpreter case, tests at each layer, and a line in
[the reference](docs/hypertalk.md). Nothing else should need to change — and
if it does, say so in the pull request, because that is a design signal.

## Reporting things

A bug report with the stack that reproduces it — or, better, a failing test —
is worth ten without. Because a stack is a directory of plain files, you can
usually attach the one card that goes wrong.

## Licence

Contributions are made under the [MIT licence](LICENSE). Please do not
contribute code or artwork you do not have the right to license, and in
particular no assets from HyperCard or any other Apple software.

## Pinned versions, and why

- **`pdf-writer` is held at 0.12.** `svg2pdf` hands back its own
  `pdf-writer` types, so `hyperlab-export` has to use the same version
  svg2pdf does. A newer one compiles and then refuses to meet it. Move them
  together, and only when svg2pdf moves first.
- **TypeScript is held at 6.** `typescript-eslint` does not support 7 yet
  ([#10940](https://github.com/typescript-eslint/typescript-eslint/issues/10940)),
  and linting is worth more than a newer compiler. Move both together.
- **`quick-xml`** carries two denial-of-service advisories that cannot be
  fixed from here: `plist`, which Tauri uses, requires a version older than
  the fix. The reasoning is in
  [`apps/desktop/src-tauri/.cargo/audit.toml`](apps/desktop/src-tauri/.cargo/audit.toml),
  and `cargo audit` runs in CI so it cannot rot quietly.
