/** The bottom strip: navigation, the message box, and anything that failed. */

import { useEffect, useState } from 'react';

import type { StackView, Tool } from '../types';

interface Props {
  view: StackView;
  tool: Tool;
  error: string | null;
  onGoTo: (position: number) => void;
  onSetTool: (tool: Tool) => void;
  onRunMessage: (source: string) => void;
  onDismissError: () => void;
}

/**
 * The message box is the oldest idea in HyperCard's interface and still the
 * best one: a single line where you can type anything the language
 * understands and see what happens.
 */
export function StatusBar({
  view,
  tool,
  error,
  onGoTo,
  onSetTool,
  onRunMessage,
  onDismissError,
}: Props) {
  const [draft, setDraft] = useState(view.messageBox);

  useEffect(() => setDraft(view.messageBox), [view.messageBox]);

  return (
    <div className="statusbar">
      <div className="navigator">
        <button
          type="button"
          className="tool"
          aria-pressed={tool === 'browse'}
          onClick={() => onSetTool('browse')}
          title="Click buttons and type in fields"
        >
          Browse
        </button>
        <button
          type="button"
          className="tool"
          aria-pressed={tool === 'edit'}
          onClick={() => onSetTool('edit')}
          title="Move and select parts"
        >
          Edit
        </button>
      </div>

      <div className="navigator">
        <button
          type="button"
          className="tool"
          onClick={() => onGoTo(1)}
          title="First card"
        >
          |◀
        </button>
        <button
          type="button"
          className="tool"
          onClick={() => onGoTo(view.cardNumber - 1)}
          title="Previous card"
        >
          ◀
        </button>
        <span className="navigator__label">
          Card {view.cardNumber} of {view.cardCount}
        </span>
        <button
          type="button"
          className="tool"
          onClick={() => onGoTo(view.cardNumber + 1)}
          title="Next card"
        >
          ▶
        </button>
        <button
          type="button"
          className="tool"
          onClick={() => onGoTo(view.cardCount)}
          title="Last card"
        >
          ▶|
        </button>
      </div>

      {error === null ? (
        <input
          className="statusbar__message"
          value={draft}
          spellCheck={false}
          placeholder="Type a HyperTalk statement and press Return"
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && draft.trim() !== '') onRunMessage(draft);
          }}
        />
      ) : (
        <button type="button" className="statusbar__error" onClick={onDismissError}>
          {error}
        </button>
      )}
    </div>
  );
}
