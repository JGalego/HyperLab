/**
 * The application, as a page.
 *
 * A sibling of the desktop's `App.tsx` that renders the same components —
 * the card, the inspector, the assistant, the map — over the WebAssembly
 * runtime. What is different is exactly what a browser does differently:
 * files are uploads and downloads, pictures arrive from a file picker, and
 * the example stacks are fetched from the site itself.
 */

import { useCallback, useEffect, useRef, useState, type CSSProperties } from 'react';

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
  'Language Models, Explained',
  'Myst',
  'Recipe Box',
  'Todo',
] as const;

/**
 * The typeface the exporters draw a picture's words with, fetched the first
 * time one runs.
 *
 * A page cannot read the machine's fonts, so words drawn inside a stack's
 * artwork would come out of a PDF or a deck missing. Fetched only when
 * somebody actually exports, so the 400 KB never lands on an ordinary
 * visit, and kept afterwards.
 */
let fontBytes: Promise<Uint8Array> | null = null;

function theFont(): Promise<Uint8Array> {
  fontBytes ??= fetch('fonts/LiberationSans-Regular.ttf')
    .then((response) => {
      if (!response.ok) throw new Error(`${response.status}`);
      return response.arrayBuffer();
    })
    .then((buffer) => new Uint8Array(buffer))
    .catch((reason: unknown) => {
      // Tried once; a later export should try again rather than inherit
      // this failure for the life of the page.
      fontBytes = null;
      throw reason;
    });
  return fontBytes;
}

/**
 * Below this the interface stacks and the panels become sheets. Matches the
 * breakpoint in `mobile.css`; they have to agree.
 *
 * 60rem, because the widest card a stack ships with is 640px and the
 * inspector holds 290 beside it — so anything under about 960 cannot show
 * the two side by side, landscape phones very much included.
 */
const NARROW = '(max-width: 60rem)';

/**
 * Whether the screen is narrow enough for the stacked layout.
 *
 * Watched rather than read once: a phone that turns on its side is a
 * different layout, and so is a desktop window dragged narrow.
 */
function useNarrowScreen(): boolean {
  const [narrow, setNarrow] = useState(
    () => typeof window !== 'undefined' && window.matchMedia(NARROW).matches,
  );
  useEffect(() => {
    const query = window.matchMedia(NARROW);
    const update = () => setNarrow(query.matches);
    query.addEventListener('change', update);
    return () => query.removeEventListener('change', update);
  }, []);
  return narrow;
}

/**
 * How much the card has to shrink to fit the room it has.
 *
 * The stage is measured rather than the window, because the window is not
 * what the card sits in: the menu bar, the status bar and any open sheet
 * have all taken their share first. A landscape phone is the case that
 * makes this matter — plenty of width, almost no height.
 *
 * Only ever shrinks. A card smaller than the screen is drawn at its own
 * size, because artwork drawn one bit deep looks wrong blown up. The answer
 * goes to CSS as `--card-zoom`, and `Part.tsx` measures the same scale back
 * off the element when a drag starts.
 */
function useCardZoom(
  stage: HTMLDivElement | null,
  card: { width: number; height: number },
  active: boolean,
): number {
  const [room, setRoom] = useState({ width: 0, height: 0 });

  useEffect(() => {
    // Taken as state rather than through a ref, because the stage does not
    // exist on the first render — the window shows a notice until the
    // runtime is up — and a ref would still be empty when the effect ran.
    if (stage === null) return undefined;
    const watch = new ResizeObserver(([entry]) => {
      if (entry === undefined) return;
      const { width, height } = entry.contentRect;
      setRoom((was) =>
        was.width === width && was.height === height ? was : { width, height },
      );
    });
    watch.observe(stage);
    return () => watch.disconnect();
  }, [stage]);

  if (!active || card.width <= 0 || card.height <= 0) return 1;
  if (room.width === 0 || room.height === 0) return 1;
  // The hard shadow the card casts needs a few pixels of its own.
  const margin = 6;
  return Math.min(
    1,
    Math.max(0.2, (room.width - margin) / card.width),
    Math.max(0.2, (room.height - margin) / card.height),
  );
}

