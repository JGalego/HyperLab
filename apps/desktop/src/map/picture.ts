/**
 * The map, as a PNG.
 *
 * The map is the one thing HyperLab draws that the core cannot: its shape is a
 * layout [`layout.ts`](./layout.ts) worked out from the graph, and only the
 * window has it. So the picture is made here and the shell only saves it.
 *
 * An SVG in the document is not a picture on its own — it is styled by the
 * page's stylesheet, and cut from it, it comes out unstyled. So the rules that
 * dress it travel with it.
 */

/** How much bigger than the screen the saved picture is. */
const SCALE = 2;

/**
 * Every CSS custom property the theme defines, with the values in force.
 *
 * Read from the stylesheet rather than guessed: `getComputedStyle` will resolve
 * `--ink` for us, but it will not say which properties exist, and a hard-coded
 * list goes stale the first time the theme gains a colour.
 */
function themeVariables(): string {
  const root = getComputedStyle(document.documentElement);
  const names = new Set<string>();

  for (const sheet of Array.from(document.styleSheets)) {
    let rules: CSSRuleList;
    try {
      rules = sheet.cssRules;
    } catch {
      continue; // A stylesheet from elsewhere; there are none, but asking is free.
    }
    for (const rule of Array.from(rules)) {
      if (!(rule instanceof CSSStyleRule)) continue;
      for (const property of Array.from(rule.style)) {
        if (property.startsWith('--')) names.add(property);
      }
    }
  }

  const declared = Array.from(names)
    .map((name) => `${name}:${root.getPropertyValue(name).trim()}`)
    .filter((pair) => !pair.endsWith(':'))
    .join(';');
  return `:root{${declared}}`;
}

/** The rules that dress the map, copied out of the page's stylesheet. */
function mapRules(): string {
  const wanted: string[] = [];
  for (const sheet of Array.from(document.styleSheets)) {
    let rules: CSSRuleList;
    try {
      rules = sheet.cssRules;
    } catch {
      continue;
    }
    for (const rule of Array.from(rules)) {
      if (rule instanceof CSSStyleRule && rule.selectorText.includes('map__')) {
        wanted.push(rule.cssText);
      }
    }
  }
  return wanted.join('\n');
}

/** The map's `<svg>`, standing on its own, with its styling attached. */
function standalone(svg: SVGSVGElement): string {
  const copy = svg.cloneNode(true) as SVGSVGElement;
  const box = svg.viewBox.baseVal;
  copy.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
  copy.setAttribute('width', String(box.width));
  copy.setAttribute('height', String(box.height));

  const style = document.createElementNS('http://www.w3.org/2000/svg', 'style');
  // A white ground, because an SVG has none and a transparent PNG of black
  // lines is invisible in every dark viewer it lands in.
  style.textContent = `${themeVariables()}\nsvg{background:var(--paper,#fff)}\n${mapRules()}`;
  copy.insertBefore(style, copy.firstChild);

  return new XMLSerializer().serializeToString(copy);
}

/**
 * Draws the map into a PNG.
 *
 * Resolves to the file's bytes, ready to be handed to the shell to write.
 */
export async function toPng(svg: SVGSVGElement): Promise<Uint8Array> {
  const box = svg.viewBox.baseVal;
  const source = standalone(svg);

  // A `data:` URI rather than a `blob:` one: the shipped app runs under a
  // Content-Security-Policy of `img-src 'self' data:`, and a blob URL is
  // neither. Encoded as base64 because the markup is full of `#` and `"`.
  const encoded = btoa(unescape(encodeURIComponent(source)));
  const drawing = new Image();
  drawing.width = box.width;
  drawing.height = box.height;

  await new Promise<void>((done, fail) => {
    drawing.onload = () => done();
    drawing.onerror = () => fail(new Error('the map could not be drawn'));
    drawing.src = `data:image/svg+xml;base64,${encoded}`;
  });

  const canvas = document.createElement('canvas');
  canvas.width = Math.round(box.width * SCALE);
  canvas.height = Math.round(box.height * SCALE);
  const paint = canvas.getContext('2d');
  if (paint === null) throw new Error('this window has no canvas to draw on');
  paint.scale(SCALE, SCALE);
  paint.drawImage(drawing, 0, 0, box.width, box.height);

  const blob = await new Promise<Blob | null>((done) => canvas.toBlob(done, 'image/png'));
  if (blob === null) throw new Error('the picture came out empty');
  return new Uint8Array(await blob.arrayBuffer());
}
