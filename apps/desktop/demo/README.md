# Filming HyperLab

```sh
GROQ_API_KEY=… apps/desktop/demo/record.sh
```

Leaves `target/demo/hyperlab.mp4` and `target/demo/hyperlab.gif`.

Without a key it films everything except the assistant, and says so.

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
| `film.mjs` | the Playwright script — what gets done, and in what order |
| `shim.js` | makes a browser look like the Tauri window |
| `cursor.js` | draws the pointer, which Playwright records but does not paint |
| `record.sh` | starts the two servers, films, converts, stops them |

`hyperlab-bridge` lives in
[`src-tauri/src/bin`](../src-tauri/src/bin/hyperlab-bridge.rs). It binds to
loopback, has no authentication, and carries only the commands the film uses.
It is a development tool you start on purpose, not a way to run HyperLab.

## Changing the film

`film.mjs` reads top to bottom in the order things happen. `say()` puts a
caption up, `press()` moves the pointer before clicking so the cut is
followable, and `write()` types rather than pasting.

The model is set through the same settings the window writes, so it is an
`openAiCompatible` provider pointed at Groq — no special case anywhere:

```js
{ kind: 'openAiCompatible', model: 'llama-3.3-70b-versatile',
  baseUrl: 'https://api.groq.com/openai/v1', apiKeyEnv: 'GROQ_API_KEY' }
```

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
