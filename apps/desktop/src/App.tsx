/**
 * The application.
 *
 * `App` holds three pieces of state and no more: the last snapshot the
 * runtime sent, what is selected, and which tool is in hand. Everything else
 * — the stack, the undo history, what a script did — lives in the runtime,
 * because a second copy is a second source of disagreement.
 */

import { useCallback, useEffect, useState } from 'react';
import {
  open as openFileDialog,
  save as saveFileDialog,
} from '@tauri-apps/plugin-dialog';

import * as api from './api';
import { Card } from './components/Card';
import { Dialog } from './components/Dialog';
import { Inspector } from './components/Inspector';
import { MenuBar, type MenuEntry } from './components/MenuBar';
import { StatusBar } from './components/StatusBar';
import type { Effect, Outcome, PartView, Selection, StackView, Tool } from './types';

/** Effects that need the user to press OK. */
type Modal = Extract<Effect, { kind: 'answer' } | { kind: 'ask' }>;

export function App() {
  const [view, setView] = useState<StackView>(api.emptyView);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [tool, setTool] = useState<Tool>('browse');
  const [error, setError] = useState<string | null>(null);
  const [dialogs, setDialogs] = useState<Modal[]>([]);
  const [ready, setReady] = useState(false);

  /** Applies whatever a command gave back. */
  const apply = useCallback((outcome: Outcome) => {
    setView(outcome.view);
    setError(null);
    const modals = outcome.effects.filter(
      (effect): effect is Modal => effect.kind === 'answer' || effect.kind === 'ask',
    );
    if (modals.length > 0) setDialogs((queued) => [...queued, ...modals]);
  }, []);

  /** Runs a command, showing anything that went wrong rather than throwing. */
  const run = useCallback(
    (work: () => Promise<Outcome>) => {
      work().then(apply, (reason: unknown) => setError(String(reason)));
    },
    [apply],
  );

  useEffect(() => {
    if (!api.inDesktopApp()) {
      setReady(true);
      return;
    }
    api.getView().then(
      (outcome) => {
        apply(outcome);
        setReady(true);
      },
      (reason: unknown) => {
        setError(String(reason));
        setReady(true);
      },
    );
  }, [apply]);

  // Keyboard shortcuts, kept in one place so the menus and the keys cannot
  // drift apart: both call the same functions.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey)) return;
      const key = event.key.toLowerCase();
      const shortcuts: Record<string, () => void> = {
        s: () => saveStack(false),
        o: () => openStack(),
        n: () => run(() => api.newCard()),
        z: () => run(() => (event.shiftKey ? api.redo() : api.undo())),
      };
      const action = shortcuts[key];
      if (action) {
        event.preventDefault();
        action();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [run, view.path]);

  function openStack() {
    openFileDialog({ directory: true, title: 'Open a HyperLab stack' }).then(
      (chosen) => {
        if (typeof chosen === 'string') run(() => api.openStack(chosen));
      },
      (reason: unknown) => setError(String(reason)),
    );
  }

  function saveStack(chooseWhere: boolean) {
    if (!chooseWhere && view.path !== null) {
      run(() => api.saveStack());
      return;
    }
    saveFileDialog({
      title: 'Save this stack',
      defaultPath: `${view.stackName}.hl`,
    }).then(
      (chosen) => {
        if (typeof chosen === 'string') run(() => api.saveStack(chosen));
      },
      (reason: unknown) => setError(String(reason)),
    );
  }

  if (!ready) return null;

  if (!api.inDesktopApp()) {
    return (
      <div className="notice">
        <h1>HyperLab</h1>
        <p>
          This page is the interface only. The runtime lives in the desktop shell, so
          start HyperLab with <code>npm run tauri dev</code> to use it.
        </p>
      </div>
    );
  }

  const menus: { title: string; entries: MenuEntry[] }[] = [
    {
      title: 'File',
      entries: [
        { label: 'New Stack', run: () => run(() => api.newStack()) },
        { label: 'Open Stack…', shortcut: '⌘O', run: openStack },
        null,
        {
          label: 'Save',
          shortcut: '⌘S',
          disabled: !view.dirty && view.path !== null,
          run: () => saveStack(false),
        },
        { label: 'Save As…', run: () => saveStack(true) },
      ],
    },
    {
      title: 'Edit',
      entries: [
        {
          label: view.undo === null ? 'Undo' : `Undo ${view.undo}`,
          shortcut: '⌘Z',
          disabled: view.undo === null,
          run: () => run(() => api.undo()),
        },
        {
          label: view.redo === null ? 'Redo' : `Redo ${view.redo}`,
          shortcut: '⇧⌘Z',
          disabled: view.redo === null,
          run: () => run(() => api.redo()),
        },
      ],
    },
    {
      title: 'Objects',
      entries: [
        { label: 'New Card', shortcut: '⌘N', run: () => run(() => api.newCard()) },
        {
          label: 'Delete Card',
          disabled: view.cardCount <= 1,
          run: () => run(() => api.deleteCard()),
        },
        null,
        { label: 'New Button', run: () => run(() => api.newPart('button', 'card')) },
        { label: 'New Field', run: () => run(() => api.newPart('field', 'card')) },
        {
          label: 'New Background Button',
          run: () => run(() => api.newPart('button', 'background')),
        },
        {
          label: 'New Background Field',
          run: () => run(() => api.newPart('field', 'background')),
        },
        null,
        {
          label: 'Delete Selected Part',
          disabled: selection === null || !isPart(selection.kind),
          run: () => {
            if (selection && isPart(selection.kind)) {
              const id = selection.id;
              setSelection(null);
              run(() => api.deletePart(id));
            }
          },
        },
      ],
    },
    {
      title: 'Go',
      entries: [
        { label: 'First Card', run: () => run(() => api.goToCard(1)) },
        {
          label: 'Previous Card',
          run: () => run(() => api.goToCard(view.cardNumber - 1)),
        },
        { label: 'Next Card', run: () => run(() => api.goToCard(view.cardNumber + 1)) },
        { label: 'Last Card', run: () => run(() => api.goToCard(view.cardCount)) },
      ],
    },
  ];

  const dialog = dialogs[0];

  return (
    <div className="app">
      <MenuBar view={view} menus={menus} />

      <div className="app__body">
        <div className="app__stage">
          <Card
            view={view}
            tool={tool}
            selection={selection}
            onClickPart={(part) => run(() => api.clickPart(part.id))}
            onSelectPart={(part: PartView) =>
              setSelection({ kind: part.kind, id: part.id })
            }
            onMovePart={(part, left, top) =>
              run(() => api.setGeometry(part.id, left, top, part.rect[2], part.rect[3]))
            }
            onEditField={(part, text) => run(() => api.setFieldText(part.id, text))}
            onSelectCard={() => setSelection({ kind: 'card', id: view.card.id })}
          />
        </div>

        <Inspector
          view={view}
          selection={selection}
          onSelect={setSelection}
          onSetProperty={(kind, id, property, value) =>
            run(() => api.setProperty(kind, id, property, value))
          }
          onSetScript={(kind, id, script) => run(() => api.setScript(kind, id, script))}
        />
      </div>

      <StatusBar
        view={view}
        tool={tool}
        error={error}
        onGoTo={(position) => run(() => api.goToCard(position))}
        onSetTool={setTool}
        onRunMessage={(source) => run(() => api.runMessageBox(source))}
        onDismissError={() => setError(null)}
      />

      {dialog && (
        <Dialog
          effect={dialog}
          onDismiss={() => setDialogs((queued) => queued.slice(1))}
        />
      )}
    </div>
  );
}

function isPart(kind: Selection['kind']): boolean {
  return kind === 'button' || kind === 'field';
}
