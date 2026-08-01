/**
 * Makes a browser look like the Tauri window, to a page that cannot tell.
 *
 * `@tauri-apps/api` reaches the shell through exactly three things on
 * `window.__TAURI_INTERNALS__`. Supplying them, and forwarding `invoke` to
 * `hyperlab-bridge`, is the whole of it: `api.ts` is unchanged, the React is
 * unchanged, and every answer comes from a real `Runtime`.
 *
 * Events are polled rather than pushed. A script blocked on `ask` is sitting
 * in the bridge waiting for a reply, so the dialog has to reach the page
 * somehow; a hundred milliseconds is imperceptible and needs no socket.
 */

(() => {
  const BRIDGE = window.__HYPERLAB_BRIDGE__ ?? 'http://127.0.0.1:7878';

  /** Callbacks handed to the shell, by the id it knows them as. */
  const callbacks = new Map();
  let nextCallback = 1;

  /** Event handlers registered through `listen`, by event name. */
  const listeners = new Map();

  async function invoke(command, args) {
    // `listen` is answered here rather than at the bridge: the handler is a
    // function, which cannot cross the wire.
    if (command === 'plugin:event|listen') {
      // `args.handler` is already an id: `listen` passed the function
      // through `transformCallback` before calling us.
      const handlers = listeners.get(args.event) ?? [];
      handlers.push(args.handler);
      listeners.set(args.event, handlers);
      return handlers.length;
    }
    if (command === 'plugin:event|unlisten') return null;

    const response = await fetch(`${BRIDGE}/invoke/${command}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(args ?? {}),
    });
    const answer = await response.json();
    if ('error' in answer) throw new Error(answer.error);
    return answer.ok;
  }

  window.__TAURI_INTERNALS__ = {
    invoke,
    transformCallback(callback) {
      const id = nextCallback++;
      callbacks.set(id, callback);
      return id;
    },
    unregisterCallback(id) {
      callbacks.delete(id);
    },
    convertFileSrc: (path) => path,
  };

  // `unlisten` reaches for a second global of its own rather than going
  // through `invoke`, so a shim that provides only the first one throws the
  // moment a component unmounts.
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener(event, id) {
      listeners.set(
        event,
        (listeners.get(event) ?? []).filter((held) => held !== id),
      );
      callbacks.delete(id);
    },
  };

  // Dialogs a script is blocked on.
  setInterval(async () => {
    try {
      const response = await fetch(`${BRIDGE}/events`);
      const { ok: requests } = await response.json();
      for (const request of requests ?? []) {
        for (const id of listeners.get('hyperlab://dialog') ?? []) {
          callbacks.get(id)?.({ event: 'hyperlab://dialog', id, payload: request });
        }
      }
    } catch {
      // The bridge is not up yet, or has gone. The next tick will find out.
    }
  }, 100);
})();
