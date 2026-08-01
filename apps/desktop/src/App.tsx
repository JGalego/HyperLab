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
import { AiSettings } from './components/AiSettings';
import { Assistant } from './components/Assistant';
import { Card } from './components/Card';
import { Dialog } from './components/Dialog';
import { Inspector } from './components/Inspector';
import { MenuBar, type MenuEntry } from './components/MenuBar';
import { StackMap } from './components/StackMap';
import { toPng } from './map/picture';
import { StatusBar } from './components/StatusBar';
import type {
  AiView,
  DialogRequest,
  Graph,
  Outcome,
  PartView,
  Selection,
  StackView,
  Tool,
} from './types';

export function App() {
  const [view, setView] = useState<StackView>(api.emptyView);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [tool, setTool] = useState<Tool>('browse');
  const [error, setError] = useState<string | null>(null);
  // Kept apart from `error` so that "saved to …" is not dressed as a failure.
  const [notice, setNotice] = useState<string | null>(null);
  const [dialog, setDialog] = useState<DialogRequest | null>(null);
  // Whether the shell is behind us never changes while we are running, so
  // a browser is ready to show its notice on the first render.
  const [ready, setReady] = useState(!api.inDesktopApp());
  const [ai, setAi] = useState<AiView | null>(null);
  const [aiOpen, setAiOpen] = useState(false);
  const [aiSettings, setAiSettings] = useState(false);
  // Read when the map is opened and thrown away when it closes: it is a
  // reading of the stack as it is now, and a script may have changed it.
  const [map, setMap] = useState<Graph | null>(null);
  // Pictures are fetched by name and kept, because a snapshot arrives after
  // every command and re-encoding a card of artwork each time would be
  // absurd. Cleared when a different stack is opened.
  const [pictures, setPictures] = useState(new Map<string, string>());

  /**
   * Applies whatever a command gave back.
   *
   * Dialogs are not in here: a script that shows one is *blocked* until it is
   * answered, so it arrives as an event while the command is still running.
   * By the time an outcome comes back, the dialogs are long dismissed.
   */
  const apply = useCallback((outcome: Outcome) => {
    setView(outcome.view);
    setError(null);
  }, []);

  /** Runs a command, showing anything that went wrong rather than throwing. */
  const run = useCallback(
    (work: () => Promise<Outcome>) => {
      work().then(apply, (reason: unknown) => setError(String(reason)));
    },
    [apply],
  );

  // Watching for dialogs comes first: a stack whose openStack handler asks a
  // question would otherwise block with nothing listening.
  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    api.onDialog(setDialog).then((unlisten) => {
      if (cancelled) unlisten();
      else stop = unlisten;
    });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);

  useEffect(() => {
    if (!api.inDesktopApp()) return;
    // Read up front, but not shown: the panel costs 300px, and someone with
    // no model configured should never have to close it.
    api.aiView().then(setAi, () => setAi(null));
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

  // Whatever the current card draws, fetched once. A picture the model has
  // already accepted, so the only thing that can go wrong is that it was
  // removed between the snapshot and the ask.
  useEffect(() => {
    const wanted = [...(view.background?.parts ?? []), ...view.card.parts]
      .map((part) => part.source)
      .filter((source) => source !== '' && !pictures.has(source));
    if (wanted.length === 0) return;

    let cancelled = false;
    Promise.all(
      [...new Set(wanted)].map((source) =>
        api.stackImage(source).then(
          (uri) => [source, uri] as const,
          () => null,
        ),
      ),
    ).then((fetched) => {
      const found = fetched.filter((one) => one !== null);
      if (cancelled || found.length === 0) return;
      setPictures((known) => new Map([...known, ...found]));
    });
    return () => {
      cancelled = true;
    };
  }, [view.card, view.background, pictures]);

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
        if (typeof chosen !== 'string') return;
        // A different stack has different pictures, and the names may well
        // collide: "board.png" in one is not "board.png" in another.
        setPictures(new Map());
        run(() => api.openStack(chosen));
      },
      (reason: unknown) => setError(String(reason)),
    );
  }

  function importImage(layer: 'card' | 'background') {
    openFileDialog({
      title: 'Choose a picture',
      filters: [
        { name: 'Pictures', extensions: ['svg', 'png', 'jpg', 'jpeg', 'gif', 'webp'] },
      ],
    }).then(
      (chosen) => {
        if (typeof chosen === 'string') run(() => api.importImage(chosen, layer));
      },
      (reason: unknown) => setError(String(reason)),
    );
  }

  /** The whole stack as a PDF, one page per card. */
  function exportPdf() {
    saveFileDialog({
      title: 'Export this stack as a PDF',
      defaultPath: `${view.stackName}.pdf`,
      filters: [{ name: 'PDF', extensions: ['pdf'] }],
    }).then(
      (chosen) => {
        if (typeof chosen !== 'string') return;
        api.exportPdf(chosen).then(
          (written) => setNotice(`Exported to ${written}`),
          (reason: unknown) => setError(String(reason)),
        );
      },
      (reason: unknown) => setError(String(reason)),
    );
  }

  /** The map as a PNG. Drawn in the window, because only it knows the shape. */
  function saveMap(svg: SVGSVGElement) {
    saveFileDialog({
      title: 'Save the map',
      defaultPath: `${view.stackName} map.png`,
      filters: [{ name: 'PNG', extensions: ['png'] }],
    }).then(
      (chosen) => {
        if (typeof chosen !== 'string') return;
        toPng(svg)
          .then((bytes) => api.exportPng(chosen, bytes))
          .then(
            (written) => setNotice(`Saved to ${written}`),
            (reason: unknown) => setError(String(reason)),
          );
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
        null,
        { label: 'Export as PDF…', run: exportPdf },
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
        { label: 'Import Picture…', run: () => importImage('card') },
        {
          label: 'Import Background Picture…',
          run: () => importImage('background'),
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
        null,
        {
          label: 'Map…',
          run: () =>
            api.stackGraph().then(setMap, (reason: unknown) => setError(String(reason))),
        },
      ],
    },
    {
      title: 'AI',
      entries: [
        {
          label: aiOpen ? 'Hide Assistant' : 'Show Assistant',
          run: () => {
            if (aiOpen) {
              setAiOpen(false);
              return;
            }
            setAiOpen(true);
            api.aiView().then(setAi, (reason: unknown) => setError(String(reason)));
          },
        },
        null,
        { label: 'Settings…', run: () => setAiSettings(true) },
      ],
    },
  ];

  return (
    <div className="app">
      <MenuBar view={view} menus={menus} />

      <div className="app__body">
        <div className="app__stage">
          <Card
            view={view}
            tool={tool}
            selection={selection}
            pictures={pictures}
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

        {aiOpen && ai !== null && (
          <Assistant
            view={ai}
            onView={setAi}
            onChanged={apply}
            onError={setError}
            onOpenSettings={() => setAiSettings(true)}
          />
        )}

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
        notice={notice}
        onGoTo={(position) => run(() => api.goToCard(position))}
        onSetTool={setTool}
        onRunMessage={(source) => run(() => api.runMessageBox(source))}
        onDismiss={() => {
          setError(null);
          setNotice(null);
        }}
      />

      {aiSettings && (
        <AiSettings
          onDone={(next) => {
            setAi(next);
            setAiOpen(true);
            setAiSettings(false);
          }}
          onCancel={() => setAiSettings(false)}
          onError={setError}
        />
      )}

      {map && (
        <StackMap
          graph={map}
          current={view.card.id}
          onGoTo={(position) => run(() => api.goToCard(position))}
          onClose={() => setMap(null)}
          onSave={saveMap}
        />
      )}

      {dialog && (
        <Dialog
          request={dialog}
          onReply={(text) => {
            setDialog(null);
            api.dialogReply(text).catch((reason: unknown) => setError(String(reason)));
          }}
        />
      )}
    </div>
  );
}

function isPart(kind: Selection['kind']): boolean {
  return kind === 'button' || kind === 'field' || kind === 'image';
}
