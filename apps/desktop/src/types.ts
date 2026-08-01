/**
 * The shapes the runtime sends to the renderer.
 *
 * These mirror the Rust types in `src-tauri/src/view.rs`. They are the only
 * description of a stack the interface ever sees: there is no client-side
 * model, no store holding a second copy, and nothing to keep in step.
 */

/** What kind of object something is. */
export type ObjectKind = 'stack' | 'background' | 'card' | 'button' | 'field';

/** Whether a part belongs to one card or to a shared background. */
export type Layer = 'card' | 'background';

/** A button or a field, ready to draw. */
export interface PartView {
  id: number;
  kind: 'button' | 'field';
  layer: Layer;
  name: string;
  text: string;
  /** left, top, width, height, in card space. */
  rect: [number, number, number, number];
  visible: boolean;
  enabled: boolean;
  style: string;
  locked: boolean;
  script: string;
  properties: PropertyView[];
}

/** One row of the property editor. */
export interface PropertyView {
  name: string;
  value: string | number | boolean | null;
  readOnly: boolean;
}

/** A card or a background, with everything on it. */
export interface CardView {
  id: number;
  kind: 'card' | 'background';
  name: string;
  script: string;
  parts: PartView[];
}

/** Everything the window needs to draw itself. */
export interface StackView {
  stackName: string;
  stackId: number;
  stackScript: string;
  cardSize: { width: number; height: number };
  cardCount: number;
  cardNumber: number;
  card: CardView;
  background: CardView | null;
  messageBox: string;
  /** What undo would do, or null if there is nothing to undo. */
  undo: string | null;
  redo: string | null;
  dirty: boolean;
  path: string | null;
}

/**
 * A modal dialog the runtime is waiting on.
 *
 * Unlike an {@link Effect}, this arrives *while* a script is running: the
 * script is blocked until the answer goes back.
 */
export type DialogRequest =
  { kind: 'answer'; message: string } | { kind: 'ask'; prompt: string; default: string };

/**
 * Something a script asked the world to do, collected while it ran.
 *
 * `answer` and `ask` appear here too, for callers with no window — tests,
 * and the MCP tools. The desktop shows them as they happen instead, through
 * {@link DialogRequest}, so this list is not replayed on screen.
 */
export type Effect =
  | { kind: 'answer'; message: string }
  | { kind: 'ask'; prompt: string; default: string }
  | { kind: 'beep' }
  | { kind: 'wait'; ticks: number }
  | { kind: 'navigated'; card: number }
  | { kind: 'messageBox'; text: string }
  | { kind: 'assistant'; prompt: string; intent: 'answer' | 'edit' };

/** What every command gives back. */
export interface Outcome {
  view: StackView;
  effects: Effect[];
}

/** What the inspector is looking at. */
export interface Selection {
  kind: ObjectKind;
  id: number;
}

/** Whether clicking runs a script or picks an object up. */
export type Tool = 'browse' | 'edit';

/** Exactly what was sent to a model with a question. */
export interface Briefing {
  context: string;
  includedFieldText: boolean;
  includedScripts: boolean;
}

/** One thing that happened in a conversation, as the user sees it. */
export type AiEntry =
  | { kind: 'question'; text: string; briefing: Briefing }
  | { kind: 'answer'; text: string }
  | { kind: 'used'; tool: string; arguments: string; allowed: boolean; outcome: string }
  | { kind: 'failed'; reason: string };

/** What the AI sidebar draws. */
export interface AiView {
  entries: AiEntry[];
  providers: string[];
  provider: string | null;
  problems: string[];
  sendsFieldText: boolean;
  mayEdit: boolean;
  busy: boolean;
}

/** One configured provider. Never holds a key — only the name of a variable. */
export interface ProviderConfig {
  kind: string;
  model: string;
  baseUrl?: string;
  apiKeyEnv?: string;
}

/** The provider settings, as they are stored. */
export interface AiSettings {
  defaultProvider?: string;
  providers: Record<string, ProviderConfig>;
}
