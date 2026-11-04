// The screenshots and the demo GIF in the README, in English, against the mock.
// Needs `bun run dev` running and ffmpeg for the GIF.
//
//   bun scripts/readme-media.ts [outDir]     (default: ../docs/screenshots)
import { chromium, type Browser, type Page } from 'playwright';
import { mkdtempSync, readdirSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const BASE = process.env.BM_URL ?? 'http://localhost:5173';
const out = resolve(process.argv[2] ?? join(import.meta.dir, '../../docs/screenshots'));
const viewport = { width: 1400, height: 880 };

const browser = await chromium.launch();
const errors: string[] = [];

async function context(b: Browser, video?: string) {
  const ctx = await b.newContext({
    viewport,
    deviceScaleFactor: video ? 1 : 2,
    colorScheme: 'dark',
    locale: 'en-US',
    ...(video ? { recordVideo: { dir: video, size: viewport } } : {}),
  });
  await ctx.addInitScript(() => {
    localStorage.setItem('bm.seenVersion', '99.0.0');
    localStorage.setItem('bm.tourSeen', '1');
  });
  return ctx;
}

async function unlock(page: Page, slowly = false) {
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto(BASE);
  await page.getByPlaceholder('Master password').pressSequentially('demo1234', { delay: slowly ? 70 : 0 });
  await page.getByRole('button', { name: 'Unlock', exact: true }).click();
  await page.getByText(/entries ·/).waitFor();
}

async function open(page: Page, title: string, slowly = false) {
  await page.keyboard.press('Meta+k');
  await page.getByPlaceholder('Search, or create a new entry …').pressSequentially(title, { delay: slowly ? 80 : 0 });
  // Other entries can mention the title and rank first, so pick the one named so.
  const hit = page.getByRole('dialog', { name: 'Search' }).getByRole('button').filter({ hasText: new RegExp(`^\\s*${title}`) }).first();
  await hit.waitFor();
  await page.waitForTimeout(slowly ? 700 : 300);
  await hit.click();
  await page.getByLabel('Title').waitFor();
  await page.waitForFunction((t) => (document.querySelector('input[aria-label="Title"]') as HTMLInputElement | null)?.value.startsWith(t), title);
}

const aiCapture = (page: Page, title: string, near: string) =>
  page.evaluate(([t, n]) => (globalThis as unknown as { mockAiCapture(t: string, n: string): void }).mockAiCapture(t, n), [title, near]);

const shot = (page: Page, name: string) => page.screenshot({ path: `${out}/${name}.png` });

// Screenshots.
{
  const ctx = await context(browser);
  const page = await ctx.newPage();
  try {
    await unlock(page);
    await page.waitForTimeout(5000);

    // An AI writes while you watch.
    await aiCapture(page, 'Session summary', 'Vault');
    await page.getByRole('status').filter({ hasText: 'Claude Code' }).waitFor();
    await page.locator('.pulse-ring').first().waitFor();
    await page.waitForTimeout(450);
    await shot(page, 'graph');
    await page.locator('.pulse-ring').first().waitFor({ state: 'detached', timeout: 6000 });

    // Suggestions need the model.
    await page.keyboard.press('Meta+,');
    const settings = page.getByRole('dialog', { name: 'Settings' });
    await settings.getByRole('button', { name: 'Search by meaning', exact: true }).click();
    await settings.getByRole('button', { name: 'Download', exact: true }).click();
    await settings.getByRole('button', { name: 'Remove model' }).waitFor({ timeout: 20000 });

    await settings.getByRole('button', { name: 'AI connections', exact: true }).click();
    await page.getByText('Server running').waitFor();
    await page.getByRole('button', { name: 'Connect AI' }).click();
    await page.getByRole('button', { name: /Claude Desktop/ }).click();
    await page.waitForTimeout(300);
    await shot(page, 'ai-connections');
    await page.keyboard.press('Escape');
    await page.waitForTimeout(400);
    if (await settings.count()) await page.keyboard.press('Escape');
    await page.waitForTimeout(400);

    await open(page, 'Sourdough');
    await page.getByText('Suggestions').waitFor();
    await page.getByText('Duplicate?').waitFor();
    await page.waitForTimeout(800);
    await shot(page, 'suggestions');
    await page.getByLabel('Close').first().click();

    // Writing a documentation page, with the slash menu.
    await page.getByRole('navigation').getByRole('button', { name: /^Brainmatter/ }).first().click();
    await page.getByRole('radio', { name: 'Docs' }).click();
    await page.getByRole('heading', { name: 'Handbook' }).waitFor();
    await page.getByRole('button', { name: 'Edit' }).click();
    const editor = page.locator('main .cm-content');
    await editor.waitFor();
    await editor.click();
    await page.keyboard.press('Meta+ArrowDown');
    await page.keyboard.type('\n\n/tab');
    await page.locator('.cm-tooltip-autocomplete').getByText('Table').waitFor();
    await page.waitForTimeout(1500);
    await shot(page, 'docs-slash');
    await page.keyboard.press('Escape');
    await page.getByRole('radio', { name: 'Graph' }).click().catch(() => {});

    // The light theme.
    await page.keyboard.press('Meta+,');
    await settings.getByRole('button', { name: 'General', exact: true }).click();
    await settings.getByRole('radio', { name: 'Light' }).click();
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: 'All', exact: true }).first().click();
    await page.waitForTimeout(600);
    await page.getByRole('button', { name: 'Show everything' }).click();
    await open(page, 'Brainmatter');
    await page.waitForTimeout(800);
    await shot(page, 'light');
  } catch (e) {
    await shot(page, 'zz-failure');
    errors.push(`screenshots: ${(e as Error).message.split('\n')[0]}`);
  }
  await ctx.close();
}

