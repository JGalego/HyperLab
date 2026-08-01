/**
 * Films a game of Cluedo being played and then solved.
 *
 * The point of the film is pictures. The board is one drawing with
 * transparent buttons over its rooms, the portraits are image parts with
 * scripts — the portrait *is* the button — and the assistant at the end
 * reads the replies out of the stack and works out who did it.
 *
 * Nothing is faked. The scripts really run, and the deduction the model
 * does at the end is over field text that the game really produced.
 *
 *   GROQ_API_KEY=… apps/desktop/demo/record.sh cluedo
 */

import {
  assistantAvailable,
  beat,
  dismissAnyDialog,
  press,
  roll,
  say,
  shoot,
  write,
} from './kit.mjs';

/** Goes to a card using the navigation on the shared background. */
async function go(page, card) {
  await press(page, `button.part:has-text("${card}")`, { settle: 700 });
}

/** Clicks a portrait or a weapon, which is a picture with a script on it. */
async function choose(page, name) {
  await press(page, `.part.image:has(img[alt="${name}"])`, { settle: 900 });
}

/** Clicks a room on the drawn board. */
async function enter(page, name) {
  await press(page, `button.part[aria-label="${name}"]`, { settle: 700 });
}

/** Makes one suggestion, end to end, and reads the reply. */
async function suggest(page, { suspect, weapon, place }) {
  await go(page, 'Suspects');
  await beat(500);
  await choose(page, suspect);
  await go(page, 'Weapons');
  await beat(500);
  await choose(page, weapon);
  await enter(page, place);
  await press(page, 'button.part:has-text("Ask")', { settle: 1600 });
}

async function main() {
  const withAi = assistantAvailable();
  // Snug around a 640×460 card and the inspector: a window with half a
  // screen of empty desk in it films as a window with nothing in it.
  const { page, finish } = await roll({ size: { width: 1020, height: 640 }, withAi });

  await say(page, 'Cluedo, as a HyperLab stack.', 2400);
  await say(page, 'The board is one drawing, and every room on it is a button.', 3000);
  await enter(page, 'Study');
  await beat(600);

  await say(page, 'The suspects are pictures the stack carries…', 2400);
  await go(page, 'Suspects');
  await say(page, '…and each picture is the button. No invisible one on top.', 3400);
  await choose(page, 'Mrs White');

  await say(page, 'Every weapon too.', 1800);
  await go(page, 'Weapons');
  await beat(1400);
  await choose(page, 'Rope');

  await say(page, 'Ask, and the house says how much of that was right.', 2800);
  await press(page, 'button.part:has-text("Ask")', { settle: 2000 });
  await say(page, 'None of it.', 1800);

  await suggest(page, { suspect: 'Professor Plum', weapon: 'Lead Pipe', place: 'Study' });
  await say(page, 'Two of three — so the room is the one that is wrong.', 3000);

  await enter(page, 'Conservatory');
  await press(page, 'button.part:has-text("Ask")', { settle: 1800 });
  await say(page, 'Three of three.', 1800);

  if (withAi) await assistantAct(page);

  await say(page, 'Accuse.', 1400);
  await press(page, 'button.part:has-text("Accuse")', { settle: 600 });
  await dismissAnyDialog(page, { wait: 8000 });
  await beat(1200);

  await say(page, 'HyperLab — github.com/JGalego/HyperLab', 3000);
  await say(page, '', 400);
  await finish();
}

/**
 * The model reads the game.
 *
 * Worth filming because the replies are ordinary field text: the assistant
 * is not wired into the game, it is looking at the same card you are.
 */
async function assistantAct(page) {
  await say(page, 'The assistant can read the card, like anyone else.', 2600);
  await press(page, '.menu__button:has-text("AI")', { settle: 300 });
  await press(page, '.menu__item:has-text("Show Assistant")', { settle: 900 });

  await write(
    page,
    '.assistant__field',
    'Read the Replies field. Who did it, with what, and where — and how do you know?',
  );
  await page.keyboard.press('Enter');
  await page.waitForSelector('.said--answer', { timeout: 90_000 });
  await beat(4200);

  await say(page, 'And it shows exactly what it was sent.', 2200);
  await press(page, '.said__sent summary', { settle: 2600 });
  await press(page, '.said__sent summary', { settle: 400 });
  await press(page, '.menu__button:has-text("AI")', { settle: 300 });
  await press(page, '.menu__item:has-text("Hide Assistant")', { settle: 800 });
}

shoot(main);
