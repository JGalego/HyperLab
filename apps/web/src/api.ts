/**
 * The only way the interface talks to the runtime — the browser edition.
 *
 * Vite swaps this module in for the desktop's `api.ts`, so the functions
 * here keep that file's names and shapes exactly: a component written
 * against the desktop cannot tell the difference. What differs is the far
 * side — a WebAssembly module in a worker instead of a Tauri shell — and
 * the file-shaped commands, which trade paths for text because a page has
 * no file system.
 */

import type {
  AiSettings,
  AiView,
  DialogRequest,
  Graph,
  KeychainView,
  Layer,
  ObjectKind,
  Outcome,
  PropertyView,
  StackView,
} from '../../desktop/src/types';

import { startBackend, type Backend, type BackendMode } from './backend';

/** Started once, awaited by every call. */
let backend: Promise<Backend> | null = null;

function runtime(): Promise<Backend> {
  backend ??= startBackend();
  return backend;
}

/** Resolves when the runtime is up, for the window's first render. */
export async function ready(): Promise<void> {
  await runtime();
}

/** Which arrangement the runtime settled on: `worker` blocks scripts behind
 * real dialogs; `fallback` (no cross-origin isolation) uses the browser's
 * own prompt and alert. */
export async function backendMode(): Promise<BackendMode> {
  return (await runtime()).mode;
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return (await runtime()).call<T>(command, args);
}

/**
 * Watches for dialogs a running script wants shown.
 *
 * Resolves to a function that stops watching. The script stays blocked until
 * {@link dialogReply} is called, so a handler that never replies leaves it
 * waiting.
 */
export async function onDialog(
  handler: (request: DialogRequest) => void,
): Promise<() => void> {
  return (await runtime()).onDialog(handler);
}

/** Answers the dialog a script is waiting on. `null` means cancelled. */
export const dialogReply = async (text: string | null): Promise<boolean> =>
  (await runtime()).dialogReply(text);

/** Takes a fresh snapshot without changing anything. */
export const getView = (): Promise<Outcome> => call('get_view');

/** Every property of one object, for the inspector. */
export const getProperties = (kind: ObjectKind, id: number): Promise<PropertyView[]> =>
  call('get_properties', { kind, id });

/** Checks whether a script parses. Resolves if it does. */
export const checkScript = async (source: string): Promise<void> => {
  await call('check_script', { source });
};

/** Reads the stack as the routes between its cards. */
export const stackGraph = (): Promise<Graph> => call('stack_graph');

/**
 * One of the stack's pictures, as a `data:` URI.
 *
 * Asked for by name rather than sent with every snapshot: a snapshot is
 * taken after every command, and a card of artwork would be re-encoded on
 * every keystroke.
 */
export const stackImage = (name: string): Promise<string> =>
  call('stack_image', { name });

/** The names of every picture the stack carries. */
export const stackImages = (): Promise<string[]> => call('stack_images');

/** Brings a picture the user picked into the stack and onto the card. */
export const importImageBytes = async (
  name: string,
  layer: Layer,
  bytes: Uint8Array,
): Promise<Outcome> =>
  (await runtime()).call('import_image_bytes', { name, layer }, bytes);

/** Sends `mouseUp` to a part, exactly as clicking it does. */
export const clickPart = (id: number): Promise<Outcome> => call('click_part', { id });

/** Types into a field. */
export const setFieldText = (id: number, text: string): Promise<Outcome> =>
  call('set_field_text', { id, text });

/** Goes to a card by position, counting from one. */
export const goToCard = (position: number): Promise<Outcome> =>
  call('go_to_card', { position });

/** Runs whatever is in the message box. */
export const runMessageBox = (source: string): Promise<Outcome> =>
  call('run_message_box', { source });

/** Adds a card after the current one. */
export const newCard = (): Promise<Outcome> => call('new_card');

/** Deletes the current card. */
export const deleteCard = (): Promise<Outcome> => call('delete_card');

/** Adds a part. */
export const newPart = (
  kind: 'button' | 'field' | 'image',
  layer: Layer,
  name?: string,
): Promise<Outcome> => call('new_part', { kind, layer, name: name ?? null });

/** Removes a part. */
export const deletePart = (id: number): Promise<Outcome> => call('delete_part', { id });

/** Moves or resizes a part. */
export const setGeometry = (
  id: number,
  left: number,
  top: number,
  width: number,
  height: number,
): Promise<Outcome> => call('set_geometry', { id, left, top, width, height });

