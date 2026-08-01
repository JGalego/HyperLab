/** The script editor. */

import { useEffect, useState } from 'react';

import { checkScript } from '../api';

interface Props {
  title: string;
  source: string;
  onSave: (script: string) => void;
}

/**
 * A plain text editor with one extra thing: it asks the runtime whether what
 * you have written parses, and says so.
 *
 * The check is the *real* parser, not a copy of it in TypeScript. There is
 * one definition of HyperTalk in HyperLab and it lives in Rust.
 */
export function ScriptEditor({ title, source, onSave }: Props) {
  const [draft, setDraft] = useState(source);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => setDraft(source), [source]);

  // Checking is debounced: nobody wants an error message about the code they
  // are halfway through typing.
  useEffect(() => {
    const timer = window.setTimeout(() => {
      checkScript(draft).then(
        () => setProblem(null),
        (error: unknown) => setProblem(String(error)),
      );
    }, 350);
    return () => window.clearTimeout(timer);
  }, [draft]);

  const dirty = draft !== source;

  return (
    <div className="script">
      <h2 className="inspector__heading">{title}</h2>
      <textarea
        className="script__source"
        value={draft}
        spellCheck={false}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          // Tab indents rather than leaving the editor: this is a code box.
          if (event.key === 'Tab') {
            event.preventDefault();
            const target = event.currentTarget;
            const { selectionStart, selectionEnd, value } = target;
            const next = `${value.slice(0, selectionStart)}  ${value.slice(selectionEnd)}`;
            setDraft(next);
            window.requestAnimationFrame(() => {
              target.selectionStart = selectionStart + 2;
              target.selectionEnd = selectionStart + 2;
            });
          }
          if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
            event.preventDefault();
            onSave(draft);
          }
        }}
      />
      {problem === null ? (
        <p className="script__ok">
          {draft.trim() === '' ? 'No handlers yet.' : 'This script parses.'}
        </p>
      ) : (
        <p className="script__error">{problem}</p>
      )}
      <div className="dialog__buttons">
        <button
          type="button"
          className="tool"
          disabled={!dirty}
          onClick={() => setDraft(source)}
        >
          Revert
        </button>
        <button
          type="button"
          className="tool"
          disabled={!dirty}
          onClick={() => onSave(draft)}
        >
          Save Script
        </button>
      </div>
    </div>
  );
}
