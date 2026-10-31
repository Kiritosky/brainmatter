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
  await page.getByText('Dein Wiederherstellungsschlüssel').waitFor();
  await shot(page, '02-recovery-key');
  await page.getByText('Ich habe den Schlüssel').click();
  await page.getByRole('button', { name: 'Weiter' }).click();
  await page.getByRole('button', { name: 'Später' }).click();
  await page.getByText('Dein Graph ist noch leer').waitFor();
  await shot(page, '03-empty-workspace');
  await page.close();
}

// Lock screen: the mock's Touch ID "cancels", so the password form stays.
const page = await open('/');
try {
  await page.getByText('Gesperrt').waitFor();
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

  // Regression: switching entries within the autosave delay must save the
  // edit to the entry it was made in, not to the next one.
  const openEntry = async (title: string) => {
    await page.keyboard.press('Meta+k');
    await page.getByPlaceholder('Suchen oder neuen Eintrag anlegen …').fill(title);
    await page.waitForTimeout(150);
    await page.keyboard.press('Enter');
    await page.locator('input[aria-label="Titel"]').and(page.locator(`[value="${title}"]`)).or(page.getByLabel('Titel')).first().waitFor();
    await page.waitForTimeout(200);
  };
  await openEntry('Marathon-Plan');
  await page.getByLabel('Bearbeiten').click();
  await page.locator('.cm-content').click();
  await page.keyboard.press('Meta+ArrowDown');
  await page.keyboard.type(' ZZMARKER');
  await page.keyboard.press('Meta+k');
  await page.getByPlaceholder('Suchen oder neuen Eintrag anlegen …').fill('Roggenbrot');
  await page.waitForTimeout(120);
  await page.keyboard.press('Enter');
  await page.waitForTimeout(1500);
  const bodies = await page.evaluate(async () => {
    const invoke = (window as any).__TAURI_INTERNALS__.invoke;
    const hits: any[] = await invoke('search', { query: { text: '', kinds: [], tags: [], limit: 50, include_subprojects: true } });
    const body = async (title: string) => (await invoke('node_detail', { id: hits.find((h) => h.node.title === title).node.id })).node.body as string;
    return { edited: await body('Marathon-Plan'), other: await body('Roggenbrot') };
  });
  if (!bodies.edited.includes('ZZMARKER')) errors.push(`edit lost: ${bodies.edited}`);
  if (bodies.other.includes('ZZMARKER')) errors.push(`edit landed in the wrong entry: ${bodies.other}`);
  console.log('autosave on switch: ok =', bodies.edited.includes('ZZMARKER') && !bodies.other.includes('ZZMARKER'));
  await page.getByLabel('Schließen').first().click();

  await page.getByRole('button', { name: /Kern/ }).first().click();
  await page.waitForTimeout(2500);
  await shot(page, '09-project-view');

  await page.getByTitle('Aktivität').click();
  await page.getByText('Demo-Daten angelegt').waitFor();
  await page.waitForTimeout(500);
  await shot(page, '10-activity');
  await page.getByRole('radio', { name: 'KI' }).click();
  await page.waitForTimeout(400);
  await page.locator('article').first().hover();
  await page.waitForTimeout(250);
  await shot(page, '10b-activity-ai');

  await page.keyboard.press('Meta+,');
  await page.getByText('Automatisch sperren', { exact: true }).waitFor();
  await page.waitForTimeout(300);
  await shot(page, '11-settings');
  await page.getByRole('button', { name: 'Automatisch sperren nach' }).click();
  await page.waitForTimeout(250);
  await shot(page, '12-settings-select');
  await page.keyboard.press('Escape');

  await page.getByRole('button', { name: 'Suche nach Sinn' }).click();
  await page.getByRole('button', { name: 'Laden', exact: true }).waitFor();
  await page.waitForTimeout(300);
  await shot(page, '18-model-missing');
  await page.getByRole('button', { name: 'Laden', exact: true }).click();
  await page.getByText('Wird geladen').waitFor();
  await page.waitForTimeout(500);
  await shot(page, '19-model-downloading');
  await page.getByRole('button', { name: 'Modell entfernen' }).waitFor();
  await page.waitForTimeout(400);
  await shot(page, '20-model-ready');
  await page.keyboard.press('Escape');

  // With the model: suggestions in the inspector, ghost edges in the graph.
  await page.getByRole('button', { name: 'Alles', exact: true }).first().click();
  await page.waitForTimeout(800);
  await page.keyboard.press('Meta+k');
  await page.getByPlaceholder('Suchen oder neuen Eintrag anlegen …').fill('Sauerteig');
  await page.waitForTimeout(200);
  await page.keyboard.press('Enter');
  await page.getByText('Vorschläge').waitFor();
  await page.getByText('Duplikat?').waitFor();
  await page.waitForTimeout(800);
  await shot(page, '21-suggestions');
  await page.getByText('Sauerteig ansetzen').first().hover();
  await page.waitForTimeout(200);
  await shot(page, '22-suggestion-actions');
  await page.getByLabel('Hierher zusammenführen').click();
  await page.getByRole('button', { name: 'Zusammenführen?' }).click();
  await page.getByText('wurde hierher verschoben').waitFor();
  await page.waitForTimeout(400);
  const merged = await page.evaluate(async () => {
    const invoke = (window as any).__TAURI_INTERNALS__.invoke;
    const hits: any[] = await invoke('search', { query: { text: 'Sauerteig ansetzen', kinds: [], tags: [], limit: 50, include_subprojects: true } });
    return hits.some((h) => h.node.title === 'Sauerteig ansetzen');
  });
  if (merged) errors.push('merged entry is still there');
  await page.getByLabel('Schließen').first().click();

  await page.getByRole('radio', { name: 'Thema' }).click();
  await page.waitForTimeout(500);
  await shot(page, '23-graph-themes');
  await page.getByRole('radio', { name: 'Projekt' }).click();

  await page.getByRole('button', { name: 'Prüfen' }).click();
  await page.getByText('Hybrid-Suche').first().waitFor();
  await page.waitForTimeout(400);
  await shot(page, '24-review');
  await page.getByRole('radio', { name: 'Unverbunden' }).click();
  await page.waitForTimeout(400);
  await shot(page, '25-review-orphans');
  await page.getByRole('radio', { name: 'Ungeprüft' }).click();
  await page.locator('aside').getByText('Hybrid-Suche').hover();
  await page.getByLabel('Als geprüft markieren').click();
  await page.getByText('Alles geprüft').waitFor();
  await page.waitForTimeout(400);
  await shot(page, '26-review-empty');

  await page.keyboard.press('Meta+,');
  await page.getByRole('button', { name: 'KI-Verbindungen' }).click();
  await page.getByText('Server läuft').waitFor();
  await page.waitForTimeout(300);
  await shot(page, '13-ai-clients');
  await page.getByRole('button', { name: 'KI verbinden' }).click();
  await page.getByRole('button', { name: /Claude Desktop/ }).click();
  await page.waitForTimeout(300);
  await shot(page, '14-ai-new');
  await page.getByRole('button', { name: 'Automatisch einrichten' }).click();
  await page.getByText('Claude Desktop ist verbunden').waitFor();
  await page.waitForTimeout(350);
  await shot(page, '15-ai-connected');
  await page.getByRole('button', { name: 'Fertig' }).click();
  await page.getByRole('button', { name: 'KI verbinden' }).click();
  await page.getByRole('button', { name: /Codex/ }).click();
  await page.waitForTimeout(250);
  await shot(page, '16-ai-not-installed');
  await page.getByRole('button', { name: 'Token erstellen' }).click();
  await page.getByText('Nur jetzt sichtbar').waitFor();
  await page.waitForTimeout(300);
  await shot(page, '17-ai-token');
} catch (e) {
  await shot(page, 'zz-failure');
  errors.push(`step failed: ${(e as Error).message.split('\n')[0]}`);
  errors.push(`visible text: ${(await page.locator('body').innerText()).slice(0, 600).replace(/\n+/g, ' | ')}`);
}

await browser.close();
console.log(errors.length ? `ERRORS:\n${errors.join('\n')}` : 'no page errors');
console.log(`screenshots in ${out}`);
