/**
 * The worker the runtime lives in.
 *
 * The desktop runs a script on a blocking thread so the window can stay
 * responsive while the script waits on a dialog. A page gets the same
 * arrangement from a Web Worker: the WebAssembly runtime runs here, and when
 * a script calls `ask`, this thread parks on `Atomics.wait` while the page —
 * still live — shows the dialog and writes the answer into shared memory.
 *
 * Everything else is a plain message pump: the page posts
 * `{type: 'call', command, args}` and gets `{type: 'result'}` back, one
 * message per command, in order. A command that shows a dialog holds the
 * pump — which is exactly the desktop's "the session is locked while a
 * script waits" behaviour, arrived at by different plumbing.
 */

import init, * as wasm from '../../wasm-pkg/hyperlab_web';
import wasmUrl from '../../wasm-pkg/hyperlab_web_bg.wasm?url';

import { complete, completeSync } from './transport';

/** How long a script waits for an answer before giving up, matching the
 * desktop's patience: generous, because a person may reasonably take a
 * while, and finite, so an unanswerable dialog cannot park the runtime for
 * ever. */
const PATIENCE_MILLISECONDS = 10 * 60 * 1000;

/** Shared with the page: [0] the state flag, [1] the reply's byte length. */
let flag: Int32Array | null = null;
/** The reply's bytes, UTF-8, after the two flags. */
let replyBytes: Uint8Array | null = null;

/**
 * The worker's copy of the page's storage, kept because a worker cannot
 * touch `localStorage`. Reads are answered from here; writes go through to
 * the page, which persists them. Only this app writes these keys, so the
 * copy cannot go stale.
 */
const storage = new Map<string, string>();

const decoder = new TextDecoder();

/** Parks this thread until the page answers the dialog, or gives up. */
function blockOnDialog(request: unknown): string | null {
  if (flag === null || replyBytes === null) return null;
  Atomics.store(flag, 0, 0);
  postMessage({ type: 'dialog', request });
  const outcome = Atomics.wait(flag, 0, 0, PATIENCE_MILLISECONDS);
  if (outcome === 'timed-out') return null;
  if (Atomics.load(flag, 0) !== 1) return null;
  const length = Math.min(Atomics.load(flag, 1), replyBytes.length);
  // Copied out before decoding: TextDecoder refuses views over shared
  // memory.
  const copy = new Uint8Array(length);
  copy.set(replyBytes.subarray(0, length));
  return decoder.decode(copy);
}

/** The host object the WebAssembly module calls back into. Its shape is a
 * contract with `crates/web/src/api.rs`. */
const host = {
  answer(message: string): void {
    blockOnDialog({ kind: 'answer', message });
  },
  ask(prompt: string, defaultText: string): string | null {
    return blockOnDialog({ kind: 'ask', prompt, default: defaultText });
  },
  beep(): void {
    postMessage({ type: 'beep' });
  },
  completeSync,
  complete,
  storageGet(key: string): string | null {
    return storage.get(key) ?? null;
  },
  storageSet(key: string, value: string | null): void {
    if (value === null) storage.delete(key);
    else storage.set(key, value);
    postMessage({ type: 'storage', key, value });
  },
};

/** Resolves once the WebAssembly module is up; calls queue behind it. */
let started: Promise<void> | null = null;

interface InitMessage {
  type: 'init';
  sab: SharedArrayBuffer | null;
  storage: Record<string, string>;
}

interface CallMessage {
  type: 'call';
  id: number;
  command: string;
  args: string;
  bytes?: Uint8Array;
}

function start(message: InitMessage): Promise<void> {
  if (message.sab !== null) {
    flag = new Int32Array(message.sab, 0, 2);
    replyBytes = new Uint8Array(message.sab, 8);
  }
  for (const [key, value] of Object.entries(message.storage)) {
    storage.set(key, value);
  }
  return init({ module_or_path: wasmUrl }).then(() => {
    wasm.init(host);
  });
}

/** Every command the module exports, called by name. The page's api module
 * is the one place names originate, and the module itself is the contract:
 * an unknown name is answered with an error, not silence. */
type Command = (args: string) => string;

/** What a command answered with: JSON, or bytes for the ones that make a
 * file. A PDF is not text, and base64 through the JSON channel would cost a
 * third of the file for nothing. */
interface Answer {
  value?: string;
  bytes?: Uint8Array;
}

async function run(message: CallMessage): Promise<Answer> {
  const given = message.bytes ?? new Uint8Array();
  switch (message.command) {
    case 'import_image_bytes':
      return { value: wasm.import_image_bytes(message.args, given) };
    case 'ai_ask':
      return { value: await wasm.ai_ask(message.args) };
    default: {
      const commands = wasm as unknown as Record<string, Command | undefined>;
      const command = commands[message.command];
      if (typeof command !== 'function') {
        throw new Error(`the runtime has no command called "${message.command}"`);
      }
      return { value: command(message.args) };
    }
  }
}

self.onmessage = (event: MessageEvent<InitMessage | CallMessage>) => {
  const message = event.data;
  if (message.type === 'init') {
    started = start(message);
    started.then(
      () => postMessage({ type: 'ready' }),
      (reason: unknown) => postMessage({ type: 'ready-error', error: String(reason) }),
    );
    return;
  }

  void (async () => {
    try {
      await started;
      const { value, bytes } = await run(message);
      const result = { type: 'result', id: message.id, ok: true, value, bytes };
      // The buffer is handed over rather than copied; nothing here reads it
      // again.
      if (bytes === undefined) postMessage(result);
      else postMessage(result, [bytes.buffer]);
    } catch (reason) {
      postMessage({
        type: 'result',
        id: message.id,
        ok: false,
        error: String(reason),
      });
    }
  })();
};
