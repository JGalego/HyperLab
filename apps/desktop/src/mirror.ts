import { useState } from 'react';

/**
 * Local editing state that follows a value from the runtime.
 *
 * A field you are typing into holds a draft, but the snapshot is the truth:
 * when a script, an undo or a move to another card changes the value
 * underneath, the draft has to follow.
 *
 * Adjusting during render rather than in an effect is React's own answer to
 * this. An effect renders once with the stale draft, then runs, then renders
 * again — so the old text is briefly on screen, and anything reading the
 * draft in between sees the wrong thing.
 */
export function useMirror<T>(value: T): [T, (next: T) => void] {
  const [draft, setDraft] = useState(value);
  const [seen, setSeen] = useState(value);

  if (value !== seen) {
    setSeen(value);
    setDraft(value);
  }

  return [draft, setDraft];
}
