/**
 * The application, as a page.
 *
 * A sibling of the desktop's `App.tsx` that renders the same components —
 * the card, the inspector, the assistant, the map — over the WebAssembly
 * runtime. What is different is exactly what a browser does differently:
 * files are uploads and downloads, pictures arrive from a file picker, and
 * the example stacks are fetched from the site itself.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import { Assistant } from '../../desktop/src/components/Assistant';
import { Card } from '../../desktop/src/components/Card';
import { Dialog } from '../../desktop/src/components/Dialog';
import { Inspector } from '../../desktop/src/components/Inspector';
import { MenuBar, type MenuEntry } from '../../desktop/src/components/MenuBar';
import { StackMap } from '../../desktop/src/components/StackMap';
import { StatusBar } from '../../desktop/src/components/StatusBar';
import { toPng } from '../../desktop/src/map/picture';
import type {
  AiView,
  DialogRequest,
  Graph,
  Outcome,
  PartView,
  Selection,
  StackView,
  Tool,
} from '../../desktop/src/types';

import * as api from './api';
import { AiSettings } from './components/AiSettings';

/**
 * The stacks bundled with the site, packed from `examples/` by
 * `cargo run -p hyperlab-persistence --example pack_single_files`.
 */
const EXAMPLES = [
  'Address Book',
  'Cluedo',
  'LLMs for n00bs',
  'Myst',
  'Recipe Box',
  'Todo',
] as const;

/** Hands the user a file, which is what "save" means on a page. */
function download(name: string, text: string, type: string) {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = name;
  anchor.click();
  URL.revokeObjectURL(url);
}

function downloadBytes(name: string, bytes: Uint8Array) {
  const url = URL.createObjectURL(
    new Blob([bytes as unknown as BlobPart], { type: 'image/png' }),
  );
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = name;
  anchor.click();
  URL.revokeObjectURL(url);
}