/** Hands the user a file, which is what "save" means on a page. */
function download(name: string, text: string, type: string) {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = name;
  anchor.click();
  URL.revokeObjectURL(url);
}

function downloadBytes(name: string, bytes: Uint8Array, type: string) {
  const url = URL.createObjectURL(new Blob([bytes as unknown as BlobPart], { type }));
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = name;
  anchor.click();
  URL.revokeObjectURL(url);
}

/**
 * What to add to a notice about a translation that lost something.
 *
 * A destination that came across whole and one that dropped a handler both
 * wrote a file, and the difference is worth saying out loud.
 */
function leftBehind(notes: string[]): string {
  if (notes.length === 0) return '';
  const each = notes.length === 1 ? 'thing' : 'things';
  return ` — ${notes.length} ${each} had no equivalent: ${[...new Set(notes)].join('; ')}`;
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
  const narrow = useNarrowScreen();
  const [stage, setStage] = useState<HTMLDivElement | null>(null);
  const cardZoom = useCardZoom(stage, view.cardSize, narrow);
  // On a phone the inspector is a sheet over half the screen, so it starts
  // out of the way and is asked for. On a desktop it is a column, always
  // there, and this is ignored.
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const showInspector = narrow ? inspectorOpen : true;

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
                'Running without cross-origin isolation; script dialogs use the browser’s plain prompt.',
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
        setNotice(`Downloaded ${name}.hl.json — it also opens in the desktop app`);
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
        setNotice(`Downloaded ${view.stackName}.html${leftBehind(notes)}`);
      },
      (reason: unknown) => setError(String(reason)),
    );
  }

  /**
   * The whole stack as a PDF, one page per card.
   *
   * The exporters are a WebAssembly module of their own, fetched here on
   * first use, so the notice goes up before the wait rather than after it.
   */
  function exportPdf() {
    setNotice(`Exporting ${view.stackName}.pdf…`);
    api.exportPdf(theFont).then(
      (bytes) => {
        downloadBytes(`${view.stackName}.pdf`, bytes, 'application/pdf');
        setNotice(`Downloaded ${view.stackName}.pdf`);
      },
      (reason: unknown) => setError(String(reason)),
    );
  }

  /** The stack as a Decker deck. */
  function exportDeck() {
    setNotice(`Exporting ${view.stackName}.deck…`);
    api.exportDeck(theFont).then(
      ({ source, notes }) => {
        download(`${view.stackName}.deck`, source, 'text/plain');
        setNotice(`Downloaded ${view.stackName}.deck${leftBehind(notes)}`);
      },
      (reason: unknown) => setError(String(reason)),
    );
  }

  /** The map as a PNG. Drawn in the window, because only it knows the shape. */
  function saveMap(svg: SVGSVGElement) {
    toPng(svg).then(
      (bytes) => {
        downloadBytes(`${view.stackName} map.png`, bytes, 'image/png');
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
        { label: 'Export as PDF…', run: exportPdf },
        { label: 'Export as a Web Page…', run: exportWebPage },
        { label: 'Export as a Decker Deck…', run: exportDeck },
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
    ...(narrow
      ? [
          {
            title: 'View',
            entries: [
              {
                label: inspectorOpen ? 'Hide Inspector' : 'Show Inspector',
                run: () => setInspectorOpen((open) => !open),
              },
            ] as MenuEntry[],
          },
        ]
      : []),
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
    <div className="app" style={{ '--card-zoom': cardZoom } as CSSProperties}>
      <MenuBar view={view} menus={menus} />

      <div className="app__body">
        <div className="app__stage" ref={setStage}>
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

        {showInspector && (
          <Inspector
            view={view}
            selection={selection}
            onSelect={setSelection}
            onSetProperty={(kind, id, property, value) =>
              run(() => api.setProperty(kind, id, property, value))
            }
            onSetScript={(kind, id, script) => run(() => api.setScript(kind, id, script))}
          />
        )}
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
