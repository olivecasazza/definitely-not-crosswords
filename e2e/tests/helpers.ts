import type { Locator, Page } from "@playwright/test";

// Human-ish interaction helpers for the demo recording. Everything here is
// pacing/cosmetics ONLY — synchronization still comes from Playwright's
// auto-waiting + web-first assertions, so the jitter adds no flakiness.

/** Random integer in [min, max]. */
export const rand = (min: number, max: number) =>
  Math.floor(min + Math.random() * (max - min + 1));

/** Jittered pause — the "someone is watching/thinking" beat. */
export const dwell = (page: Page, min = 700, max = 1800) =>
  page.waitForTimeout(rand(min, max));

/**
 * Move the mouse to the target through a couple of waypoints with a slight
 * overshoot-and-settle, the way a real hand lands on an element.
 */
export async function humanMove(page: Page, target: Locator) {
  const box = await target.boundingBox();
  if (!box) return;
  const destX = box.x + box.width * (0.35 + Math.random() * 0.3);
  const destY = box.y + box.height * (0.35 + Math.random() * 0.3);
  // Approach with a small offset, then settle onto the final point.
  await page.mouse.move(destX + rand(-36, 36), destY + rand(-24, 24), {
    steps: rand(4, 8),
  });
  await page.waitForTimeout(rand(60, 180));
  await page.mouse.move(destX, destY, { steps: rand(2, 4) });
}

/** Scroll, drift over, settle, click. */
export async function humanClick(page: Page, target: Locator) {
  await target.scrollIntoViewIfNeeded();
  await humanMove(page, target);
  await page.waitForTimeout(rand(90, 280)); // finger settling before the press
  await target.click();
}

/**
 * Type with a per-keystroke cadence and the occasional mid-word hesitation.
 * Focuses the field with a human click first.
 */
export async function humanType(page: Page, target: Locator, text: string) {
  await humanClick(page, target);
  for (const ch of text) {
    await page.keyboard.type(ch);
    let delay = rand(55, 190);
    if (Math.random() < 0.08) delay += rand(250, 650); // brief think
    await page.waitForTimeout(delay);
  }
}

/**
 * Type letters one-by-one into an already-focused input (the crossword clue
 * inputs auto-advance, so we never click between letters).
 */
export async function humanTypeLetters(page: Page, letters: string) {
  for (const ch of letters) {
    await page.keyboard.type(ch.toUpperCase());
    let delay = rand(70, 210);
    if (Math.random() < 0.06) delay += rand(200, 500);
    await page.waitForTimeout(delay);
  }
}

/** Idle mouse drift so the recording never feels scripted between actions. */
export async function wander(page: Page, moves = rand(1, 3)) {
  for (let i = 0; i < moves; i++) {
    await page.mouse.move(rand(240, 1680), rand(160, 940), {
      steps: rand(4, 9),
    });
    await page.waitForTimeout(rand(180, 650));
  }
}
