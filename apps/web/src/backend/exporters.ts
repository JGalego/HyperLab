/**
 * The PDF and Decker exporters, fetched the first time they are wanted.
 *
 * They are a WebAssembly module of their own because of what they weigh: an
 * SVG renderer, a font database and a text shaper, which together are most
 * of what HyperLab compiles to. Kept in the main module they were
 * downloaded by everybody, including the people who only wanted to click
 * through Cluedo.
 *
 * So the split is along the line where the cost is: opening a stack loads
 * the runtime, and exporting one loads this. The stack crosses between them
 * as the same single-file JSON it would have been downloaded as, so the two
 * modules share no state.
 */

/** The module, once it has been asked for. */
let loaded: Promise<typeof import('../../wasm-export-pkg/hyperlab_web_export')> | null =
  null;

/** The typeface, handed over once per page rather than once per export. */
let fontGiven = false;

function load() {
  loaded ??= (async () => {
    const [module, { default: wasmUrl }] = await Promise.all([
      import('../../wasm-export-pkg/hyperlab_web_export'),
      import('../../wasm-export-pkg/hyperlab_web_export_bg.wasm?url'),
    ]);
    await module.default({ module_or_path: wasmUrl });
    module.init();
    return module;
  })();
  return loaded;
}

/**
 * Gives the exporters a typeface for the words drawn inside pictures.
 *
 * A browser has none of its own to find, so without this a picture's labels
 * are missing from whatever comes out. A failure is not fatal — the export
 * goes ahead without the labels, which is what a desktop with no fonts
 * would also produce — so this never rejects.
 */
async function withFont(
  module: Awaited<ReturnType<typeof load>>,
  font: () => Promise<Uint8Array>,
): Promise<void> {
  if (fontGiven) return;
  try {
    module.add_font(await font());
    fontGiven = true;
  } catch {
    // Tried once, and a later export is welcome to try again.
  }
}

/** The stack as a PDF, one page per card. */
export async function toPdf(
  stackJson: string,
  font: () => Promise<Uint8Array>,
): Promise<Uint8Array> {
  const module = await load();
  await withFont(module, font);
  return module.to_pdf(stackJson);
}

/** The stack as a Decker deck, plus whatever had no equivalent there. */
export async function toDeck(
  stackJson: string,
  font: () => Promise<Uint8Array>,
): Promise<{ source: string; notes: string[] }> {
  const module = await load();
  await withFont(module, font);
  return JSON.parse(module.to_deck(stackJson)) as {
    source: string;
    notes: string[];
  };
}
