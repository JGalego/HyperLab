/** The card itself: background parts, then card parts, then nothing else. */

import { Part } from './Part';
import type { PartView, Selection, StackView, Tool } from '../types';

interface Props {
  view: StackView;
  tool: Tool;
  selection: Selection | null;
  onClickPart: (part: PartView) => void;
  onSelectPart: (part: PartView) => void;
  onMovePart: (part: PartView, left: number, top: number) => void;
  onEditField: (part: PartView, text: string) => void;
  onSelectCard: () => void;
}

/**
 * Draws the current card.
 *
 * Background parts are drawn first and card parts on top, which is both how
 * HyperCard looked and why a card can cover one field of a shared layout
 * without disturbing the others.
 */
export function Card({
  view,
  tool,
  selection,
  onClickPart,
  onSelectPart,
  onMovePart,
  onEditField,
  onSelectCard,
}: Props) {
  const parts: PartView[] = [...(view.background?.parts ?? []), ...view.card.parts];

  return (
    <div
      className={`card ${tool === 'edit' ? 'card--editing' : ''}`}
      style={{ width: view.cardSize.width, height: view.cardSize.height }}
      onMouseDown={(event) => {
        // A click on the card itself, rather than on a part, selects the card.
        if (event.target === event.currentTarget) onSelectCard();
      }}
    >
      {tool === 'edit' && <div className="card__grid" aria-hidden="true" />}
      {parts.map((part) => (
        <Part
          key={`${part.layer}-${part.id}`}
          part={part}
          tool={tool}
          selected={selection?.id === part.id && selection.kind === part.kind}
          onClick={onClickPart}
          onSelect={onSelectPart}
          onMove={onMovePart}
          onEdit={onEditField}
        />
      ))}
    </div>
  );
}
