/**
 * Films HyperLab using it.
 *
 * Everything here happens to a real `Runtime`: the scripts really run, the
 * assistant really calls a model, and the edits it makes really go through
 * the command bus, which is why the last thing the film does is undo one.
 *
 * Run it through `record.sh`, which starts the two servers it needs and
 * turns the recording into an mp4 and a gif.
 */

import { readFileSync, mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { execSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));

/**
 * Playwright, from wherever it happens to be.
 *
 * It is not a dependency of the application — it is a dependency of filming
 * the application — so it is not in package.json, where it would cost every
 * `npm ci` in CI a browser-sized download for something CI never runs.
 */
const chromium = await (async () => {
  const require = createRequire(import.meta.url);
  const candidates = ['playwright', 'playwright-core'];
  for (const name of candidates) {
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
const APP = process.env.HYPERLAB_APP ?? 'http://127.0.0.1:5173';
const BRIDGE = process.env.HYPERLAB_BRIDGE ?? 'http://127.0.0.1:7878';
const OUT = resolve(HERE, '../../../target/demo');

/** The model to film. Groq speaks the OpenAI protocol, so nothing special. */
const PROVIDER = {
  kind: 'openAiCompatible',
  model: process.env.GROQ_MODEL ?? 'llama-3.3-70b-versatile',
  baseUrl: process.env.GROQ_BASE_URL ?? 'https://api.groq.com/openai/v1',
  apiKeyEnv: 'GROQ_API_KEY',
};

const beat = (ms) => new Promise((wake) => setTimeout(wake, ms));

/** Types like a person rather than pasting like a machine. */
async function write(page, selector, text, delay = 45) {
  await page.click(selector);
  await page.type(selector, text, { delay });
}

/** Moves visibly, then clicks: a jump cut with no travel reads as a glitch. */
async function press(page, selector, { settle = 500 } = {}) {
  const target = page.locator(selector).first();
  await target.waitFor({ state: 'visible' });
  const box = await target.boundingBox();
  if (box)
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, { steps: 18 });
  await beat(180);
  await target.click();
  await beat(settle);
}

/** A line of narration over the film, so it can be watched without sound. */
async function say(page, text, hold = 2200) {
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

async function main() {
  // Without a key the HyperTalk half still films. Saying so loudly beats
  // refusing to run for someone who only wanted to see the interface.
  const withAi = Boolean(process.env.GROQ_API_KEY);
  if (!withAi) {
    console.warn('GROQ_API_KEY is not set — filming without the assistant.');
  }

  mkdirSync(OUT, { recursive: true });

  const browser = await chromium.launch({
    executablePath:
      process.env.CHROMIUM ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome',
  });
  const context = await browser.newContext({
    viewport: { width: 1180, height: 760 },
    deviceScaleFactor: 1,
    recordVideo: { dir: OUT, size: { width: 1180, height: 760 } },
  });

  await context.addInitScript(`window.__HYPERLAB_BRIDGE__ = ${JSON.stringify(BRIDGE)};`);
  await context.addInitScript(readFileSync(resolve(HERE, 'shim.js'), 'utf8'));
  await context.addInitScript(readFileSync(resolve(HERE, 'cursor.js'), 'utf8'));

  const page = await context.newPage();
  page.on('pageerror', (error) => console.error('page error:', error.message));

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

  await page.goto(APP);
  await page.waitForSelector('.card', { timeout: 20_000 });
  await beat(1200);

  // ------------------------------------------------------------- the stack

  await say(page, 'HyperLab — a stack of cards, and everything on it is live.');
  await press(page, '.navigator__label', { settle: 200 });

  await say(page, 'Cards hold buttons and fields. This one is a recipe.', 2400);

  await say(page, 'Next card.', 1200);
  await press(page, 'button.part:has-text("Next")', { settle: 900 });
  await say(page, 'And back.', 1000);
  await press(page, 'button.part:has-text("Previous")', { settle: 900 });

  // ------------------------------------------- a script, and a modal dialog

  await say(page, 'This button runs HyperTalk over the ingredients…', 2200);
  await press(page, 'button.part:has-text("Double")', { settle: 700 });

  await say(page, '…and stops, because a script can wait for a person.', 2600);
  await page.waitForSelector('.dialog', { timeout: 10_000 });
  await beat(800);
  await press(page, '.dialog__buttons .tool:has-text("OK")', { settle: 700 });

  await say(page, 'Every change is a command, so every change undoes.', 2200);
  await page.keyboard.press('Control+z');
  await beat(1400);

  // ------------------------------------------------------- the message box

  await say(page, 'The message box runs a statement against this card.', 2200);
  await write(page, '.statusbar__message', 'put the number of cards & " cards"');
  await page.keyboard.press('Enter');
  await beat(1800);

  // ------------------------------------------------------------ the script

  await say(page, 'Any object shows you its script.', 1800);
  await press(page, 'button.part:has-text("Double")', { settle: 400 });
  await page.waitForSelector('.dialog', { timeout: 10_000 });
  await press(page, '.dialog__buttons .tool:has-text("OK")', { settle: 400 });
  await page.keyboard.press('Control+z');
  await beat(600);

  // ---------------------------------------------------------- the assistant

  if (withAi) await assistantAct(page);

  await say(page, 'HyperLab — github.com/JGalego/HyperLab', 3000);
  await say(page, '', 400);

  await context.close();
  await browser.close();
  console.log(`recorded into ${OUT}`);
}

/** The part of the film that needs a model. */
async function assistantAct(page) {
  await say(page, 'And a model can be asked about it — running on Groq.', 2400);
  await press(page, '.menu__button:has-text("AI")', { settle: 300 });
  await press(page, '.menu__item:has-text("Show Assistant")', { settle: 900 });

  await say(page, 'Ask it what the stack is.', 1600);
  await write(page, '.assistant__field', 'In one sentence, what does this stack do?');
  await page.keyboard.press('Enter');
  await page.waitForSelector('.said--answer', { timeout: 60_000 });
  await beat(3200);

  await say(page, 'It shows you exactly what it sent.', 2000);
  await press(page, '.said__sent summary', { settle: 2400 });
  await press(page, '.said__sent summary', { settle: 400 });

  await say(page, 'Now let it change the stack.', 1800);
  await write(
    page,
    '.assistant__field',
    'Add a button called "Halve" to this card. Give it a script that halves the first number on every line of the Ingredients field.',
  );
  await page.keyboard.press('Enter');

  await page.waitForSelector('.said--used', { timeout: 90_000 });
  await say(page, 'It works through the same tools a person has…', 2600);
  await page.waitForFunction(
    () => !document.querySelector('.assistant__thinking'),
    null,
    {
      timeout: 90_000,
    },
  );
  await beat(1600);

  await say(page, '…so what it did shows up like anyone else’s change…', 2600);
  await press(page, '.said--used summary', { settle: 2200 });

  await say(page, '…and undoes the same way.', 2000);
  await page.keyboard.press('Control+z');
  await beat(2000);
}

main().catch(async (error) => {
  console.error(error);
  // A film that stopped is much easier to explain with a frame of where.
  try {
    const { writeFileSync } = await import('node:fs');
    writeFileSync(resolve(OUT, 'failed.txt'), String(error?.stack ?? error));
  } catch {
    /* nothing more to say */
  }
  process.exit(1);
});
