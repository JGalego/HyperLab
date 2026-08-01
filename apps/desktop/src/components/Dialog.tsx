/** The dialog a script's `answer` and `ask` put on the screen. */

import type { Effect } from '../types';

interface Props {
  effect: Extract<Effect, { kind: 'answer' } | { kind: 'ask' }>;
  onDismiss: () => void;
}

/**
 * Effects are replayed here after a handler has finished, rather than while
 * it runs. That is why the runtime never blocks and never re-enters itself.
 *
 * The consequence, for now, is that `ask` shows its question but cannot hand
 * the answer back: the script has already finished, and sees `it` empty with
 * `the result` set to "Cancel". Making the runtime able to suspend mid-handler
 * is Phase 2 work — see docs/roadmap.md.
 */
export function Dialog({ effect, onDismiss }: Props) {
  return (
    <div className="dialog__scrim" role="presentation">
      <div className="dialog" role="dialog" aria-modal="true">
        <p className="dialog__message">
          {effect.kind === 'answer' ? effect.message : effect.prompt}
        </p>
        {effect.kind === 'ask' && (
          <>
            <p className="script__ok">
              HyperLab cannot yet return this answer to the script.
            </p>
            <p className="dialog__message">{effect.default}</p>
          </>
        )}
        <div className="dialog__buttons">
          <button type="button" className="tool" onClick={onDismiss} autoFocus>
            OK
          </button>
        </div>
      </div>
    </div>
  );
}
