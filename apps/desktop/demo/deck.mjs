/**
 * Films the slide deck, and then lets it prove its own last claim.
 *
 * Most of the deck is a drawing, a paragraph and the paper to read next, all
 * written in advance. The card before the references is a question box wired
 * to `ask assistant`, so with a model configured the deck about language
 * models is finished by one — and with none configured it says so and keeps
 * working, which is the rule the runtime enforces and the thing worth
 * filming either way.
 *
 *   GROQ_API_KEY=… apps/desktop/demo/record.sh deck
 */

import { assistantAvailable, beat, press, roll, say, shoot, write } from './kit.mjs';

/** Pages forward, and stays long enough to read the slide. */
async function next(page, caption, hold = 3200) {
  await press(page, 'button.part:has-text("Next")', { settle: 700 });
  if (caption) await say(page, caption, hold);
  else await beat(hold);
}

async function main() {
  const withAi = assistantAvailable();
  // Snug around a 640×500 card and the inspector.
  const { page, finish } = await roll({ size: { width: 1020, height: 680 }, withAi });

  await say(page, 'A stack that teaches the thing it is built on.', 2800);
  await beat(1400);

  await next(page, 'Score every token, sample one, do it again.');
  await next(page, 'Which is why it cannot count the r’s in strawberry.');
  await next(page, 'Nothing carries over. It was all sent again.');
  await next(page, 'Temperature reshapes the distribution. It does not add truth.');
  await next(page, 'No index. Every answer is computed.');
  await next(page, 'No magic words — just more context.');
  await next(page, 'And a tool call is text too.');

  await press(page, 'button.part:has-text("Next")', { settle: 900 });
  await askAct(page, withAi);

  await next(page, 'Every claim on the way here, with somewhere to check it.', 3600);

  await say(page, 'HyperLab — github.com/JGalego/HyperLab', 3000);
  await say(page, '', 400);
  await finish();
}

/** The last slide, which is not written down anywhere. */
async function askAct(page, withAi) {
  await say(page, 'Everything so far was written in advance. This is not.', 3400);

  await page.fill('textarea[aria-label="Question"]', '');
  await write(page, 'textarea[aria-label="Question"]', 'What is a token, in one sentence?');
  await beat(700);

  await press(page, 'button.part:has-text("Ask")', { settle: withAi ? 6000 : 1200 });
  await say(
    page,
    withAi
      ? 'A real model, answering into a real field.'
      : 'With no model set up it says so, and the stack keeps working.',
    4000,
  );
}

shoot(main);
