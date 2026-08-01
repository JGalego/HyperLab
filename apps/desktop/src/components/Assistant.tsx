/**
 * The AI sidebar.
 *
 * Three things it must never be vague about, because they are the whole
 * bargain: which model is answering, whether the assistant may change the
 * stack, and exactly what was sent. The last of those is a disclosure on
 * every question — the same string the model received, not a summary of it.
 */

import { useEffect, useRef, useState } from 'react';

import * as api from '../api';
import type { AiEntry, AiView, Outcome } from '../types';

interface Props {
  view: AiView;
  onView: (view: AiView) => void;
  onChanged: (outcome: Outcome) => void;
  onError: (reason: string) => void;
  onOpenSettings: () => void;
}

/** Things worth trying, for a sidebar that is otherwise an empty box. */
const SUGGESTIONS = [
  'Explain this script',
  'Add a search button',
  'What is this card for?',
] as const;

export function Assistant({ view, onView, onChanged, onError, onOpenSettings }: Props) {
  const [draft, setDraft] = useState('');
  const transcript = useRef<HTMLDivElement>(null);

  // A conversation that scrolls itself is the difference between reading the
  // answer and hunting for it.
  useEffect(() => {
    transcript.current?.scrollTo({ top: transcript.current.scrollHeight });
  }, [view.entries, view.busy]);

  const ask = (question: string) => {
    const asked = question.trim();
    if (asked === '' || view.busy) return;
    setDraft('');
    onView({ ...view, busy: true });

    api
      .aiAsk(asked)
      .then(onChanged, (reason: unknown) => onError(String(reason)))
      .finally(() => {
        api.aiView().then(onView, (reason: unknown) => onError(String(reason)));
      });
  };

  const toggle = (work: () => Promise<AiView>) => {
    work().then(onView, (reason: unknown) => onError(String(reason)));
  };

  const ready = view.providers.length > 0;

  return (
    <aside className="assistant" aria-label="Assistant">
      <div className="assistant__bar">
        <span className="assistant__who">{view.provider ?? 'No model'}</span>
        <button type="button" className="tool" onClick={onOpenSettings}>
          Settings
        </button>
        <button
          type="button"
          className="tool"
          disabled={view.entries.length === 0}
          onClick={() => toggle(api.aiClear)}
        >
          Clear
        </button>
      </div>

      <div className="assistant__switches">
        <label className="assistant__switch">
          <input
            type="checkbox"
            checked={view.mayEdit}
            onChange={(event) => toggle(() => api.aiSetMayEdit(event.target.checked))}
          />
          May change the stack
        </label>
        <label className="assistant__switch">
          <input
            type="checkbox"
            checked={view.sendsFieldText}
            onChange={(event) =>
              toggle(() => api.aiSetSendsFieldText(event.target.checked))
            }
          />
          Send field contents
        </label>
      </div>

      {view.problems.length > 0 && (
        <ul className="assistant__problems">
          {view.problems.map((problem) => (
            <li key={problem}>{problem}</li>
          ))}
        </ul>
      )}

      <div className="assistant__transcript" ref={transcript}>
        {!ready && (
          <p className="assistant__empty">
            No language model is set up. HyperLab works without one — this panel is the
            only part that does not.
          </p>
        )}

        {ready && view.entries.length === 0 && (
          <div className="assistant__empty">
            <p>Ask about the card you are looking at. For example:</p>
            <ul className="assistant__suggestions">
              {SUGGESTIONS.map((suggestion) => (
                <li key={suggestion}>
                  <button type="button" className="tool" onClick={() => ask(suggestion)}>
                    {suggestion}
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}

        {view.entries.map((entry, index) => (
          <Said key={index} entry={entry} />
        ))}

        {view.busy && <p className="assistant__thinking">Thinking…</p>}
      </div>

      <form
        className="assistant__ask"
        onSubmit={(event) => {
          event.preventDefault();
          ask(draft);
        }}
      >
        <textarea
          className="assistant__field"
          value={draft}
          rows={2}
          spellCheck={false}
          disabled={!ready}
          placeholder={ready ? 'Ask about this stack' : 'Set up a model first'}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            // Return sends; shift-Return is a new line, as everywhere else.
            if (event.key === 'Enter' && !event.shiftKey) {
              event.preventDefault();
              ask(draft);
            }
          }}
        />
        <button type="submit" className="tool" disabled={!ready || view.busy}>
          Ask
        </button>
      </form>
    </aside>
  );
}

/** One entry in the transcript. */
function Said({ entry }: { entry: AiEntry }) {
  if (entry.kind === 'question') {
    return (
      <div className="said said--question">
        <p className="said__text">{entry.text}</p>
        <details className="said__sent">
          <summary>
            What was sent
            {entry.briefing.includedFieldText ? ', including field contents' : ''}
          </summary>
          <pre className="said__context">{entry.briefing.context}</pre>
        </details>
      </div>
    );
  }

  if (entry.kind === 'answer') {
    return (
      <div className="said said--answer">
        <p className="said__text">{entry.text}</p>
      </div>
    );
  }

  if (entry.kind === 'used') {
    return (
      <details className={`said said--used${entry.allowed ? '' : ' said--refused'}`}>
        <summary>
          {entry.allowed ? 'Used' : 'Could not use'} <code>{entry.tool}</code>
        </summary>
        <pre className="said__context">
          {entry.arguments}
          {'\n\n'}
          {entry.outcome}
        </pre>
      </details>
    );
  }

  return (
    <div className="said said--failed">
      <p className="said__text">{entry.reason}</p>
    </div>
  );
}
