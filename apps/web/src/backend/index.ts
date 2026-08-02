/**
 * The page's side of the runtime.
 *
 * Two arrangements, one interface. Where the page is cross-origin isolated —
 * the dev server sends the headers; on GitHub Pages `coi-serviceworker.js`
 * arranges them — the runtime lives in a worker, and a script's dialog
 * blocks that worker while the page shows the real, HyperCard-shaped one.
 * Where isolation is not available, the runtime runs on this thread and
 * dialogs fall back to the browser's own `prompt` and `alert`: everything
 * works, it just isn't dressed for the part.
 */

import type { DialogRequest } from '../../../desktop/src/types';

import wasmUrl from '../../wasm-pkg/hyperlab_web_bg.wasm?url';

import { complete, completeSync } from './transport';

/** Which arrangement [`startBackend`] settled on. */
export type BackendMode = 'worker' | 'fallback';

/** What the api module needs from either arrangement. */
export interface Backend {
  mode: BackendMode;
  /** Runs one command. `bytes` rides along for picture imports only. */
  call<T>(command: string, args?: unknown, bytes?: Uint8Array): Promise<T>;
  /** Watches for dialogs a running script wants shown. */
  onDialog(handler: (request: DialogRequest) => void): () => void;
  /** Answers the dialog a script is waiting on. `null` means cancelled.
   * Returns whether anything was waiting. */
  dialogReply(text: string | null): boolean;
}

/** Only this app's keys travel to the worker's storage mirror. */
const STORAGE_PREFIX = 'hyperlab.';

/** The dialog reply buffer: two `Int32` flags, then up to 64 KiB of UTF-8. */
const REPLY_CAPACITY = 8 + 64 * 1024;

// -------------------------------------------------------------------- beep

let audio: AudioContext | null = null;

/** A short square-wave blip, in the spirit of the original. */
function beep(): void {
  try {
    audio ??= new AudioContext();
    if (audio.state === 'suspended') void audio.resume();
    const oscillator = audio.createOscillator();
    oscillator.type = 'square';
    oscillator.frequency.value = 830;
    const gain = audio.createGain();
    gain.gain.value = 0.06;
    oscillator.connect(gain);
    gain.connect(audio.destination);
    oscillator.start();
    oscillator.stop(audio.currentTime + 0.12);
  } catch {
    // A page that cannot make a sound still works; `beep` is best-effort on
    // every platform HyperLab runs on.
  }
}

// ------------------------------------------------------------------ worker

function snapshotStorage(): Record<string, string> {
  const found: Record<string, string> = {};
  for (let index = 0; index < localStorage.length; index += 1) {
    const key = localStorage.key(index);
    if (key !== null && key.startsWith(STORAGE_PREFIX)) {
      const value = localStorage.getItem(key);
      if (value !== null) found[key] = value;
    }
  }
  return found;
}

function persist(key: string, value: string | null): void {
  if (!key.startsWith(STORAGE_PREFIX)) return;
  if (value === null) localStorage.removeItem(key);
  else localStorage.setItem(key, value);
}

interface WorkerReply {
  type: 'ready' | 'ready-error' | 'result' | 'dialog' | 'beep' | 'storage';
  id?: number;
  ok?: boolean;
  value?: string;
  error?: string;
  request?: DialogRequest;
  key?: string;
}

