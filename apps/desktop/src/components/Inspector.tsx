/**
 * The inspector: what an object is, and what it is made of.
 *
 * The inspector reads the snapshot and writes through commands. It never
 * holds a copy of an object, which is why it cannot disagree with the card
 * next to it.
 */

import { useMemo, useState } from 'react';

import { ScriptEditor } from './ScriptEditor';
import type { ObjectKind, PropertyView, Selection, StackView } from '../types';

type Tab = 'properties' | 'script' | 'hierarchy';

interface Props {
  view: StackView;
  selection: Selection | null;
  onSelect: (selection: Selection) => void;
  onSetProperty: (
    kind: ObjectKind,
    id: number,
    property: string,
    value: string | number | boolean,
  ) => void;
  onSetScript: (kind: ObjectKind, id: number, script: string) => void;
}

/** What the inspector is currently pointed at, gathered from the snapshot. */
interface Subject {
  kind: ObjectKind;
  id: number;
  name: string;
  script: string;
  properties: PropertyView[];
}

export function Inspector({
  view,
  selection,
  onSelect,
  onSetProperty,
  onSetScript,
}: Props) {
  const [tab, setTab] = useState<Tab>('properties');
  const subject = useMemo(() => findSubject(view, selection), [view, selection]);

  return (
    <aside className="inspector">
      <div className="inspector__tabs" role="tablist">
        {(['properties', 'script', 'hierarchy'] as Tab[]).map((name) => (
          <button
            key={name}
            type="button"
            role="tab"
            className="inspector__tab"
            aria-selected={tab === name}
            onClick={() => setTab(name)}
          >
            {name === 'properties'
              ? 'Properties'
              : name === 'script'
                ? 'Script'
                : 'Objects'}
          </button>
        ))}
      </div>

      <div className="inspector__body">
        {tab === 'hierarchy' ? (
          <Hierarchy view={view} selection={selection} onSelect={onSelect} />
        ) : subject === null ? (
          <p className="inspector__empty">Nothing is selected.</p>
        ) : tab === 'properties' ? (
          <>
            <h2 className="inspector__heading">
              {subject.kind} “{subject.name}”
            </h2>
            <Properties subject={subject} onSetProperty={onSetProperty} />
          </>
        ) : (
          <ScriptEditor
            key={`${subject.kind}-${subject.id}`}
            title={`script of ${subject.kind} “${subject.name}”`}
            source={subject.script}
            onSave={(script) => onSetScript(subject.kind, subject.id, script)}
          />
        )}
      </div>
    </aside>
  );
}

/** The property table. */
function Properties({
  subject,
  onSetProperty,
}: {
  subject: Subject;
  onSetProperty: Props['onSetProperty'];
}) {
  return (
    <div className="properties">
      {subject.properties.map((property) => (
        <PropertyRow
          key={property.name}
          property={property}
          onChange={(value) =>
            onSetProperty(subject.kind, subject.id, property.name, value)
          }
        />
      ))}
    </div>
  );
}

/**
 * One property. The editor is chosen from the value's type, so a property
 * HyperLab has never heard of still gets a sensible row.
 */
function PropertyRow({
  property,
  onChange,
}: {
  property: PropertyView;
  onChange: (value: string | number | boolean) => void;
}) {
  const [draft, setDraft] = useState(String(property.value ?? ''));

  if (typeof property.value === 'boolean') {
    return (
      <>
        <label className="properties__name" htmlFor={`property-${property.name}`}>
          {property.name}
        </label>
        <input
          id={`property-${property.name}`}
          type="checkbox"
          checked={property.value}
          disabled={property.readOnly}
          onChange={(event) => onChange(event.target.checked)}
        />
      </>
    );
  }

  const numeric = typeof property.value === 'number';
  return (
    <>
      <label className="properties__name" htmlFor={`property-${property.name}`}>
        {property.name}
      </label>
      <input
        id={`property-${property.name}`}
        type={numeric ? 'number' : 'text'}
        value={draft}
        readOnly={property.readOnly}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={() => {
          if (draft === String(property.value ?? '')) return;
          onChange(numeric ? Number(draft) : draft);
        }}
        onKeyDown={(event) => {
          if (event.key === 'Enter') event.currentTarget.blur();
        }}
      />
    </>
  );
}

/** The tree of objects, which doubles as a way to select them. */
function Hierarchy({
  view,
  selection,
  onSelect,
}: {
  view: StackView;
  selection: Selection | null;
  onSelect: (selection: Selection) => void;
}) {
  const row = (kind: ObjectKind, id: number, label: string) => (
    <li key={`${kind}-${id}`}>
      <button
        type="button"
        className="hierarchy__item"
        aria-current={selection?.kind === kind && selection.id === id}
        onClick={() => onSelect({ kind, id })}
      >
        {label}
      </button>
    </li>
  );

  return (
    <ul className="hierarchy">
      {row('stack', view.stackId, `stack “${view.stackName}”`)}
      <ul className="hierarchy__children">
        {view.background && (
          <>
            {row(
              'background',
              view.background.id,
              `background “${view.background.name}”`,
            )}
            <ul className="hierarchy__children">
              {view.background.parts.map((part) =>
                row(part.kind, part.id, `${part.kind} “${part.name}”`),
              )}
            </ul>
          </>
        )}
        {row('card', view.card.id, `card “${view.card.name}”`)}
        <ul className="hierarchy__children">
          {view.card.parts.map((part) =>
            row(part.kind, part.id, `${part.kind} “${part.name}”`),
          )}
        </ul>
      </ul>
    </ul>
  );
}

/** Finds the selected object in the snapshot. */
function findSubject(view: StackView, selection: Selection | null): Subject | null {
  if (selection === null) return null;

  if (selection.kind === 'stack') {
    return {
      kind: 'stack',
      id: view.stackId,
      name: view.stackName,
      script: view.stackScript,
      properties: [
        { name: 'name', value: view.stackName, readOnly: false },
        { name: 'width', value: view.cardSize.width, readOnly: true },
        { name: 'height', value: view.cardSize.height, readOnly: true },
        { name: 'cards', value: view.cardCount, readOnly: true },
      ],
    };
  }

  if (selection.kind === 'card' || selection.kind === 'background') {
    const container = selection.kind === 'card' ? view.card : view.background;
    if (!container || container.id !== selection.id) return null;
    return {
      kind: selection.kind,
      id: container.id,
      name: container.name,
      script: container.script,
      properties: [
        { name: 'id', value: container.id, readOnly: true },
        { name: 'name', value: container.name, readOnly: false },
      ],
    };
  }

  const part = [...(view.background?.parts ?? []), ...view.card.parts].find(
    (candidate) => candidate.id === selection.id,
  );
  if (!part) return null;
  return {
    kind: part.kind,
    id: part.id,
    name: part.name,
    script: part.script,
    properties: part.properties,
  };
}