/** Sets a property from the inspector. */
export const setProperty = (
  kind: ObjectKind,
  id: number,
  property: string,
  value: string | number | boolean | null,
): Promise<Outcome> => call('set_property', { kind, id, property, value });

/** Replaces an object's script. */
export const setScript = (
  kind: ObjectKind,
  id: number,
  script: string,
): Promise<Outcome> => call('set_script', { kind, id, script });

/** Renames an object. */
export const rename = (kind: ObjectKind, id: number, name: string): Promise<Outcome> =>
  call('rename', { kind, id, name });

/** Resizes every card in the stack. */
export const setStackSize = (width: number, height: number): Promise<Outcome> =>
  call('set_stack_size', { width, height });

/** Undoes the last change. */
export const undo = (): Promise<Outcome> => call('undo');

/** Redoes the last undone change. */
export const redo = (): Promise<Outcome> => call('redo');

/** Starts a new, empty stack. */
export const newStack = (name?: string): Promise<Outcome> =>
  call('new_stack', { name: name ?? null });

/** Opens a stack from single-file JSON — an upload, or a bundled example. */
export const openStackText = (text: string): Promise<Outcome> =>
  call('open_stack_json', { text });

/**
 * The stack as single-file JSON, for the page to offer as a download. The
 * copy the user takes *is* the save, so the document is clean afterwards.
 */
export const saveStackText = (): Promise<{ name: string; text: string }> =>
  call('save_stack_json');

/** The stack as one HTML page driven by _hyperscript, plus a line for
 * everything that had no equivalent there. */
export const exportWebPage = (): Promise<{ source: string; notes: string[] }> =>
  call('export_web');

/**
 * Offers a typeface for the words drawn inside pictures.
 *
 * A browser has no system fonts to find, so without one a picture's labels
 * are missing from a PDF or a deck. The page fetches a font the first time
 * either export is asked for; see {@link withFont} in `App.tsx`.
 */
export const addFont = async (bytes: Uint8Array): Promise<void> => {
  await (await runtime()).call('add_font', {}, bytes);
};

/** The whole stack as a PDF, one page per card, as the bytes themselves. */
export const exportPdf = async (): Promise<Uint8Array> =>
  (await runtime()).call<Uint8Array>('export_pdf');

/** The stack as a Decker deck, plus a line for everything Lil and a deck
 * have no equivalent for. */
export const exportDeck = (): Promise<{ source: string; notes: string[] }> =>
  call('export_deck');

/** An empty snapshot, for the moment before the first one arrives. */
export const emptyView: StackView = {
  stackName: '',
  stackId: 0,
  stackScript: '',
  cardSize: { width: 512, height: 342 },
  cardCount: 0,
  cardNumber: 0,
  card: { id: 0, kind: 'card', name: '', script: '', parts: [] },
  background: null,
  messageBox: '',
  undo: null,
  redo: null,
  dirty: false,
  path: null,
};

// ------------------------------------------------------------------- the AI

/** What the assistant sidebar should draw. */
export const aiView = (): Promise<AiView> => call('ai_view');

/** Asks the assistant something. Slow: it goes to a model and back. */
export const aiAsk = (question: string): Promise<Outcome> => call('ai_ask', { question });

/** Forgets the conversation. */
export const aiClear = (): Promise<AiView> => call('ai_clear');

/** Chooses whether the contents of fields are sent with a question. */
export const aiSetSendsFieldText = (sending: boolean): Promise<AiView> =>
  call('ai_set_sends_field_text', { sending });

/** Chooses whether the assistant may change the stack. */
export const aiSetMayEdit = (editing: boolean): Promise<AiView> =>
  call('ai_set_may_edit', { editing });

/** The provider settings. */
export const aiSettings = (): Promise<AiSettings> => call('ai_settings');

/** Saves provider settings to this browser and rebuilds the providers. */
export const aiSaveSettings = (settings: AiSettings): Promise<AiView> =>
  call('ai_save_settings', { settings });

/** Which providers have a key saved in this browser. */
export const aiKeychain = (): Promise<KeychainView> => call('ai_keychain');

/**
 * Saves a provider's key in this browser's storage.
 *
 * The key goes one way. What comes back says which providers have one, and
 * there is no call anywhere that reads a key out again. It is sent only to
 * the provider it belongs to, straight from this browser — the server this
 * page came from is static files and never sees it.
 */
export const aiSetKey = (provider: string, key: string): Promise<KeychainView> =>
  call('ai_set_key', { provider, key });

/** Removes a provider's key from this browser. */
export const aiForgetKey = (provider: string): Promise<KeychainView> =>
  call('ai_forget_key', { provider });
