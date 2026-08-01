/**
 * The only way the interface talks to the runtime.
 *
 * Every function here is a thin, typed wrapper around one Tauri command. No
 * component calls `invoke` directly, so the list of things the interface can
 * do to a stack is exactly the list of functions in this file — and it is
 * short on purpose.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type {
  AiSettings,
  AiView,
  DialogRequest,
  Layer,
  ObjectKind,
  Outcome,
  PropertyView,
  StackView,
} from './types';

/** The event the shell emits when a script wants a dialog shown. */
const DIALOG_EVENT = 'hyperlab://dialog';

/**
 * Whether we are running inside the desktop shell.
 *
 * Opening the dev server in an ordinary browser is useful for looking at the
 * interface, but there is no runtime behind it, so the app says so rather
 * than failing one call at a time.
 */
export function inDesktopApp(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!inDesktopApp()) {
    throw new Error(
      'HyperLab is not running in the desktop shell. Start it with `npm run tauri dev`.',
    );
  }
  return invoke<T>(command, args);
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
  if (!inDesktopApp()) return () => {};
  return listen<DialogRequest>(DIALOG_EVENT, (event) => handler(event.payload));
}

/** Answers the dialog a script is waiting on. `null` means cancelled. */
export const dialogReply = (text: string | null): Promise<boolean> =>
  call('dialog_reply', { text });

/** Takes a fresh snapshot without changing anything. */
export const getView = (): Promise<Outcome> => call('get_view');

/** Every property of one object, for the inspector. */
export const getProperties = (kind: ObjectKind, id: number): Promise<PropertyView[]> =>
  call('get_properties', { kind, id });

/** Checks whether a script parses. Resolves if it does. */
export const checkScript = (source: string): Promise<void> =>
  call('check_script', { source });

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

/** Adds a button or a field. */
export const newPart = (
  kind: 'button' | 'field',
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

/** Opens a `.hl` bundle. */
export const openStack = (path: string): Promise<Outcome> => call('open_stack', { path });

/** Saves the stack, to `path` if given and to where it came from otherwise. */
export const saveStack = (path?: string): Promise<Outcome> =>
  call('save_stack', { path: path ?? null });

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

/** Saves provider settings and rebuilds the providers. */
export const aiSaveSettings = (settings: AiSettings): Promise<AiView> =>
  call('ai_save_settings', { settings });
