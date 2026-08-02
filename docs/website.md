# The website

> **What:** [jgalego.github.io/HyperLab](https://jgalego.github.io/HyperLab/) —
> a retro landing page telling HyperCard's story, and the full application
> running in the browser.
>
> **Where:** `apps/web`, over `crates/web`, deployed by
> `.github/workflows/pages.yml`.

The site is two pages of static files on GitHub Pages. There is no server,
which is not a limitation but the design: everything below follows from it.

## The playground is the application

The playground page renders the desktop's own React components — the card,
the inspector, the assistant sidebar, the map, the dialogs. It can, because
the desktop keeps its whole shell behind one module (`apps/desktop/src/api.ts`,
a typed wrapper per command), and the renderer never learns which shell
answers. Vite swaps that one module for `apps/web/src/api.ts`; nothing else
in the desktop tree is touched, so the two interfaces cannot drift apart —
they are the same files.

Behind the web `api.ts` sits `crates/web` (`hyperlab-web`): the same crates
the desktop wraps — runtime, parser, persistence, graph, the assistant —
compiled to WebAssembly, exporting one function per desktop command. Its
`view.rs` is a hand-kept twin of the desktop's, and a test serializes a
snapshot and checks the field names against what the renderer expects.

```
apps/desktop:  React renderer → api.ts  → Tauri commands → Runtime
apps/web:      React renderer → api.ts′ → Web Worker     → wasm  → Runtime
                    (the same files)
```

## How a script blocks on a dialog

HyperTalk's `ask` stops the script until the person answers, and the answer
lands in `it` on the very next line. The desktop does this by running scripts
on a blocking thread while the window stays live. A page gets the same
arrangement from a Web Worker:

```
script → Host::ask → worker JS → postMessage ───────────► page
            (parked on Atomics.wait)                        │ shows the
         ◄─────────── shared memory + Atomics.notify ◄──────┘ real dialog
```

The WebAssembly runtime lives in the worker. When a script asks, the worker
posts the request to the page and parks on `Atomics.wait`; the page — still
responsive — shows the same dialog component the desktop shows, and writes
the reply into a `SharedArrayBuffer`. The runtime never learns any of this:
it called `Host::ask` and got an answer, exactly as it does on the desktop.

`SharedArrayBuffer` needs cross-origin isolation, and GitHub Pages cannot
send the two headers that grant it. A vendored service worker
(`coi-serviceworker.js`, MIT) adds them, at the cost of one automatic reload
on first visit — the accepted arrangement for static hosts. Where isolation
is unavailable anyway, the runtime falls back to the page's own thread and
`window.prompt`: everything works, it just isn't dressed for the part.

## Files, without a file system

A stack travels as the single-file JSON `hyperlab-persistence` already
speaks: **File ▸ Open Stack…** is an upload, **File ▸ Download Stack** is a
download, and the same file opens in the desktop application. The six
examples are packed from `examples/` at build time by

```sh
cargo run -p hyperlab-persistence --example pack_single_files -- apps/web/public/examples
```

and fetched from the site itself; the landing page's gallery links open the
playground with `?stack=<name>`.

## Keys never leave the browser

The desktop keeps API keys in the operating system's keychain, one-way: the
settings panel can learn that a key is saved, never what it is. The web
keeps the same shape with the parts a page has:

- A key typed into **AI ▸ Settings** goes into this browser's storage under
  the provider's name. No command reads one back out.
- A request goes straight from the browser to the configured provider —
  `fetch` from the worker for the sidebar, a synchronous request for a
  script blocked mid-line on `ai("…")`. The wire protocol is the same Rust
  code the desktop uses (`hyperlab-ai-providers` built without its native
  clients); only the transport is the browser's.
- The site being static files is the guarantee's other half: there is no
  HyperLab server for a key, a stack, or a question to visit.

Anthropic requires the `anthropic-dangerous-direct-browser-access` header
before answering a browser at all. Sending it is honest here: the request
really does come from the user's own browser, with a key they typed into it,
which is the situation the header exists to make explicit. The mock provider
answers locally, so the whole AI surface can be tried with no key and no
network.

## Building it locally

```sh
cd apps/web
npm install
npm run prepare:assets   # wasm-pack build + pack the examples
npm run dev              # the dev server sends the isolation headers itself
```

Needs Rust with the `wasm32-unknown-unknown` target,
[wasm-pack](https://rustwasm.github.io/wasm-pack/), and Node 20.

## Deploying

`.github/workflows/pages.yml` builds and deploys on every push to `main`.
One-time repository setup: **Settings ▸ Pages ▸ Source** must say
**GitHub Actions**.
