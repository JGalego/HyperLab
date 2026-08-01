/**
 * Films the Myst stack, and then the map of it.
 *
 * The point of this film is the last twenty seconds. You walk the island,
 * take a linking book, open one that is not labelled, and find yourself
 * somewhere with no way back — and then **Go ▸ Map** shows that it knew,
 * without running a line of it.
 *
 *   apps/desktop/demo/record.sh myst
 *
 * No model is needed for this one.
 */

import { beat, press, roll, say, shoot } from './kit.mjs';

/** Clicks an exit, which is an ordinary button with the way out written on it. */
async function leave(page, label, settle = 1100) {
  await press(page, `button.part:has-text("${label}")`, { settle });
}

async function main() {
  // A 600×400 card and the inspector, and no more empty desk than that.
  const { page, finish } = await roll({ size: { width: 960, height: 510 } });

  await say(page, 'Myst, as a HyperLab stack.', 2400);
  await say(page, 'Eleven places. Every picture travels inside the bundle.', 3000);
  await leave(page, 'Up to the library');

  await say(page, 'Almost everything on the island runs through the library.', 3000);
  await leave(page, 'The clock tower');
  await beat(600);
  await leave(page, 'Back to the library');

  await say(page, 'And four books, each of which is somewhere else.', 2600);
  await leave(page, 'The linking books');
  await beat(700);
  await leave(page, 'Channelwood');
  await say(page, 'Put your hand on the page and you are there.', 2400);
  await leave(page, 'Link home');

  await leave(page, 'Stoneship');
  await say(page, 'There is a book here that nobody labelled.', 2800);
  await leave(page, 'Open the unlabelled book', 1600);
  await say(page, 'And now there is no way out. No button does anything.', 3400);

  await mapAct(page);

  await say(page, 'HyperLab — github.com/JGalego/HyperLab', 3000);
  await say(page, '', 400);
  await finish();
}

/** The map, which is the reason the stack is shaped like this. */
async function mapAct(page) {
  await say(page, 'The map reads every script. It runs none of them.', 3000);
  await press(page, '.menu__button:has-text("Go")', { settle: 300 });
  await press(page, '.menu__item:has-text("Map…")', { settle: 2200 });

  await say(page, 'Two hubs, four Ages — and one card you cannot leave.', 3600);
  // Hovering a card lifts its routes out of the tangle, which is the only
  // way to follow one place through a busy stack.
  await page.hover('.map__card:has-text("Library")');
  await beat(2000);
  await page.hover('.map__card:has-text("Linking Books")');
  await beat(2000);

  await say(page, 'It found the trap book on its own.', 2800);
  await page.hover('.map__foot .map__jump');
  await beat(2400);
}

shoot(main);
