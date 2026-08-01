/** Drawing one part. */

import { useEffect, useRef, useState } from 'react';

import type { PartView, Tool } from '../types';

interface Props {
  part: PartView;
  tool: Tool;
  selected: boolean;
  /** The picture an image part draws, once it has been fetched. */
  picture: string | undefined;
  /** Browse mode: the part was clicked. */
  onClick: (part: PartView) => void;
  /** Edit mode: the part was picked. */
  onSelect: (part: PartView) => void;
  /** Edit mode: the part was dragged to a new position. */
  onMove: (part: PartView, left: number, top: number) => void;
  /** Browse mode: a field's contents were edited. */
  onEdit: (part: PartView, text: string) => void;
}

/**
 * A part draws itself from the snapshot and reports what happened. It never
 * changes anything: even dragging only reports where the part ended up, and
 * the runtime decides what that means.
 */
export function Part({
  part,
  tool,
  selected,
  picture,
  onClick,
  onSelect,
  onMove,
  onEdit,
}: Props) {
  const [left, top, width, height] = part.rect;
  const [offset, setOffset] = useState<{ dx: number; dy: number } | null>(null);
  const origin = useRef({ x: 0, y: 0 });
  const [draft, setDraft] = useState(part.text);

  // The snapshot is the truth: if the runtime changed the text underneath us
  // — a script, an undo, a different card — the draft follows it.
  useEffect(() => setDraft(part.text), [part.text]);

  useEffect(() => {
    if (offset === null) return undefined;
    const move = (event: MouseEvent) =>
      setOffset({
        dx: event.clientX - origin.current.x,
        dy: event.clientY - origin.current.y,
      });
    const up = (event: MouseEvent) => {
      const dx = event.clientX - origin.current.x;
      const dy = event.clientY - origin.current.y;
      setOffset(null);
      if (dx !== 0 || dy !== 0) onMove(part, left + dx, top + dy);
    };
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', up);
    return () => {
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', up);
    };
    // `offset === null` is the only thing that starts and stops the drag;
    // re-running on every pixel of movement would tear the listeners down.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [offset === null, left, top, part, onMove]);

  if (!part.visible && tool === 'browse') {
    return null;
  }

  const style: React.CSSProperties = {
    left: left + (offset?.dx ?? 0),
    top: top + (offset?.dy ?? 0),
    width,
    height,
  };
  const classes = [
    'part',
    part.kind,
    styleClass(part),
    part.visible ? '' : 'part--hidden',
    selected ? 'part--selected' : '',
    part.layer === 'background' ? 'part--background' : '',
  ]
    .filter(Boolean)
    .join(' ');

  if (tool === 'edit') {
    // In edit mode every part is an inert rectangle to be picked up.
    return (
      <div
        className={classes}
        style={style}
        onMouseDown={(event) => {
          event.preventDefault();
          onSelect(part);
          origin.current = { x: event.clientX, y: event.clientY };
          setOffset({ dx: 0, dy: 0 });
        }}
      >
        {part.kind === 'image' ? <Picture part={part} picture={picture} /> : null}
        {part.kind === 'button' && part.showName ? part.name : ''}
        {part.kind === 'field' ? part.text : ''}
      </div>
    );
  }

  if (part.kind === 'image') {
    return (
      <div
        className={classes}
        style={style}
        role={part.script === '' ? undefined : 'button'}
        onClick={() => onClick(part)}
      >
        <Picture part={part} picture={picture} />
      </div>
    );
  }

  if (part.kind === 'button') {
    return (
      <button
        type="button"
        className={classes}
        style={style}
        disabled={!part.enabled}
        // A transparent button laid over artwork shows no text, so its name
        // is the only thing that says what it does. Without this a screen
        // reader — and anything else driving the window — sees nine
        // identical unlabelled buttons on a drawn board.
        aria-label={part.showName ? undefined : part.name}
        onClick={() => onClick(part)}
      >
        {part.showName ? part.name : ''}
      </button>
    );
  }

  return (
    <textarea
      className={classes}
      style={style}
      value={draft}
      readOnly={part.locked || !part.enabled}
      spellCheck={false}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={() => {
        if (draft !== part.text) onEdit(part, draft);
      }}
    />
  );
}

/**
 * The picture itself.
 *
 * Always an `<img>`, never an inlined `<svg>`. A browser runs no script and
 * fetches nothing for an image loaded this way, so a stack someone sent you
 * cannot phone home or reach into the window — and the model refuses bytes
 * that are not the format they claim, so the two checks stand together.
 */
function Picture({ part, picture }: { part: PartView; picture: string | undefined }) {
  if (part.source === '') {
    return <span className="image__missing">no picture chosen</span>;
  }
  if (picture === undefined) {
    // Fetched by name after the snapshot arrives, so there is a moment with
    // nothing to draw. Saying which picture beats an empty box.
    return <span className="image__missing">{part.source}</span>;
  }
  return (
    <img className="image__picture" src={picture} alt={part.name} draggable={false} />
  );
}

/** Turns the `style` property into a class the theme can dress. */
function styleClass(part: PartView): string {
  const style = part.style.toLowerCase();
  if (part.kind === 'button') {
    if (style === 'rectangle') return 'button--rectangle';
    if (style === 'transparent') return 'button--transparent';
    return '';
  }
  if (part.kind === 'image') {
    return style === 'rectangle' ? 'image--framed' : '';
  }
  if (style === 'transparent') return 'field--transparent';
  if (style === 'shadow') return 'field--shadow';
  return '';
}
