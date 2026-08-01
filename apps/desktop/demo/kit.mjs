/**
 * The things every film needs.
 *
 * A film is a Playwright script that drives the real interface against a
 * real `Runtime` over `hyperlab-bridge`. This module is the camera and the
 * tripod; the films themselves say what happens.
 */

import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { execSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));

export const APP = process.env.HYPERLAB_APP ?? 'http://127.0.0.1:5173';
export const BRIDGE = process.env.HYPERLAB_BRIDGE ?? 'http://127.0.0.1:7878';
export const OUT = resolve(HERE, '../../../target/demo');

/**
 * The model to film. Groq speaks the OpenAI protocol, so nothing special.
 *
 * `gpt-oss-120b` rather than a Llama: asked to write a script, Llama 3.3 on
 * Groq tends to emit the tool call as prose — `<function(create_button){…}>`
 * — instead of calling the tool, and the turn goes nowhere. Any model that
 * calls tools properly will do.
 */
export const PROVIDER = {
  kind: 'openAiCompatible',
  model: process.env.GROQ_MODEL ?? 'openai/gpt-oss-120b',
  baseUrl: process.env.GROQ_BASE_URL ?? 'https://api.groq.com/openai/v1',
  key: { in: 'environment', name: 'GROQ_API_KEY' },
};

/**
 * Playwright, from wherever it happens to be.
 *
 * It is not a dependency of the application — it is a dependency of filming
 * the application — so it is not in package.json, where it would cost every
 * `npm ci` in CI a browser-sized download for something CI never runs.
 */
const chromium = await (async () => {
  const require = createRequire(import.meta.url);
  for (const name of ['playwright', 'playwright-core']) {
    try {
      return require(name).chromium;
    } catch {
      /* not here; try the next */
    }
  }
  try {
    const global = execSync('npm root -g', { encoding: 'utf8' }).trim();
    return createRequire(`${global}/`)('playwright').chromium;
  } catch {
    throw new Error(
      'playwright is not installed. `npm i -D playwright` here, or `npm i -g playwright`.',
    );
  }
})();

export const beat = (ms) => new Promise((wake) => setTimeout(wake, ms));

/** Types like a person rather than pasting like a machine. */
export async function write(page, selector, text, delay = 45) {
  await page.click(selector);
  await page.type(selector, text, { delay });
}

/** Moves visibly, then clicks: a jump cut with no travel reads as a glitch. */
export async function press(page, selector, { settle = 500 } = {}) {
  const target = page.locator(selector).first();
  await target.waitFor({ state: 'visible' });
  const box = await target.boundingBox();
  if (box) {
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, { steps: 18 });
  }
  await beat(180);
  await target.click();
  await beat(settle);
}

/** A line of narration over the film, so it can be watched without sound. */
export async function say(page, text, hold = 2200) {
  await page.evaluate((words) => {
    let caption = document.querySelector('[data-demo-caption]');
    if (!caption) {
      caption = document.createElement('div');
      caption.setAttribute('data-demo-caption', '');
      caption.style.cssText = [
        'position:fixed',
        'left:50%',
        'bottom:34px',
        'transform:translateX(-50%)',
        'max-width:78%',
        'padding:8px 14px',
        'border:2px solid #000',
        'background:#fff',
        'box-shadow:3px 3px 0 rgba(0,0,0,0.55)',
        'font:13px/1.4 ChicagoFLF, Geneva, Verdana, sans-serif',
        'text-align:center',
        'z-index:2147483646',
        'pointer-events:none',
      ].join(';');
      document.body.append(caption);
    }
    caption.textContent = words;
    caption.style.display = words ? 'block' : 'none';
  }, text);
  await beat(hold);
}

/** Dismisses a modal if a script put one up, and shrugs if it did not. */
export async function dismissAnyDialog(page, { wait = 3000 } = {}) {
  try {
    await page.locator('.dialog').waitFor({ state: 'visible', timeout: wait });
  } catch {
    return false;
  }
  await beat(900);
  await press(page, '.dialog__buttons .tool:has-text("OK")', { settle: 700 });
  return true;
}

/**
 * Opens a recording browser pointed at the interface, with the shim that
 * makes it look like the Tauri window.
 *
 * Returns the page and a `finish` that closes the recording cleanly —
 * Playwright only writes the video out when the context is closed.
 */
export async function roll({ size = { width: 1180, height: 760 }, withAi = false } = {}) {
  mkdirSync(OUT, { recursive: true });

  const browser = await chromium.launch({
    executablePath:
      process.env.CHROMIUM ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome',
  });
  const context = await browser.newContext({
    viewport: size,
    deviceScaleFactor: 1,
    recordVideo: { dir: OUT, size },
  });

  await context.addInitScript(`window.__HYPERLAB_BRIDGE__ = ${JSON.stringify(BRIDGE)};`);
  await context.addInitScript(readFileSync(resolve(HERE, 'shim.js'), 'utf8'));
  await context.addInitScript(readFileSync(resolve(HERE, 'cursor.js'), 'utf8'));

  // Point the assistant at Groq before the window asks what it can use.
  if (withAi) {
    await fetch(`${BRIDGE}/invoke/ai_save_settings`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        settings: { defaultProvider: 'groq', providers: { groq: PROVIDER } },
      }),
    }).then((response) => response.json());
  }

  const page = await context.newPage();
  page.on('pageerror', (error) => console.error('page error:', error.message));
  await page.goto(APP);
  await page.waitForSelector('.card', { timeout: 20_000 });
  await beat(1200);

  return {
    page,
    finish: async () => {
      await context.close();
      await browser.close();
      console.log(`recorded into ${OUT}`);
    },
  };
}

/** Whether there is a key to film the assistant with, and a word if not. */
export function assistantAvailable() {
  const key = Boolean(process.env.GROQ_API_KEY);
  if (!key) {
    console.warn('GROQ_API_KEY is not set — filming without the assistant.');
  }
  return key;
}

/** Runs a film, leaving a note behind if it stops early. */
export function shoot(film) {
  film().catch((error) => {
    console.error(error);
    // A film that stopped is much easier to explain with a note of where.
    try {
      mkdirSync(OUT, { recursive: true });
      writeFileSync(resolve(OUT, 'failed.txt'), String(error?.stack ?? error));
    } catch {
      /* nothing more to say */
    }
    process.exit(1);
  });
}
