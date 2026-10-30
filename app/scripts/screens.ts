// Clicks through the main screens against the mock backend and saves
// screenshots. Needs `bun run dev` running.
//
//   bun scripts/screens.ts [outDir]
import { chromium, type Page } from 'playwright';

const BASE = process.env.BM_URL ?? 'http://localhost:5173';
const out = process.argv[2] ?? '/tmp/bm-shots';

const browser = await chromium.launch();
const context = await browser.newContext({ viewport: { width: 1400, height: 880 }, deviceScaleFactor: 2, colorScheme: 'dark' });
const errors: string[] = [];

async function open(path: string): Promise<Page> {
  const page = await context.newPage();
  page.on('pageerror', (e) => errors.push(`${path}: ${e.message}`));
  page.on('console', (m) => m.type() === 'error' && errors.push(`${path}: ${m.text()}`));
  await page.goto(`${BASE}${path}`);
  return page;
}

const shot = (page: Page, name: string) => page.screenshot({ path: `${out}/${name}.png` });

// Onboarding flow on an empty vault.
{
  const page = await open('/?mock=fresh');
  await page.getByText('Willkommen bei Brainmatter').waitFor();
  await page.getByPlaceholder('z. B. Lasse').fill('Lasse');
  await page.locator('input[type=password]').nth(0).fill('Korrekt-Pferd-9!');
  await page.locator('input[type=password]').nth(1).fill('Korrekt-Pferd-9!');
  await shot(page, '01-onboarding');
  await page.getByRole('button', { name: 'Tresor erstellen' }).click();
  await page.getByText('Wiederherstellungsschlüssel').first().waitFor();
  await shot(page, '02-recovery-key');
  await page.getByRole('checkbox').check();
  await page.getByRole('button', { name: 'Weiter' }).click();
  await page.getByRole('button', { name: 'Später' }).click();
  await page.getByText('Dein Graph ist noch leer').waitFor();
  await shot(page, '03-empty-workspace');
  await page.close();
}

// Lock screen: the mock's Touch ID "cancels", so the password form stays.
const page = await open('/');
try {
  await page.getByText('Brainmatter ist gesperrt').waitFor();
  await page.waitForTimeout(300);
  await shot(page, '04-lock-screen');
  await page.getByPlaceholder('Master-Passwort').fill('falsch');
  await page.getByRole('button', { name: 'Entsperren', exact: true }).click();
  await page.getByText('Falsches Passwort').waitFor();
  await page.getByPlaceholder('Master-Passwort').fill('demo1234');
  await page.getByRole('button', { name: 'Entsperren', exact: true }).click();

  // Workspace with demo data; give the layout time to settle.
  await page.getByText('Einträge ·').waitFor();
  await page.waitForTimeout(5000);
  await shot(page, '05-graph');

  await page.keyboard.press('Meta+k');
  await page.getByPlaceholder('Suchen oder neuen Eintrag anlegen …').fill('sigma');
  await page.waitForTimeout(300);
  await shot(page, '06-palette');
  await page.keyboard.press('Escape');

  await page.keyboard.press('Meta+k');
  await page.getByPlaceholder('Suchen oder neuen Eintrag anlegen …').fill('Brainmatter');
  await page.waitForTimeout(300);
  await page.keyboard.press('Enter');
  await page.getByText('Verbindungen', { exact: true }).waitFor();
  await page.waitForTimeout(800);
  await shot(page, '07-inspector');

  await page.getByTitle('Bearbeiten').click();
  await page.locator('.cm-content').click();
  await page.keyboard.press('Meta+ArrowDown');
  await page.keyboard.type('\n\nSiehe auch [[Tok');
  await page.waitForTimeout(500);
  await shot(page, '08-editor-autocomplete');

  await page.getByRole('button', { name: /Kern/ }).first().click();
  await page.waitForTimeout(2500);
  await shot(page, '09-project-view');

  await page.getByTitle('Aktivität').click();
  await page.getByText('Demo-Daten angelegt').waitFor();
  await shot(page, '10-activity');

  await page.keyboard.press('Meta+,');
  await page.getByText('Automatisch sperren nach').waitFor();
  await shot(page, '11-settings');
} catch (e) {
  await shot(page, 'zz-failure');
  errors.push(`step failed: ${(e as Error).message.split('\n')[0]}`);
  errors.push(`visible text: ${(await page.locator('body').innerText()).slice(0, 600).replace(/\n+/g, ' | ')}`);
}

await browser.close();
console.log(errors.length ? `ERRORS:\n${errors.join('\n')}` : 'no page errors');
console.log(`screenshots in ${out}`);
