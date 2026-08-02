# Filming HyperLab

```sh
GROQ_API_KEY=… apps/desktop/demo/record.sh          # the tour
GROQ_API_KEY=… apps/desktop/demo/record.sh cluedo   # the game
apps/desktop/demo/record.sh myst                    # the island, and its map
```

Leaves `target/demo/<film>.mp4` (the whole thing) and `target/demo/<film>.gif`
(a highlight — the whole film at a legible size would be twenty megabytes,
and nobody scrolls past that). Move the cut with `GIF_FROM`, `GIF_FOR` and
`GIF_WIDTH`.

A key is only needed by the films with an assistant act. Without one they
shoot everything else and say so.

| Film | Stack | What it is for |
| --- | --- | --- |
| `film` | Recipe Box | cards, scripts, undo, the message box, the assistant |
| `cluedo` | Cluedo | pictures, and a model reading the game off the card |
| `myst` | Myst | what **Go ▸ Map** knows about a stack it never ran |
| `deck` | Language Models, Explained | a slide deck that asks a real model before its references |

## What is actually being filmed

The real thing. The scripts really run, the assistant really calls a model,
and its edits really go through the command bus — which is why the film ends
by undoing one.

The one piece of scaffolding is how the browser reaches the runtime.
HyperLab's window is a WebKit view owned by Tauri, and Playwright cannot
attach to one, so the film would otherwise have to be of a mock. Instead:

```
  Chromium ── shim.js ──HTTP──► hyperlab-bridge ──► Runtime
   (the real React, unchanged)                      (the real one)
```

`shim.js` supplies the three things `@tauri-apps/api` looks for on `window`
and forwards `invoke` over HTTP. Nothing in `src/` knows, because there is
nothing to know: same components, same `api.ts`, same snapshot type, same
`Runtime`. What you see is what the window would show.

| | |
| --- | --- |
| `kit.mjs` | the camera: launching, captions, a pointer that travels |
| `film.mjs` | the tour — what gets done, and in what order |
| `cluedo.mjs` | the game |
| `myst.mjs` | the island, and the map of it |
| `deck.mjs` | the slides, and the question at the end of them |
| `shim.js` | makes a browser look like the Tauri window |
| `cursor.js` | draws the pointer, which Playwright records but does not paint |
| `record.sh` | starts the two servers, films, converts, stops them |

`hyperlab-bridge` lives in
[`src-tauri/src/bin`](../src-tauri/src/bin/hyperlab-bridge.rs). It binds to
loopback, has no authentication, and carries only the commands a browser
needs to drive the interface. It is a development tool you start on purpose,
not a way to run HyperLab.

## Changing a film

Each film reads top to bottom in the order things happen. `say()` puts a
caption up, `press()` moves the pointer before clicking so the cut is
followable, and `write()` types rather than pasting.

The model is set through the same settings the window writes, so it is an
`openAiCompatible` provider pointed at Groq — no special case anywhere:

```js
{ kind: 'openAiCompatible', model: 'openai/gpt-oss-120b',
  baseUrl: 'https://api.groq.com/openai/v1',
  key: { in: 'environment', name: 'GROQ_API_KEY' } }
```

Pick a model that calls tools properly. Llama 3.3 on Groq, asked to write a
script, tends to emit `<function(create_button){…}>` as prose rather than
calling the tool, and the turn goes nowhere.

Override with `GROQ_MODEL` or `GROQ_BASE_URL`. Point `GROQ_BASE_URL` at
`http://localhost:11434/v1` and it films against Ollama instead.

## Needing Playwright

It is not in `package.json`. Filming is not something the application does,
and every `npm ci` in CI should not pay for a browser it never opens.

```sh
npm install -g playwright   # or: npm i -D playwright, here
```

Conversion needs a real `ffmpeg`; the one Playwright bundles can only write
the WebM it recorded.
