/** The dialog a script's `answer` and `ask` put on the screen. */

import { useEffect, useRef, useState } from 'react';

import type { DialogRequest } from '../types';

interface Props {
  request: DialogRequest;
  /** `null` means the person cancelled. */
  onReply: (text: string | null) => void;
}

/**
 * A script is waiting on this: it stopped mid-handler, and the next line will
 * not run until a reply goes back. `ask` puts the reply into `it`; cancelling
 * leaves `it` empty and sets `the result` to "Cancel".
 */
export function Dialog({ request, onReply }: Props) {
  const [draft, setDraft] = useState(request.kind === 'ask' ? request.default : '');
  const field = useRef<HTMLInputElement>(null);
  const confirm = useRef<HTMLButtonElement>(null);

  // A modal that does not take the keyboard makes people reach for the mouse.
  useEffect(() => {
    (field.current ?? confirm.current)?.focus();
    field.current?.select();
  }, []);

  const message = request.kind === 'answer' ? request.message : request.prompt;
  const reply = () => onReply(request.kind === 'ask' ? draft : '');

  return (
    <div className="dialog__scrim" role="presentation">
      <div
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-label={message}
        onKeyDown={(event) => {
          if (event.key === 'Enter') reply();
          if (event.key === 'Escape') onReply(null);
        }}
      >
        <p className="dialog__message">{message}</p>

        {request.kind === 'ask' && (
          <input
            ref={field}
            className="dialog__field"
            value={draft}
            spellCheck={false}
            onChange={(event) => setDraft(event.target.value)}
          />
        )}

        <div className="dialog__buttons">
          {request.kind === 'ask' && (
            <button type="button" className="tool" onClick={() => onReply(null)}>
              Cancel
            </button>
          )}
          <button ref={confirm} type="button" className="tool" onClick={reply}>
            OK
          </button>
        </div>
      </div>
    </div>
  );
}