function startWorkerBackend(): Promise<Backend> {
  const worker = new Worker(new URL('./worker.ts', import.meta.url), {
    type: 'module',
  });
  const shared = new SharedArrayBuffer(REPLY_CAPACITY);
  const flag = new Int32Array(shared, 0, 2);
  const replyBytes = new Uint8Array(shared, 8);
  const encoder = new TextEncoder();

  const pending = new Map<
    number,
    { resolve: (value: unknown) => void; reject: (reason: Error) => void }
  >();
  let nextId = 1;
  let handler: ((request: DialogRequest) => void) | null = null;
  let waiting = false;

  const dialogReply = (text: string | null): boolean => {
    if (!waiting) return false;
    waiting = false;
    if (text === null) {
      Atomics.store(flag, 0, 2);
    } else {
      const encoded = encoder.encode(text);
      const length = Math.min(encoded.length, replyBytes.length);
      replyBytes.set(encoded.subarray(0, length));
      Atomics.store(flag, 1, length);
      Atomics.store(flag, 0, 1);
    }
    Atomics.notify(flag, 0);
    return true;
  };

  return new Promise((resolveBackend, rejectBackend) => {
    worker.onmessage = (event: MessageEvent<WorkerReply>) => {
      const message = event.data;
      switch (message.type) {
        case 'ready':
          resolveBackend({
            mode: 'worker',
            call<T>(command: string, args?: unknown, bytes?: Uint8Array): Promise<T> {
              return new Promise<T>((resolve, reject) => {
                const id = nextId;
                nextId += 1;
                pending.set(id, {
                  resolve: resolve as (value: unknown) => void,
                  reject,
                });
                const call = {
                  type: 'call',
                  id,
                  command,
                  args: JSON.stringify(args ?? {}),
                  ...(bytes === undefined ? {} : { bytes }),
                };
                if (bytes === undefined) worker.postMessage(call);
                else worker.postMessage(call, [bytes.buffer]);
              });
            },
            onDialog(wanted) {
              handler = wanted;
              return () => {
                if (handler === wanted) handler = null;
              };
            },
            dialogReply,
          });
          break;
        case 'ready-error':
          rejectBackend(new Error(message.error ?? 'the runtime failed to start'));
          break;
        case 'result': {
          const settle = message.id === undefined ? undefined : pending.get(message.id);
          if (settle === undefined || message.id === undefined) break;
          pending.delete(message.id);
          if (message.ok === true) {
            settle.resolve(
              message.value === undefined ? undefined : JSON.parse(message.value),
            );
          } else {
            settle.reject(new Error(message.error ?? 'the command failed'));
          }
          break;
        }
        case 'dialog':
          waiting = true;
          if (handler !== null && message.request !== undefined) {
            handler(message.request);
          } else {
            // Nothing is listening — an `openStack` handler asked before the
            // window mounted. Cancelling is what a host that cannot ask
            // reports; leaving the script parked would be worse.
            dialogReply(null);
          }
          break;
        case 'beep':
          beep();
          break;
        case 'storage':
          if (message.key !== undefined) persist(message.key, message.value ?? null);
          break;
        default:
          break;
      }
    };
    worker.postMessage({
      type: 'init',
      sab: shared,
      storage: snapshotStorage(),
    });
  });
}

// ---------------------------------------------------------------- fallback

async function startFallbackBackend(): Promise<Backend> {
  const module = await import('../../wasm-pkg/hyperlab_web');
  await module.default({ module_or_path: wasmUrl });
  module.init({
    answer: (message: string) => window.alert(message),
    ask: (prompt: string, defaultText: string) => window.prompt(prompt, defaultText),
    beep,
    completeSync,
    complete,
    storageGet: (key: string) => localStorage.getItem(key),
    storageSet: persist,
  });

  const commands = module as unknown as Record<string, (args: string) => string>;
  return {
    mode: 'fallback',
    async call<T>(command: string, args?: unknown, bytes?: Uint8Array): Promise<T> {
      const argsJson = JSON.stringify(args ?? {});
      let value: string | undefined;
      if (command === 'import_image_bytes') {
        value = module.import_image_bytes(argsJson, bytes ?? new Uint8Array());
      } else if (command === 'ai_ask') {
        value = await module.ai_ask(argsJson);
      } else {
        const run = commands[command];
        if (typeof run !== 'function') {
          throw new Error(`the runtime has no command called "${command}"`);
        }
        value = run(argsJson);
      }
      return (value === undefined ? undefined : JSON.parse(value)) as T;
    },
    onDialog() {
      // Dialogs are the browser's own in this arrangement, shown and
      // answered inside the host's `ask`/`answer` before the call returns.
      return () => {};
    },
    dialogReply() {
      return false;
    },
  };
}

// ------------------------------------------------------------------- entry

/** Starts the runtime, in the best arrangement this browser offers. */
export function startBackend(): Promise<Backend> {
  const isolated =
    typeof SharedArrayBuffer !== 'undefined' &&
    typeof window !== 'undefined' &&
    window.crossOriginIsolated === true;
  return isolated ? startWorkerBackend() : startFallbackBackend();
}
