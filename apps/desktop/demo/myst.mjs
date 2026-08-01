/**
 * Films the Myst stack, and then the map of it.
 *
 * Every one of the eleven places gets its moment, because every one of them
 * is a drawing the bundle carries — and then the film opens one book that
 * nobody labelled, finds itself somewhere with no way back, and **Go ▸ Map**
 * shows that it knew, without running a line of the stack.
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

/**
 * Goes somewhere, stays long enough to look at it, and comes back.
 *
 * The staying is the point. Every place in the stack is a drawing the
 * bundle carries, and a film that clicks straight through them is a film
 * of a menu.
 */
async function visit(page, { to, back, caption, hold = 2400 }) {
  await leave(page, to);
  if (caption) await say(page, caption, hold);
  else await beat(hold);
  await leave(page, back);
}

async function main() {
  // A 600×400 card and the inspector, and no more empty desk than that.
  const { page, finish } = await roll({ size: { width: 960, height: 510 } });

  await say(page, 'Myst, as a HyperLab stack.', 2400);
  await say(page, 'Eleven places, and every picture travels in the bundle.', 3000);
  await leave(page, 'Up to the library');
  await say(page, 'Almost everything on the island runs through here.', 2800);

  const HOME = 'Back to the library';
  await visit(page, {
    to: 'The clock tower',
    back: HOME,
    caption: 'A clock set by two brass wheels down on the shore.',
  });
  await visit(page, {
    to: 'The planetarium',
    back: HOME,
    caption: 'A dome that will show you any date you ask it for.',
  });
  await visit(page, {
    to: 'The generator room',
    back: HOME,
    caption: 'And a needle that has to sit between two marks.',
  });

  await say(page, 'Then four books, each of which is somewhere else.', 2800);
  await leave(page, 'The linking books');
  await beat(900);

  const BOOKS = 'Link home';
  await visit(page, {
    to: 'Channelwood',
    back: BOOKS,
    caption: 'Walkways over water that goes down further than the light.',
  });
  await visit(page, {
    to: 'Mechanical',
    back: BOOKS,
    caption: 'A fortress on a pivot.',
  });
  await visit(page, {
    to: 'Selenitic',
    back: BOOKS,
    caption: 'Craters, and five sounds carried on aerials.',
  });

  await leave(page, 'Stoneship');
  await say(page, 'And a ship in the rock, with a book nobody labelled.', 3000);
  await leave(page, 'Open the unlabelled book', 1600);
  await say(page, 'Now there is no way out. No button does anything.', 3400);

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
