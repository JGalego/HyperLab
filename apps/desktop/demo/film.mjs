/**
 * Films a tour of HyperLab: cards, scripts, undo, and the assistant.
 *
 * Everything here happens to a real `Runtime`: the scripts really run, the
 * assistant really calls a model, and the edits it makes really go through
 * the command bus, which is why the last thing the film does is undo one.
 *
 * Run it through `record.sh`, which starts the two servers it needs and
 * turns the recording into an mp4 and a gif.
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

async function main() {
  // Without a key the HyperTalk half still films. Saying so loudly beats
  // refusing to run for someone who only wanted to see the interface.
  const withAi = assistantAvailable();
  const { page, finish } = await roll({ withAi });

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

  await say(page, 'Pick anything up, and its script is right there.', 2200);
  await press(page, '.statusbar .tool:has-text("Edit")', { settle: 400 });
  await press(page, '.part:has-text("Double It")', { settle: 400 });
  await press(page, '.inspector__tab:has-text("Script")', { settle: 2800 });
  await press(page, '.statusbar .tool:has-text("Browse")', { settle: 600 });

  // ---------------------------------------------------------- the assistant

  if (withAi) await assistantAct(page);

  await say(page, 'HyperLab — github.com/JGalego/HyperLab', 3000);
  await say(page, '', 400);

  await finish();
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
    'Add a button called "Halve" below the other buttons, at left 300 and top 250. ' +
      'Give it a script that halves the first number on every line of the Ingredients field.',
  );
  await page.keyboard.press('Enter');

  await page.waitForSelector('.said--used', { timeout: 90_000 });
  await say(page, 'It works through the same tools a person has…', 2400);
  await page.waitForFunction(
    () => !document.querySelector('.assistant__thinking'),
    null,
    { timeout: 90_000 },
  );
  await beat(1200);
  await press(page, '.said--used summary', { settle: 2600 });

  await say(page, '…and the script it wrote really runs.', 1800);
  await press(page, 'button.part:has-text("Halve")', { settle: 400 });
  // Whether it ends in `answer` is the model's choice, so wait and see.
  await dismissAnyDialog(page);
  await beat(2400);

  // Three commands went in — the button, its script, and the edit it made —
  // so three come out. The middle one is invisible, which is the point: it
  // is the same history a person's edits go into.
  await say(page, 'Every step of that was a command. So every step undoes.', 2400);
  for (const _ of [0, 1, 2]) {
    await page.keyboard.press('Control+z');
    await beat(900);
  }
  await beat(1400);
}

shoot(main);