// The demo: unlock, find an entry, watch an AI add one, see what it did.
{
  const videos = mkdtempSync(join(tmpdir(), 'bm-demo-'));
  const ctx = await context(browser, videos);
  const page = await ctx.newPage();
  try {
    await unlock(page, true);
    await page.waitForTimeout(3200);
    await open(page, 'Sigma', true);
    await page.waitForTimeout(2200);
    await page.getByLabel('Close').first().click();
    await page.waitForTimeout(800);
    await aiCapture(page, 'Session summary', 'Vault');
    await page.waitForTimeout(2600);
    await page.getByRole('button', { name: 'Activity', exact: true }).click();
    await page.waitForTimeout(2600);
    await page.getByRole('button', { name: 'Activity', exact: true }).click();
    await page.waitForTimeout(600);
    await page.getByRole('navigation').getByRole('button', { name: /^Core/ }).first().click();
    await page.waitForTimeout(2200);
    await page.getByRole('button', { name: 'All', exact: true }).first().click();
    await page.waitForTimeout(1800);
  } catch (e) {
    errors.push(`demo: ${(e as Error).message.split('\n')[0]}`);
  }
  await ctx.close();
  const webm = readdirSync(videos).find((f) => f.endsWith('.webm'));
  if (webm) {
    const gif = `${out}/demo.gif`;
    // Skip the blank first frames, then a palette made for this clip keeps the colours clean.
    const ff = Bun.spawnSync([
      'ffmpeg', '-y', '-loglevel', 'error', '-ss', '0.6', '-i', join(videos, webm),
      '-vf', 'fps=12,scale=960:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle',
      gif,
    ]);
    if (ff.exitCode !== 0) errors.push(`ffmpeg: ${ff.stderr.toString()}`);
    else console.log(`demo.gif: ${(statSync(gif).size / 1e6).toFixed(1)} MB`);
  }
  rmSync(videos, { recursive: true, force: true });
}

await browser.close();
console.log(errors.length ? `ERRORS:\n${errors.join('\n')}` : 'no page errors');
console.log(`written to ${out}`);