export function App() {
  const [view, setView] = useState<StackView>(api.emptyView);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [tool, setTool] = useState<Tool>('browse');
  const [error, setError] = useState<string | null>(null);
  // Kept apart from `error` so that "downloaded" is not dressed as a failure.
  const [notice, setNotice] = useState<string | null>(null);
  const [dialog, setDialog] = useState<DialogRequest | null>(null);
  const [ready, setReady] = useState(false);
  const [ai, setAi] = useState<AiView | null>(null);
  const [aiOpen, setAiOpen] = useState(false);
  const [aiSettings, setAiSettings] = useState(false);
  const [map, setMap] = useState<Graph | null>(null);
  // Pictures are fetched by name and kept, because a snapshot arrives after
  // every command and re-encoding a card of artwork each time would be
  // absurd. Cleared when a different stack is opened.
  const [pictures, setPictures] = useState(new Map<string, string>());

  // The two file pickers, hidden until a menu item clicks them.
  const stackPicker = useRef<HTMLInputElement>(null);
  const picturePicker = useRef<HTMLInputElement>(null);
  const pictureLayer = useRef<'card' | 'background'>('card');

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

  const openExample = useCallback(
    (name: string) => {
      setPictures(new Map());
      run(() =>
        fetch(`examples/${encodeURIComponent(name)}.hl.json`)
          .then((response) => {
            if (!response.ok) throw new Error(`could not fetch "${name}"`);
            return response.text();
          })
          .then((text) => api.openStackText(text)),
      );
    },
    [run],
  );

  // Watching for dialogs comes first: a stack whose openStack handler asks a
  // question would otherwise block with nothing listening.
  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    api.onDialog(setDialog).then(
      (unlisten) => {
        if (cancelled) unlisten();
        else stop = unlisten;
      },
      () => {},
    );
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    api
      .ready()
      .then(() => Promise.all([api.aiView().catch(() => null), api.getView()]))
      .then(
        ([aiLoaded, outcome]) => {
          if (cancelled) return;
          setAi(aiLoaded);
          apply(outcome);
          setReady(true);
          // A link from the landing page may name a stack to start in.
          const wanted = new URLSearchParams(window.location.search).get('stack');
          if (wanted !== null && (EXAMPLES as readonly string[]).includes(wanted)) {
            openExample(wanted);
          }
          void api.backendMode().then((mode) => {
            if (mode === 'fallback' && !cancelled) {
              setNotice(
                'Running without cross-origin isolation, so script dialogs use the ' +
                  'browser’s plain prompt instead of the real thing.',
              );
            }
          });
        },
        (reason: unknown) => {
          if (cancelled) return;
          setError(String(reason));
          setReady(true);
        },
      );
    return () => {
      cancelled = true;
    };
  }, [apply, openExample]);

  // Whatever the current card draws, fetched once.
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
        s: () => downloadStack(),
        o: () => stackPicker.current?.click(),
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
  }, [run]);

  function downloadStack() {
    api
      .saveStackText()
      .then(({ name, text }) => {
        download(`${name}.hl.json`, text, 'application/json');
        setNotice(`Downloaded ${name}.hl.json — open it here or in the desktop app`);
        return api.getView();
      })
      .then(apply, (reason: unknown) => setError(String(reason)));
  }

  function openPickedStack(files: FileList | null) {
    const file = files?.[0];
    if (file === undefined) return;
    setPictures(new Map());
    run(() => file.text().then((text) => api.openStackText(text)));
  }

  function importPickedPicture(files: FileList | null) {
    const file = files?.[0];
    if (file === undefined) return;
    run(() =>
      file
        .arrayBuffer()
        .then((buffer) =>
          api.importImageBytes(file.name, pictureLayer.current, new Uint8Array(buffer)),
        ),
    );
  }

  /** The stack as one HTML file, which a browser can hand straight over. */
  function exportWebPage() {
    api.exportWebPage().then(
      ({ source, notes }) => {
        download(`${view.stackName}.html`, source, 'text/html');
        const missing =
          notes.length === 0
            ? ''
            : ` — ${notes.length} thing${notes.length === 1 ? '' : 's'} had no equivalent: ${[...new Set(notes)].join('; ')}`;
        setNotice(`Downloaded ${view.stackName}.html${missing}`);
      },
      (reason: unknown) => setError(String(reason)),
    );
  }

  /** The map as a PNG. Drawn in the window, because only it knows the shape. */
  function saveMap(svg: SVGSVGElement) {
    toPng(svg).then(
      (bytes) => {
        downloadBytes(`${view.stackName} map.png`, bytes);
        setNotice(`Downloaded ${view.stackName} map.png`);
      },
      (reason: unknown) => setError(String(reason)),
    );
  }

  if (!ready) {
    return (
      <div className="notice">
        <h1>HyperLab</h1>
        <p>Waking the runtime up…</p>
      </div>
    );
  }

  const menus: { title: string; entries: MenuEntry[] }[] = [
    {
      title: 'File',
      entries: [
        { label: 'New Stack', run: () => run(() => api.newStack()) },
        {
          label: 'Open Stack…',
          shortcut: '⌘O',
          run: () => stackPicker.current?.click(),
        },
        null,
        { label: 'Download Stack', shortcut: '⌘S', run: downloadStack },
        null,
        { label: 'Export as a Web Page…', run: exportWebPage },
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
        {
          label: 'New Card',
          shortcut: '⌘N',
          run: () => run(() => api.newCard()),
        },
        {
          label: 'Delete Card',
          disabled: view.cardCount <= 1,
          run: () => run(() => api.deleteCard()),
        },
        null,
        {
          label: 'New Button',
          run: () => run(() => api.newPart('button', 'card')),
        },
        {
          label: 'New Field',
          run: () => run(() => api.newPart('field', 'card')),
        },
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
          label: 'Import Picture…',
          run: () => {
            pictureLayer.current = 'card';
            picturePicker.current?.click();
          },
        },
        {
          label: 'Import Background Picture…',
          run: () => {
            pictureLayer.current = 'background';
            picturePicker.current?.click();
          },
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
        {
          label: 'Next Card',
          run: () => run(() => api.goToCard(view.cardNumber + 1)),
        },
        {
          label: 'Last Card',
          run: () => run(() => api.goToCard(view.cardCount)),
        },
        null,
        {
          label: 'Map…',
          run: () =>
            api.stackGraph().then(setMap, (reason: unknown) => setError(String(reason))),
        },
      ],
    },
    {
      title: 'Examples',
      entries: EXAMPLES.map((name) => ({
        label: name,
        run: () => openExample(name),
      })),
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

      <input
        ref={stackPicker}
        type="file"
        accept=".json,application/json"
        style={{ display: 'none' }}
        onChange={(event) => {
          openPickedStack(event.target.files);
          event.target.value = '';
        }}
      />
      <input
        ref={picturePicker}
        type="file"
        accept=".svg,.png,.jpg,.jpeg,.gif,.webp"
        style={{ display: 'none' }}
        onChange={(event) => {
          importPickedPicture(event.target.files);
          event.target.value = '';
        }}
      />
    </div>
  );
}

function isPart(kind: Selection['kind']): boolean {
  return kind === 'button' || kind === 'field' || kind === 'image';
}
