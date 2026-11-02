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
// Past every release, so the "what's new" dialog is not in the way. It gets its
// own context at the end.
await context.addInitScript(() => {
  if (!sessionStorage.getItem('bm.keepSeenVersion')) localStorage.setItem('bm.seenVersion', '99.0.0');
  localStorage.setItem('bm.tourSeen', '1');
});

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

  // Hovering: the entry grows, its surroundings make room and come back, nothing is saved.
  {
    const spot = (title: string) =>
      page.evaluate((t) => (window as unknown as { __bmGraph: { onScreen(t: string): { x: number; y: number; size: number; savedX: number } | null } }).__bmGraph.onScreen(t), title);
    const apart = (a: { x: number; y: number }, b: { x: number; y: number }) => Math.hypot(a.x - b.x, a.y - b.y);
    const canvas = (await page.locator('main canvas').first().boundingBox())!;
    const [entry, nearby] = [(await spot('SQLCipher'))!, (await spot('Hybrid-Suche'))!];
    await page.mouse.move(canvas.x + entry.x, canvas.y + entry.y);
    await page.waitForTimeout(450);
    const [grown, pushed] = [(await spot('SQLCipher'))!, (await spot('Hybrid-Suche'))!];
    await shot(page, '05b-hover');
    if (grown.size < entry.size * 1.3) errors.push(`hovered entry did not grow: ${entry.size} to ${grown.size}`);
    if (apart(grown, pushed) < apart(entry, nearby) + 8) errors.push(`neighbour did not move away: ${apart(entry, nearby)} to ${apart(grown, pushed)}`);
    if (pushed.savedX !== nearby.savedX) errors.push('hovering changed a saved position');
    await page.mouse.move(canvas.x + 4, canvas.y + 4);
    await page.waitForTimeout(500);
    const back = (await spot('Hybrid-Suche'))!;
    if (apart(back, nearby) > 1) errors.push(`neighbour did not come back: ${apart(back, nearby)} px off`);
    const size = (await spot('SQLCipher'))!.size;
    if (Math.abs(size - entry.size) > 0.05) errors.push(`entry did not shrink back: ${size}`);
  }

  // The personal settings take effect right away.
  {
    await page.keyboard.press('Meta+,');
    const dialog = page.getByRole('dialog', { name: 'Einstellungen' });
    await dialog.getByRole('button', { name: 'Graph', exact: true }).click();
    await dialog.getByRole('switch', { name: 'Hintergründe der Projekte' }).click();
    await page.waitForTimeout(250);
    await shot(page, '05c-graph-settings');
    if (await page.locator('.group-circle').count()) errors.push('group backgrounds stayed after switching them off');
    await dialog.getByRole('button', { name: 'Darstellung zurücksetzen' }).click();
    if (!(await page.locator('.group-circle').count())) errors.push('resetting did not bring group backgrounds back');
    await dialog.getByRole('button', { name: 'Allgemein', exact: true }).click();
    const accent = () => page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--color-accent').trim());
    const before = await accent();
    await dialog.getByRole('radio', { name: 'Grün' }).click();
    await page.waitForTimeout(200);
    await shot(page, '05d-general-settings');
    if ((await accent()) === before) errors.push(`accent stayed ${before}`);
    await dialog.getByRole('button', { name: 'Darstellung zurücksetzen' }).click();
    await page.keyboard.press('Escape');
    await dialog.waitFor({ state: 'detached' });
    await page.waitForTimeout(300);
  }

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

  // Attachments: picked, previewed, shown inline when pasted, removed.
  const PNG = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M/wHwAEBgIApD5fRAAAAABJRU5ErkJggg==', 'base64');
  await page.locator('input[type=file]').setInputFiles([
    { name: 'Rezept.md', mimeType: 'text/markdown', buffer: Buffer.from('# Roggenbrot\n\n70 % Roggen') },
    { name: 'Foto.png', mimeType: 'image/png', buffer: PNG },
    { name: 'daten.bin', mimeType: 'application/octet-stream', buffer: Buffer.from([1, 2, 3]) },
  ]);
  await page.getByRole('button', { name: 'Rezept.md' }).waitFor();
  await page.waitForTimeout(400);
  await shot(page, '27-attachments');
  await page.getByRole('button', { name: 'Rezept.md' }).click();
  await page.getByRole('dialog', { name: 'Rezept.md' }).getByText('70 % Roggen').waitFor();
  await page.waitForTimeout(300);
  await shot(page, '28-attachment-text');
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Foto.png' }).click();
  await page.getByRole('dialog', { name: 'Foto.png' }).locator('img').waitFor();
  await page.waitForTimeout(300);
  const decoded = await page.getByRole('dialog', { name: 'Foto.png' }).locator('img').evaluate((img: HTMLImageElement) => img.complete && img.naturalWidth);
  if (decoded !== 1) errors.push(`attachment image did not decode: ${decoded}`);
  await shot(page, '29-attachment-image');
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'daten.bin' }).click();
  await page.getByText('keine Vorschau').waitFor();
  await page.keyboard.press('Escape');

  // A pasted screenshot lands in the text and renders decrypted.
  await page.getByLabel('Bearbeiten').click();
  await page.locator('.cm-content').click();
  await page.keyboard.press('Meta+ArrowDown');
  await page.evaluate((b64) => {
    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    const data = new DataTransfer();
    data.items.add(new File([bytes], 'image.png', { type: 'image/png' }));
    document.querySelector('.cm-content')!.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
  }, PNG.toString('base64'));
  await page.getByText('attachment:').waitFor();
  await page.getByLabel('Ansehen').click();
  await page.waitForTimeout(1200);
  const inline = await page.locator('.prose-bm img').evaluate((img: HTMLImageElement) => img.naturalWidth).catch(() => 0);
  if (inline !== 1) errors.push(`pasted image did not render: ${inline}`);
  await shot(page, '30-attachment-inline');

  // A note (possibly written by an AI) can neither run code nor pull in the
  // attachment of another entry.
  const hostile = await page.evaluate(async () => {
    const invoke = (window as any).__TAURI_INTERNALS__.invoke;
    const hits: any[] = await invoke('search', { query: { text: 'Marathon-Plan', kinds: [], tags: [], limit: 5, include_subprojects: true } });
    const roggen: any[] = await invoke('search', { query: { text: 'Roggenbrot', kinds: [], tags: [], limit: 5, include_subprojects: true } });
    const own = (await invoke('node_detail', { id: roggen[0].node.id })).attachments[0].id;
    const id = hits[0].node.id;
    await invoke('update_node', { id, patch: { body: { mode: 'replace', text: `![fremd](attachment:${own})\n\n<img src="data:," onerror="window.__pwned = 1">\n\n<script>window.__pwned = 1</script>` } } });
    return id as string;
  });
  await openEntry('Marathon-Plan');
  await page.waitForTimeout(600);
  const sources = await page.locator('.prose-bm img').evaluateAll((imgs) => imgs.map((i) => i.getAttribute('src')));
  if (sources.some((s) => s?.startsWith('blob:'))) errors.push(`foreign attachment rendered: ${sources}`);
  if (await page.evaluate(() => (window as any).__pwned)) errors.push('script ran from a note');
  console.log('hostile note blocked:', hostile.length > 0 && !sources.some((s) => s?.startsWith('blob:')));

  await openEntry('Roggenbrot');
  await page.getByRole('button', { name: 'daten.bin' }).hover();
  await page.getByRole('button', { name: 'Anhang entfernen' }).locator('visible=true').click();
  await page.getByRole('button', { name: 'daten.bin' }).waitFor({ state: 'detached' });
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

  // Projects: nest three levels deep, collapse, and find the way back.
  const sidebar = page.getByRole('navigation');
  const addSubproject = async (parent: string, name: string) => {
    await sidebar.getByRole('button', { name: parent, exact: false }).first().hover();
    await sidebar.getByRole('button', { name: `Unterprojekt in ${parent} anlegen` }).click();
    await sidebar.getByPlaceholder('Unterprojekt').fill(name);
    await page.keyboard.press('Enter');
    await sidebar.getByRole('button', { name: new RegExp(`^${name}`) }).waitFor();
    await page.waitForTimeout(250);
  };
  await addSubproject('Kern', 'Tresor-Details');
  await addSubproject('Tresor-Details', 'Schlüssel');
  await page.waitForTimeout(250);
  await shot(page, '43-nested-projects');
  await sidebar.getByRole('button', { name: 'Einklappen: Brainmatter' }).click();
  await page.waitForTimeout(300);
  if (await sidebar.getByRole('button', { name: /^Schlüssel/ }).count()) errors.push('collapsing left a subproject visible');
  await shot(page, '44-collapsed-projects');
  await sidebar.getByRole('button', { name: 'Ausklappen: Brainmatter' }).click();
  await page.waitForTimeout(300);
  await sidebar.getByRole('button', { name: /^Schlüssel/ }).waitFor();
  await sidebar.getByRole('button', { name: 'Einklappen: Kern' }).click();
  await page.waitForTimeout(250);
  // The breadcrumb still leads there; arriving unfolds the way.
  await page.getByRole('button', { name: 'Tresor-Details', exact: true }).first().click();
  await page.waitForTimeout(350);
  await sidebar.getByRole('button', { name: /^Tresor-Details/ }).waitFor();
  await page.getByRole('button', { name: 'Alles', exact: true }).first().click();
  await page.waitForTimeout(400);

  // The palette also finds projects and actions.
  await page.keyboard.press('Meta+k');
  await page.getByPlaceholder('Suchen oder neuen Eintrag anlegen …').fill('privat');
  await page.getByRole('dialog', { name: 'Suche' }).getByText('Prüfen', { exact: false }).first().waitFor({ state: 'attached' }).catch(() => {});
  await page.getByRole('dialog', { name: 'Suche' }).getByRole('button', { name: /Privat.*Projekt/ }).waitFor();
  await page.waitForTimeout(200);
  await shot(page, '55-palette-project');
  await page.keyboard.press('Enter');
  await page.getByRole('button', { name: 'Privat', exact: true }).first().waitFor();
  await page.waitForTimeout(300);
  await page.keyboard.press('Meta+k');
  await page.getByPlaceholder('Suchen oder neuen Eintrag anlegen …').fill('papierkorb');
  await page.getByRole('dialog', { name: 'Suche' }).getByRole('button', { name: /Papierkorb öffnen/ }).click();
  await page.getByText('Der Papierkorb ist leer').or(page.getByRole('heading', { name: 'Papierkorb' })).first().waitFor();
  await page.getByLabel('Schließen').first().click();
  await page.getByRole('button', { name: 'Alles', exact: true }).first().click();
  await page.waitForTimeout(300);

  // Filters hide entries without moving the rest; the local graph follows the selection.
  const countText = async () => (await page.getByRole('dialog', { name: 'Graph filtern' }).getByText(/von \d+ Einträgen/).innerText());
  await page.getByRole('button', { name: 'Filter', exact: true }).click();
  const total = await countText();
  await page.getByRole('dialog', { name: 'Graph filtern' }).getByRole('button', { name: 'concept' }).click();
  await page.waitForTimeout(250);
  const filtered = await countText();
  if (parseInt(filtered) >= parseInt(total.split(' von ')[1])) errors.push(`filter hid nothing: ${filtered}`);
  await shot(page, '56-graph-filter');
  await page.getByRole('dialog', { name: 'Graph filtern' }).getByRole('button', { name: 'Zurücksetzen' }).click();
  await page.keyboard.press('Escape');
  await page.waitForTimeout(200);
  await openEntry('Tresor');
  await page.getByRole('button', { name: 'Zu den Favoriten' }).click();
  await sidebar.getByRole('button', { name: 'Tresor', exact: true }).waitFor();
  await page.getByRole('button', { name: /^Filter/ }).click();
  await page.getByRole('dialog', { name: 'Graph filtern' }).getByRole('button', { name: '1', exact: true }).click();
  await page.waitForTimeout(300);
  const near = await countText();
  if (parseInt(near) < 2 || parseInt(near) > 6) errors.push(`local graph looks wrong: ${near}`);
  await page.keyboard.press('Escape');
  await page.waitForTimeout(350);
  await shot(page, '57-local-graph');
  await page.getByRole('button', { name: /^Filter/ }).click();
  await page.getByRole('dialog', { name: 'Graph filtern' }).getByRole('button', { name: 'Zurücksetzen' }).click();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Aus den Favoriten entfernen' }).click();
  await sidebar.getByRole('button', { name: 'Tresor', exact: true }).waitFor({ state: 'detached' });
  await page.getByLabel('Schließen').first().click();

  // An AI writes while you watch: a note names it, and the entry pulses.
  await page.evaluate(() => (globalThis as unknown as { mockAiCapture(t: string, n: string): void }).mockAiCapture('Von der KI', 'Tresor'));
  await page.getByRole('status').filter({ hasText: 'Claude Code' }).waitFor();
  await page.locator('.pulse-ring').first().waitFor();
  await page.waitForTimeout(450);
  const pulses = await page.locator('.pulse-ring').count();
  if (pulses !== 1) errors.push(`expected one entry to pulse, saw ${pulses}`);
  await shot(page, '61-ai-activity');
  await page.locator('.pulse-ring').first().waitFor({ state: 'detached', timeout: 6000 });
  await page.getByRole('status').filter({ hasText: 'Claude Code' }).waitFor({ state: 'detached', timeout: 9000 });
  await page.evaluate(() => (globalThis as unknown as { mockAiForget(t: string): void }).mockAiForget('Von der KI'));
  await page.waitForTimeout(300);

  // Managing projects: rename, move by menu and by dragging, delete.
  const row = (name: string) => sidebar.getByRole('listitem').filter({ hasText: name }).last();
  const order = () => sidebar.getByRole('listitem').locator('span.truncate').allInnerTexts();

  // Hiding projects: one away, only one left, and opening a hidden one on purpose.
  const shownNow = async () => {
    await page.getByRole('button', { name: /^Filter/ }).click();
    const text = await countText();
    await page.keyboard.press('Escape');
    await page.waitForTimeout(200);
    return parseInt(text);
  };
  await sidebar.getByRole('button', { name: 'Alles', exact: true }).click();
  await page.waitForTimeout(500);
  const everything = await shownNow();
  await row('Privat').hover();
  await page.getByRole('button', { name: 'Ausblenden: Privat' }).click();
  await sidebar.getByText('1 ausgeblendet').waitFor();
  await page.waitForTimeout(350);
  await shot(page, '59-project-hidden');
  const withoutPrivate = await shownNow();
  if (withoutPrivate >= everything) errors.push(`hiding hid nothing: ${withoutPrivate} of ${everything}`);
  if (await sidebar.getByRole('button', { name: /^Privat/ }).count()) errors.push('hidden project still listed');
  await sidebar.getByText('1 ausgeblendet').click();
  await sidebar.getByRole('button', { name: /^Privat/ }).first().click();
  await page.waitForTimeout(500);
  if ((await shownNow()) === 0) errors.push('an opened hidden project shows nothing');
  await sidebar.getByRole('button', { name: 'Alles', exact: true }).click();
  await sidebar.getByRole('button', { name: 'Alle zeigen' }).click();
  await sidebar.getByText('ausgeblendet').waitFor({ state: 'detached' });
  await row('Kern').hover();
  await page.getByRole('button', { name: 'Mehr zu Kern' }).click();
  await page.getByRole('menuitem', { name: 'Nur dieses anzeigen' }).click();
  await page.waitForTimeout(450);
  await shot(page, '60-project-only');
  if ((await shownNow()) >= everything) errors.push('showing only one project hid nothing');
  // The one project left must fill the picture, not sit in a corner of the old one.
  const onlyCircles = page.locator('.group-circle');
  const canvas = await page.locator('main').boundingBox();
  if ((await onlyCircles.count()) !== 1) errors.push(`showing one project draws ${await onlyCircles.count()} circles`);
  if (!(await page.locator('.group-label', { hasText: 'Kern' }).count())) errors.push('the only project shown has no name');
  const onlyCircle = await onlyCircles.first().boundingBox();
  if (onlyCircle && canvas && onlyCircle.width < canvas.height * 0.6) errors.push(`the only project shown stays small: ${Math.round(onlyCircle.width)}px`);
  await sidebar.getByRole('button', { name: 'Alle zeigen' }).click();
  if ((await shownNow()) !== everything) errors.push('showing all did not bring everything back');
  await row('Tresor-Details').hover();
  await sidebar.getByRole('button', { name: 'Mehr zu Tresor-Details' }).click();
  await page.getByRole('menu').waitFor();
  await page.waitForTimeout(200);
  await shot(page, '50-project-menu');
  await page.getByRole('menuitem', { name: 'Umbenennen' }).click();
  await sidebar.getByLabel('Neuer Name').fill('Tresor intern');
  await page.keyboard.press('Enter');
  await row('Tresor intern').waitFor();

  await row('Tresor intern').hover();
  await sidebar.getByRole('button', { name: 'Mehr zu Tresor intern' }).click();
  await page.getByRole('menuitem', { name: 'Verschieben nach' }).click();
  await page.waitForTimeout(200);
  await shot(page, '51-project-move');
  await page.getByRole('menu').getByLabel('Projekt suchen …').fill('pri');
  await page.getByRole('menu').getByRole('option', { name: 'Privat' }).click();
  await page.waitForTimeout(300);
  let names = await order();
  if (names.indexOf('Tresor intern') < names.indexOf('Privat')) errors.push(`menu move failed: ${names}`);

  // Dragging onto a project puts it inside; onto the edge, next to it.
  await row('UI').dragTo(row('Kern'), { targetPosition: { x: 60, y: 16 } });
  await page.waitForTimeout(300);
  names = await order();
  if (names.indexOf('UI') !== names.indexOf('Kern') + 1) errors.push(`drag inside failed: ${names}`);
  const uiIndent = await row('UI').evaluate((el) => parseInt((el as HTMLElement).style.paddingLeft));
  if (uiIndent !== 24) errors.push(`UI should be two levels deep: ${uiIndent}`);
  await row('Tresor intern').dragTo(row('Brainmatter'), { targetPosition: { x: 60, y: 2 } });
  await page.waitForTimeout(300);
  names = await order();
  if (names[0] !== 'Tresor intern') errors.push(`drag before failed: ${names}`);
  // A project cannot be dropped into what is below it.
  await row('Brainmatter').dragTo(row('Kern'), { targetPosition: { x: 60, y: 16 } });
  await page.waitForTimeout(300);
  const parent = await page.evaluate(async () => {
    const list: any[] = await (window as any).__TAURI_INTERNALS__.invoke('projects');
    return list.find((p) => p.project.name === 'Brainmatter').project.parent_id;
  });
  if (parent) errors.push('a project was moved into itself');
  await shot(page, '52-projects-moved');

  // Deleting asks first, says what happens, and keeps entries by default.
  await row('Privat').hover();
  await sidebar.getByRole('button', { name: 'Mehr zu Privat' }).click();
  await page.getByRole('menuitem', { name: 'Löschen' }).click();
  await page.getByRole('dialog', { name: 'Projekt löschen' }).getByText('nur hier').waitFor();
  await page.waitForTimeout(250);
  await shot(page, '53-project-delete');
  await page.keyboard.press('Escape');
  await page.getByRole('dialog', { name: 'Projekt löschen' }).waitFor({ state: 'detached', timeout: 3000 });
  await row('Tresor intern').hover();
  await sidebar.getByRole('button', { name: 'Mehr zu Tresor intern' }).click();
  await page.getByRole('menuitem', { name: 'Löschen' }).click();
  await page.getByRole('button', { name: 'Projekt löschen' }).click();
  await page.getByText('„Tresor intern“ gelöscht').waitFor();
  await row('Schlüssel').waitFor({ state: 'detached' });
  await sidebar.getByRole('button', { name: /^Brainmatter/ }).first().click();
  await page.waitForTimeout(300);

  // Documentation: read, navigate, write, and mirror a folder read-only.
  await sidebar.getByRole('button', { name: /^Brainmatter/ }).first().click();
  await page.getByRole('radio', { name: 'Doku' }).click();
  await page.getByRole('heading', { name: 'Handbuch' }).waitFor();
  await page.waitForTimeout(400);
  // Doc tags: filter the page list, then add one to the open page.
  const tagFilters = page.getByRole('group', { name: 'Nach Tag filtern' });
  await tagFilters.getByRole('button', { name: /#start/ }).click();
  await page.getByRole('navigation', { name: 'Seiten' }).getByText('Architektur').waitFor({ state: 'detached' });
  await shot(page, '58-docs-tag-filter');
  await tagFilters.getByRole('button', { name: /#start/ }).click();
  await page.getByLabel('Tag hinzufügen').first().fill('neu');
  await page.keyboard.press('Enter');
  await tagFilters.getByRole('button', { name: /#neu/ }).waitFor();
  await page.getByRole('button', { name: 'Tag neu entfernen' }).click();
  await tagFilters.getByRole('button', { name: /#neu/ }).waitFor({ state: 'detached' });

  // A page is written on the page: the slash menu inserts what one does not know by heart,
  // and text typed while earlier text is being saved stays, with the cursor where it was.
  await page.getByRole('button', { name: 'Bearbeiten' }).click();
  const docEditor = page.locator('main .cm-content');
  await docEditor.waitFor();
  if (await page.getByRole('complementary', { name: 'Eintrag' }).count()) errors.push('writing a doc page opened the side panel');
  await docEditor.click();
  await page.keyboard.press('Meta+ArrowDown');
  await page.keyboard.type('\n\n/');
  await page.locator('.cm-tooltip-autocomplete').getByText('Tabelle').waitFor();
  await page.getByText('Rückgängig über die Aktivität').waitFor({ state: 'detached', timeout: 15000 }).catch(() => {});
  await page.waitForTimeout(150);
  await shot(page, '77a-docs-slash-menu');
  await page.keyboard.type('tab');
  await page.locator('.cm-tooltip-autocomplete').getByText('Tabelle').waitFor();
  await page.waitForTimeout(150);
  await shot(page, '77-docs-slash');
  await page.keyboard.press('Enter');
  await page.keyboard.type('Name');
  await page.keyboard.press('Meta+ArrowDown');
  await page.keyboard.type('\n\n/h2');
  await page.locator('.cm-tooltip-autocomplete').getByText('Überschrift 2').waitFor();
  await page.keyboard.press('Enter');
  await page.keyboard.type('Getippt beim Speichern');
  // Longer than the save delay, more typing right around the moment it goes out.
  await page.waitForTimeout(880);
  await page.keyboard.type(' A');
  await page.waitForTimeout(40);
  await page.keyboard.type(' B');
  await page.waitForTimeout(1100);
  await page.keyboard.type(' C');
  await page.keyboard.press('Meta+s');
  await page.waitForTimeout(250);
  await page.keyboard.type(' D');
  await page.waitForTimeout(1200);
  const written = await docEditor.innerText();
  if (!written.includes('## Getippt beim Speichern A B C D')) errors.push(`typing while saving lost text or moved the cursor: ${written.slice(-120)}`);
  if (!written.includes('| Name | Spalte |')) errors.push(`slash menu inserted the wrong table: ${written.slice(-160)}`);
  await page.waitForTimeout(100);
  await shot(page, '78-docs-writing');
  await page.keyboard.press('Escape');
  await page.locator('article').getByRole('heading', { name: 'Getippt beim Speichern A B C D' }).waitFor();
  if (await page.locator('main .cm-content').count()) errors.push('Esc stayed in the doc editor');
  await page.waitForTimeout(300);
  await shot(page, '48-docs');
  // The test's own text out again, for the steps below.
  await page.evaluate(async () => {
    const invoke = (window as any).__TAURI_INTERNALS__.invoke;
    const hits: any[] = await invoke('search', { query: { text: 'Getippt beim Speichern', kinds: ['doc'], tags: [], limit: 1, include_subprojects: true } });
    const node = (await invoke('node_detail', { id: hits[0].node.id })).node;
    await invoke('update_node', { id: node.id, patch: { body: { mode: 'replace', text: node.body.split('\n\n| Name')[0] } } });
  });
  await page.getByRole('heading', { name: 'Getippt beim Speichern A B C D' }).waitFor({ state: 'detached' });
  const contents = page.getByRole('complementary', { name: 'Auf dieser Seite' });
  await contents.getByRole('button', { name: 'KI-Zugriff' }).click();
  await page.getByRole('button', { name: /Weiter/ }).click();
  await page.getByRole('heading', { name: 'Installation', level: 1 }).first().waitFor();
  await page.getByRole('button', { name: /Zurück/ }).click();
  await page.getByRole('heading', { name: 'Handbuch' }).waitFor();
  await page.getByLabel('In der Doku suchen').fill('Tresor');
  await page.getByText('Der Tresor').first().waitFor();
  await page.waitForTimeout(250);
  await page.getByLabel('In der Doku suchen').fill('');
  await page.getByRole('navigation', { name: 'Seiten' }).getByText('Architektur').hover();
  await page.getByRole('button', { name: '„Architektur“ nach oben' }).click();
  await page.waitForTimeout(250);
  await page.getByRole('button', { name: 'Doku-Ordner einbinden' }).click();
  await page.getByRole('navigation', { name: 'Seiten' }).getByText('Einrichtung').click();
  await page.getByText('nur dort änderbar').waitFor();
  if (await page.getByRole('main').getByRole('button', { name: 'Bearbeiten' }).count()) errors.push('a mirrored page offers editing');
  await page.waitForTimeout(250);
  await shot(page, '49-docs-mirrored');
  await page.getByRole('radio', { name: 'Graph' }).click();
  await page.getByRole('button', { name: 'Alles', exact: true }).first().click();
  await page.waitForTimeout(400);

  // File links: a link an AI suggested needs approval before it can be opened.
  await openEntry('Tresor');
  await page.getByRole('button', { name: 'Datei verknüpfen' }).click();
  await page.getByText('Spezifikation.md', { exact: true }).waitFor();
  await page.getByText('KIs dürfen diese Datei nicht lesen.').waitFor();
  await page.getByText('Spezifikation.md', { exact: true }).hover();
  await page.waitForTimeout(250);
  await shot(page, '47-file-links');
  if (await page.getByRole('button', { name: '„config“ öffnen' }).count()) errors.push('an unapproved path can be opened');
  await page.getByRole('button', { name: 'Freigeben' }).click();
  await page.getByRole('button', { name: '„config“ öffnen' }).waitFor({ state: 'attached' });
  await page.getByRole('button', { name: 'Verknüpfung „config“ entfernen' }).click({ force: true });
  await page.getByText('/Users/alex/.ssh/config').waitFor({ state: 'detached' });
  await page.getByLabel('Schließen').first().click();

  // Library: fill in a template, and check what would be copied.
  await sidebar.getByRole('button', { name: 'Bibliothek' }).click();
  await page.getByText('Code-Review', { exact: true }).waitFor();
  await page.waitForTimeout(350);
  await shot(page, '45-library');
  await page.getByText('Code-Review', { exact: true }).click();
  await page.getByText('Verwenden', { exact: true }).waitFor();
  await page.getByLabel('Wert für datei').fill('session.rs');
  await page.waitForTimeout(150);
  const preview = await page.locator('pre').innerText();
  if (!preview.includes('Prüfe session.rs auf {{thema}}')) errors.push(`template preview wrong: ${preview}`);
  await page.getByText('1 Platzhalter noch offen').waitFor();
  await page.getByLabel('Wert für thema').fill('Fehler');
  await page.getByText('Alles ausgefüllt').waitFor();
  await page.waitForTimeout(250);
  await shot(page, '46-template-use');
  if (await page.getByText('Code-Review', { exact: true }).locator('xpath=ancestor::main').count()) errors.push('template shown in the graph header');
  const inGraph = await page.evaluate(async () => {
    const invoke = (window as any).__TAURI_INTERNALS__.invoke;
    return (await invoke('graph', { project: null })).nodes.some((n: any) => n.kind === 'template');
  });
  if (inGraph) errors.push('templates must stay out of the graph');
  await page.getByLabel('Schließen').first().click();

  // A new template is walked through, and using it gives an entry of its own.
  await page.getByRole('button', { name: 'Neue Vorlage' }).click();
  const wizard = page.getByRole('dialog', { name: 'Neue Vorlage' });
  const onward = wizard.getByRole('button', { name: 'Weiter' });
  if (await onward.isEnabled()) errors.push('the template wizard goes on with nothing chosen');
  await wizard.getByRole('radio', { name: /Besprechung/ }).click();
  if ((await wizard.getByLabel('Name der Vorlage').inputValue()) !== 'Besprechung') errors.push('the wizard did not suggest a name');
  await page.waitForTimeout(250);
  await shot(page, '79-wizard-kind');
  await onward.click();
  await wizard.getByLabel('Name des Platzhalters').fill('ort');
  await wizard.getByRole('button', { name: 'Einfügen' }).click();
  await wizard.getByText(/fragt sie nach:.*ort/).waitFor();
  await page.waitForTimeout(150);
  await shot(page, '80-wizard-content');
  await onward.click();
  await wizard.getByText('In Projekten').click();
  const create = wizard.getByRole('button', { name: 'Vorlage anlegen' });
  if (await create.isEnabled()) errors.push('the wizard finished with no project chosen');
  await wizard.getByLabel('Projekt suchen').fill('pri');
  await wizard.getByLabel('Projekt suchen').press('Enter');
  await wizard.getByRole('button', { name: 'Privat entfernen' }).waitFor();
  await page.waitForTimeout(150);
  await shot(page, '81-wizard-where');
  await create.click();
  await wizard.waitFor({ state: 'detached' });
  await page.getByText('Verwenden', { exact: true }).waitFor();
  await page.getByLabel('Wert für thema').fill('Planung');
  await page.getByRole('button', { name: 'Als Eintrag anlegen' }).click();
  await page.getByLabel('Titel').and(page.locator('input')).first().waitFor();
  await page.waitForFunction(() => (document.querySelector('input[aria-label="Titel"]') as HTMLInputElement | null)?.value === 'Besprechung: Planung');
  await page.waitForTimeout(250);
  const usedIn = await page.getByRole('complementary', { name: 'Eintrag' }).innerText();
  if (!usedIn.includes('Privat')) errors.push('an entry from a template did not land in its project');

  // Assigning: projects and tags are found by typing, and a missing project can be created.
  await page.getByRole('complementary', { name: 'Eintrag' }).getByRole('button', { name: 'Projekt', exact: true }).click();
  await page.getByLabel('Projekt suchen …').fill('ker');
  await page.getByRole('option', { name: /Kern/ }).first().waitFor();
  await page.waitForTimeout(150);
  await shot(page, '82-assign-project');
  await page.getByLabel('Projekt suchen …').press('Enter');
  await page.getByRole('button', { name: 'Aus Projekt entfernen' }).nth(1).waitFor();
  await page.getByRole('complementary', { name: 'Eintrag' }).getByRole('button', { name: 'Projekt', exact: true }).click();
  await page.getByLabel('Projekt suchen …').fill('Ganz neu');
  await page.getByRole('option', { name: /neu anlegen/ }).click();
  await page.getByRole('complementary', { name: 'Eintrag' }).getByText('Ganz neu').waitFor();
  await page.getByRole('complementary', { name: 'Eintrag' }).getByLabel('Tag hinzufügen').fill('konz');
  await page.getByRole('option', { name: '#konzept' }).waitFor();
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
  await page.getByRole('complementary', { name: 'Eintrag' }).getByText('#konzept').waitFor();
  await page.getByLabel('Schließen').first().click();
  await page.waitForTimeout(250);
  if (await page.getByRole('button', { name: 'Neue Vorlage' }).count()) await sidebar.getByRole('button', { name: 'Bibliothek' }).click();
  await page.waitForTimeout(250);

  // Trash: delete, restore, delete again, erase for good.
  const deleteOpenEntry = async () => {
    await page.getByLabel('In den Papierkorb').click();
    await page.getByRole('button', { name: 'Löschen?' }).click();
    await page.waitForTimeout(300);
  };
  await openEntry('Graph-Layout');
  await deleteOpenEntry();
  await page.getByRole('button', { name: 'Papierkorb', exact: true }).click();
  await page.getByText('Graph-Layout').first().waitFor();
  await page.waitForTimeout(350);
  await shot(page, '40-trash');
  await page.getByRole('button', { name: '„Graph-Layout“ wiederherstellen' }).click({ force: true });
  await page.getByLabel('Titel').waitFor();
  await page.waitForTimeout(300);
  await deleteOpenEntry();
  await page.getByRole('button', { name: '„Graph-Layout“ endgültig löschen' }).click({ force: true });
  await page.waitForTimeout(250);
  await shot(page, '41-trash-confirm');
  await page.getByRole('button', { name: 'Endgültig löschen', exact: true }).click();
  await page.getByText('Der Papierkorb ist leer').waitFor();
  await page.waitForTimeout(300);
  await shot(page, '42-trash-empty');
  await page.getByRole('button', { name: 'Papierkorb', exact: true }).click();

  // Time slider: halfway through, some entries are not there yet.
  await page.getByRole('button', { name: 'Zeitregler' }).click();
  const slider = page.getByRole('slider', { name: 'Zeitpunkt' });
  await slider.waitFor();
  const box = (await slider.boundingBox())!;
  await page.mouse.click(box.x + box.width * 0.45, box.y + box.height / 2);
  await page.waitForTimeout(500);
  await shot(page, '32-time-slider');
  const count = await page.getByText(/von \d+ Einträgen/).innerText();
  if (!/^[1-9]\d* von /.test(count) || /^(\d+) von \1 /.test(count)) errors.push(`time slider count looks wrong: ${count}`);
  await page.getByRole('button', { name: 'Abspielen' }).click();
  await page.waitForTimeout(600);
  await page.getByRole('button', { name: 'Anhalten' }).click();
  await page.getByRole('button', { name: 'Zeitregler schließen' }).click();

  await page.keyboard.press('Meta+,');
  await page.getByRole('heading', { name: 'Allgemein' }).waitFor();
  await page.waitForTimeout(300);
  await page.getByRole('button', { name: 'Änderungen', exact: true }).click();
  await page.getByRole('dialog', { name: 'Alle Änderungen' }).getByText('Version 0.2.0').waitFor();
  await page.keyboard.press('Escape');
  await page.getByRole('dialog', { name: 'Alle Änderungen' }).waitFor({ state: 'detached' });
  await page.getByRole('button', { name: 'Jetzt suchen' }).click();
  await page.getByText('Version 0.5.0 ist verfügbar').waitFor();
  await page.waitForTimeout(250);
  await shot(page, '33-settings-general');
  await page.getByRole('button', { name: 'Was ist neu?' }).click();
  await page.getByRole('dialog', { name: 'Neu in Version 0.5.0' }).getByText('Etwas Schönes').waitFor();
  await page.waitForTimeout(250);
  await shot(page, '33b-update-notes');
  await page.keyboard.press('Escape');
  await page.getByRole('dialog', { name: 'Neu in Version 0.5.0' }).waitFor({ state: 'detached' });
  await page.getByRole('radio', { name: 'Hell' }).click();
  await page.waitForTimeout(500);
  await shot(page, '34-light-settings');
  await page.keyboard.press('Escape');
  await page.waitForTimeout(600);
  // Opening an entry earlier moved the camera; the fit button has to bring every circle back.
  await page.getByRole('button', { name: 'Alles zeigen' }).click();
  await page.waitForTimeout(500);
  const stage = await page.locator('main').boundingBox();
  for (const ring of await page.locator('.group-circle').all()) {
    const box = await ring.boundingBox();
    if (stage && box && (box.x < stage.x - 1 || box.y < stage.y - 1 || box.x + box.width > stage.x + stage.width + 1 || box.y + box.height > stage.y + stage.height + 1)) {
      errors.push(`a circle stays out of view after fitting: ${JSON.stringify(box)}`);
    }
  }
  await shot(page, '35-light-graph');
  await openEntry('Brainmatter');
  await page.waitForTimeout(600);
  await shot(page, '36-light-inspector');
  await page.keyboard.press('Meta+,');
  await page.getByRole('radio', { name: 'Dunkel' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Sperren' }).click();
  await page.getByText('Automatisch sperren', { exact: true }).waitFor();
  await page.waitForTimeout(300);
  await shot(page, '11-settings');
  await page.getByRole('button', { name: 'Automatisch sperren nach' }).click();
  await page.waitForTimeout(250);
  await shot(page, '12-settings-select');
  await page.keyboard.press('Escape');

  await page.getByRole('button', { name: 'Backups' }).click();
  await page.getByText('Vorhandene Backups').waitFor();
  await page.waitForTimeout(250);
  await page.getByRole('button', { name: 'Jetzt sichern' }).click();
  await page.waitForTimeout(350);
  await page.getByRole('button', { name: 'Wiederherstellen' }).nth(1).click();
  await page.getByPlaceholder('Master-Passwort').fill('falsch');
  await page.getByRole('button', { name: 'Zurücksetzen' }).click();
  await page.getByText('Falsches Passwort').waitFor();
  await page.waitForTimeout(250);
  await shot(page, '31-backups');

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
  await page.getByText('Übergabe', { exact: true }).waitFor(); // AI-written templates need review too
  await page.locator('aside').getByText('Hybrid-Suche').hover();
  await page.locator('aside').locator('div.group', { hasText: 'Hybrid-Suche' }).getByLabel('Als geprüft markieren').click();
  await page.getByRole('button', { name: 'Alle als geprüft markieren' }).waitFor({ state: 'detached' });
  await page.locator('aside').getByLabel('Als geprüft markieren').click();
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
  errors.push(`step failed: ${(e as Error).message.split("\n")[0]} at ${(e as Error).stack?.match(/screens\.ts:\d+/)?.[0]} ${(e as Error).message.split("\n").find((l) => l.includes("waiting for")) ?? ""}`);
  errors.push(`visible text: ${(await page.locator('body').innerText()).slice(0, 600).replace(/\n+/g, ' | ')}`);
}

// Quick capture: asks to unlock first, then saves and confirms.
{
  const capture = await open('/?window=capture&touchid=ok');
  try {
    await capture.setViewportSize({ width: 620, height: 168 });
    await capture.getByText('Brainmatter ist gesperrt').waitFor();
    await capture.waitForTimeout(250);
    await shot(capture, '37-capture-locked');
    await capture.getByRole('button', { name: 'Touch ID' }).click();
    const field = capture.getByLabel('Neuer Eintrag');
    await field.waitFor();
    await field.fill('Idee für den Zeitregler\nAuch Verbindungen zeigen');
    await capture.waitForTimeout(150);
    await shot(capture, '38-capture');
    await field.press('Enter');
    await capture.getByText('Festgehalten').waitFor();
    await shot(capture, '39-capture-saved');
    const saved = await capture.evaluate(async () => {
      const invoke = (window as any).__TAURI_INTERNALS__.invoke;
      const hits: any[] = await invoke('search', { query: { text: 'Idee für den Zeitregler', kinds: [], tags: [], limit: 5, include_subprojects: true } });
      return (await invoke('node_detail', { id: hits[0].node.id })).node;
    });
    if (saved.title !== 'Idee für den Zeitregler' || saved.body !== 'Auch Verbindungen zeigen') errors.push(`capture saved wrongly: ${JSON.stringify(saved)}`);
  } catch (e) {
    await shot(capture, 'zz-capture-failure');
    errors.push(`capture failed: ${(e as Error).message.split('\n')[0]}`);
  }
}

// Getting around the graph, and what the settings say when a shortcut is taken.
{
  const nav = await open('/?shortcutBlocked');
  type Hook = {
    onScreen(t: string): { x: number; y: number } | null;
    ringOnScreen(id: string): { x: number; y: number; radius: number } | null;
    camera(): { x: number; y: number; ratio: number };
  };
  const hook = <T,>(fn: (h: Hook, arg: string) => T, arg = '') =>
    nav.evaluate(([f, a]) => new Function('h', 'a', `return (${f})(h, a)`)((window as unknown as { __bmGraph: Hook }).__bmGraph, a), [fn.toString(), arg] as const) as Promise<T>;
  try {
    await nav.getByPlaceholder('Master-Passwort').fill('demo1234');
    await nav.getByRole('button', { name: 'Entsperren', exact: true }).click();
    await nav.getByText('Einträge ·').waitFor();
    await nav.waitForTimeout(1200);
    const canvas = (await nav.locator('main canvas').first().boundingBox())!;

    // ⌥-drag from one entry to another links them, and moves neither the entry nor the camera.
    const from = (await hook((h, a) => h.onScreen(a), 'SQLCipher'))!;
    const to = (await hook((h, a) => h.onScreen(a), 'Hybrid-Suche'))!;
    const before = await hook((h) => h.camera());
    await nav.keyboard.down('Alt');
    await nav.mouse.move(canvas.x + from.x, canvas.y + from.y);
    await nav.waitForTimeout(250);
    await nav.mouse.down();
    await nav.mouse.move(canvas.x + (from.x + to.x) / 2, canvas.y + (from.y + to.y) / 2, { steps: 6 });
    await nav.waitForTimeout(80);
    await shot(nav, '70-link-drag');
    if (!(await nav.locator('svg line.link-line').count())) errors.push('alt-drag draws no line');
    await nav.mouse.move(canvas.x + to.x, canvas.y + to.y, { steps: 6 });
    await nav.waitForTimeout(100);
    await nav.mouse.up();
    await nav.keyboard.up('Alt');
    await nav.getByText('„SQLCipher“ mit „Hybrid-Suche“ verbunden').waitFor();
    await nav.waitForTimeout(100);
    if (await nav.locator('svg line.link-line').count()) errors.push('link line stays after letting go');
    const after = await hook((h) => h.camera());
    if (Math.abs(before.x - after.x) + Math.abs(before.y - after.y) > 1e-6) errors.push('alt-drag moved the camera');
    const moved = (await hook((h, a) => h.onScreen(a), 'SQLCipher'))!;
    if (Math.hypot(moved.x - from.x, moved.y - from.y) > 30) errors.push('alt-drag moved the entry');

    // Double-click on a subproject's circle opens it; Esc walks back up.
    const ids = await nav.evaluate(async () => {
      const list: { project: { id: string; name: string } }[] = await (window as any).__TAURI_INTERNALS__.invoke('projects');
      return Object.fromEntries(list.map((p) => [p.project.name, p.project.id]));
    });
    const ring = (await hook((h, a) => h.ringOnScreen(a), ids.UI))!;
    await nav.mouse.move(0, 0);
    await nav.waitForTimeout(300);
    await nav.mouse.dblclick(canvas.x + ring.x + ring.radius * 0.88, canvas.y + ring.y);
    const crumbs = nav.locator('header');
    await crumbs.getByText('UI').first().waitFor({ timeout: 3000 });
    await nav.waitForTimeout(300);
    await nav.keyboard.press('Escape');
    await crumbs.getByText('Brainmatter').first().waitFor({ timeout: 3000 });
    await nav.waitForTimeout(300);
    await nav.keyboard.press('Escape');
    await crumbs.getByText('Alles').first().waitFor({ timeout: 3000 });

    // A saved view brings back the project and the filter, and can be deleted.
    await nav.getByRole('button', { name: 'Privat' }).first().click();
    await nav.getByTitle('Graph filtern', { exact: true }).click();
    await nav.getByLabel('Name für diese Ansicht').fill('Nur privat');
    await nav.getByRole('button', { name: 'Speichern', exact: true }).click();
    await nav.getByRole('button', { name: 'Nur privat', exact: true }).waitFor();
    await nav.waitForTimeout(300);
    await shot(nav, '71-saved-view');
    await nav.keyboard.press('Escape');
    await nav.getByRole('button', { name: 'Alles', exact: true }).first().click();
    await nav.waitForTimeout(300);
    await nav.getByRole('button', { name: 'Nur privat', exact: true }).click();
    await crumbs.getByText('Privat').first().waitFor({ timeout: 3000 });
    await nav.getByRole('button', { name: 'Nur privat', exact: true }).hover();
    await nav.getByRole('button', { name: 'Ansicht löschen: Nur privat' }).click();
    await nav.getByRole('button', { name: 'Nur privat', exact: true }).waitFor({ state: 'detached' });

    // A taken shortcut is said in the settings, not just in a log.
    await nav.keyboard.press('Meta+,');
    const settings = nav.getByRole('dialog', { name: 'Einstellungen' });
    await settings.getByText('Eine andere App belegt').waitFor({ timeout: 3000 });
    await nav.waitForTimeout(300);
    await shot(nav, '72-shortcut-blocked');

    // Exporting asks for the password and warns that the result is not encrypted.
    await settings.getByRole('button', { name: 'Backups' }).click();
    await settings.getByRole('button', { name: 'Exportieren' }).click();
    await settings.getByText('Der Export ist nicht verschlüsselt').waitFor();
    await settings.getByLabel('Master-Passwort für den Export').fill('falsch');
    await settings.getByRole('button', { name: 'Ordner wählen' }).click();
    await settings.getByText('Falsches Passwort').waitFor();
    await settings.getByLabel('Master-Passwort für den Export').fill('demo1234');
    await settings.getByRole('button', { name: 'Ordner wählen' }).click();
    await settings.getByText(/Einträge/).first().waitFor();
    await nav.waitForTimeout(300);
    await shot(nav, '73-exported');
  } catch (e) {
    await shot(nav, 'zz-nav-failure');
    errors.push(`getting around failed: ${(e as Error).message.split('\n')[0]}`);
  }
  await nav.close();
}

// A full sidebar in a small window scrolls inside; nothing is pushed out of the window.
{
  const full = await open('/');
  try {
    await full.setViewportSize({ width: 900, height: 600 });
    await full.getByPlaceholder('Master-Passwort').fill('demo1234');
    await full.getByRole('button', { name: 'Entsperren', exact: true }).click();
    await full.getByText('Einträge ·').waitFor();
    await full.evaluate(async () => {
      const invoke = (window as any).__TAURI_INTERNALS__.invoke;
      for (let i = 0; i < 40; i++) await invoke('create_project', { input: { name: `Projekt ${i + 1}` } });
    });
    // Any change of the project list brings all of them in.
    await full.getByRole('button', { name: 'Neues Projekt' }).click();
    await full.getByPlaceholder('Projektname').fill('Projekt 0');
    await full.getByPlaceholder('Projektname').press('Enter');
    await full.getByRole('button', { name: 'Projekt 40' }).first().waitFor({ state: 'attached' });
    await full.getByRole('button', { name: 'Alles', exact: true }).first().click();
    await full.waitForTimeout(900);
    await shot(full, '75-small-window-full-sidebar');
    const inWindow = async (name: string, target: ReturnType<Page['locator']>) => {
      const box = await target.boundingBox();
      if (!box || box.y < 0 || box.y + box.height > 600) errors.push(`${name} is out of the window: ${JSON.stringify(box)}`);
    };
    await inWindow('sidebar footer', full.getByRole('button', { name: 'Einstellungen', exact: true }));
    await inWindow('trash button', full.getByRole('button', { name: 'Papierkorb' }));
    await inWindow('graph filter', full.getByTitle('Graph filtern', { exact: true }));
    await inWindow('fit to window', full.locator('main').getByRole('button').last());
    const scrolled = await full.getByRole('button', { name: 'Projekt 40' }).first().evaluate((row) => {
      const list = row.closest('.scroll-fade') as HTMLElement;
      list.scrollTop = list.scrollHeight;
      return { scrolls: list.scrollHeight > list.clientHeight, doc: document.scrollingElement!.scrollTop };
    });
    if (!scrolled.scrolls) errors.push('sidebar list does not scroll');
    if (scrolled.doc !== 0) errors.push('the whole page scrolled');
    await full.waitForTimeout(100);
    await inWindow('last project after scrolling', full.getByRole('button', { name: 'Projekt 40' }).first());

    // With the inspector open too, the graph has little width left, and its tools still fit.
    await full.getByRole('button', { name: 'Neuer Eintrag' }).click();
    await full.waitForTimeout(500);
    await shot(full, '76-small-window-inspector');
    await inWindow('graph filter beside the inspector', full.getByTitle('Graph filtern', { exact: true }));
    const crowded = await full.evaluate(() => {
      const main = document.querySelector('main')!.getBoundingClientRect();
      const tools = [...document.querySelectorAll('main .absolute.bottom-4')].map((t) => t.getBoundingClientRect()).filter((r) => r.width > 0);
      return {
        out: tools.some((r) => r.right > main.right + 0.5 || r.left < main.left - 0.5),
        overlap: tools.some((a, i) => tools.some((b, j) => i < j && a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom)),
      };
    });
    if (crowded.out || crowded.overlap) errors.push(`graph tools do not fit beside the inspector: ${JSON.stringify(crowded)}`);
  } catch (e) {
    await shot(full, 'zz-full-sidebar-failure');
    errors.push(`full sidebar failed: ${(e as Error).message.split('\n')[0]}`);
  }
  await full.close();
}

// The first start shows around once, and the help page has what it showed and the AI's tools.
{
  const fresh = await browser.newContext({ viewport: { width: 1400, height: 880 }, deviceScaleFactor: 2, colorScheme: 'dark' });
  await fresh.addInitScript(() => localStorage.setItem('bm.seenVersion', '99.0.0'));
  const tour = await fresh.newPage();
  tour.on('pageerror', (e) => errors.push(`tour: ${e.message}`));
  const unlock = async () => {
    await tour.goto(BASE);
    await tour.getByPlaceholder('Master-Passwort').fill('demo1234');
    await tour.getByRole('button', { name: 'Entsperren', exact: true }).click();
  };
  try {
    await unlock();
    const card = tour.getByRole('dialog', { name: 'Einführung' });
    await card.getByText('Willkommen bei Brainmatter').waitFor();
    await tour.waitForTimeout(350);
    await shot(tour, '83-tour-start');
    const total = Number((await card.getByText(/von \d+/).innerText()).split(' ').pop());
    for (let i = 1; i < total; i++) {
      await tour.keyboard.press('ArrowRight');
      await tour.waitForTimeout(320);
      const fits = await card.evaluate((el) => {
        const r = el.getBoundingClientRect();
        return r.left >= 0 && r.top >= 0 && r.right <= innerWidth && r.bottom <= innerHeight;
      });
      if (!fits) errors.push(`tour step ${i + 1} leaves the window`);
      if (i === 1) await shot(tour, '84-tour-search');
      if (i === 4) await shot(tour, '85-tour-graph');
      if (i === 10) await shot(tour, '86-tour-review');
    }
    await card.getByRole('button', { name: 'Fertig' }).click();
    await card.waitFor({ state: 'detached' });
    await unlock();
    await tour.getByText('Einträge ·').waitFor();
    await tour.waitForTimeout(500);
    if (await card.count()) errors.push('the tour came back after it was finished');

    await tour.keyboard.press('Meta+,');
    const settings = tour.getByRole('dialog', { name: 'Einstellungen' });
    await settings.getByRole('button', { name: 'Hilfe' }).click();
    await settings.getByText('Zwei Einträge verbinden').waitFor();
    await tour.waitForTimeout(250);
    await shot(tour, '87-help-app');
    await settings.getByText('Für KIs').click();
    await settings.getByText('project_overview').waitFor();
    await settings.getByText('setup-project').waitFor();
    await tour.waitForTimeout(250);
    await shot(tour, '88-help-ai');
    await settings.getByRole('button', { name: 'Einführung starten' }).click();
    await card.getByText('Willkommen bei Brainmatter').waitFor();
    await tour.keyboard.press('Escape');
    await card.waitFor({ state: 'detached' });
  } catch (e) {
    await shot(tour, 'zz-tour-failure');
    errors.push(`tour failed: ${(e as Error).message.split('\n')[0]}`);
  }
  await fresh.close();
}

// Quick capture: where the entry goes is picked in place and remembered.
{
  const capture = await open('/?window=capture&touchid=ok');
  try {
    await capture.setViewportSize({ width: 620, height: 168 });
    await capture.getByRole('button', { name: 'Touch ID' }).click();
    const field = capture.getByLabel('Neuer Eintrag');
    await field.waitFor();
    await field.press('Meta+p');
    const search = capture.getByLabel('Projekt suchen');
    await search.waitFor();
    await search.fill('pri');
    await capture.waitForTimeout(150);
    await shot(capture, '74-capture-project');
    await search.press('Enter');
    await field.waitFor();
    await capture.getByRole('button', { name: /Privat/ }).waitFor();
    // Esc leaves the list only, not the window.
    await field.press('Meta+p');
    await search.waitFor();
    await search.press('Escape');
    await field.waitFor();
    await field.fill('Geht ins Projekt');
    await field.press('Enter');
    await capture.getByText('Festgehalten').waitFor();
    const projects = await capture.evaluate(async () => {
      const invoke = (window as any).__TAURI_INTERNALS__.invoke;
      const hits: any[] = await invoke('search', { query: { text: 'Geht ins Projekt', kinds: [], tags: [], limit: 5, include_subprojects: true } });
      return (await invoke('node_detail', { id: hits[0].node.id })).projects.map((p: { name: string }) => p.name);
    });
    if (projects.join() !== 'Privat') errors.push(`capture went to the wrong project: ${projects}`);
    await capture.evaluate(() => localStorage.removeItem('bm.captureProject'));
  } catch (e) {
    await shot(capture, 'zz-capture-project-failure');
    errors.push(`capture project failed: ${(e as Error).message.split('\n')[0]}`);
  }
  await capture.close();
}

// After an update the app says once what changed, back to the last seen version.
{
  const fresh = await browser.newContext({ viewport: { width: 1400, height: 880 }, deviceScaleFactor: 2, colorScheme: 'dark' });
  await fresh.addInitScript(() => {
    if (!localStorage.getItem('bm.seenVersion')) localStorage.setItem('bm.seenVersion', '0.2.0');
    localStorage.setItem('bm.tourSeen', '1');
  });
  const updated = await fresh.newPage();
  const unlock = async () => {
    await updated.goto(BASE);
    await updated.getByPlaceholder('Master-Passwort').fill('demo1234');
    await updated.getByRole('button', { name: 'Entsperren', exact: true }).click();
  };
  try {
    await unlock();
    const dialog = updated.getByRole('dialog', { name: 'Was ist neu' });
    await dialog.getByText('Version 0.4.0').waitFor();
    await dialog.getByText('Version 0.3.0').waitFor();
    if (await dialog.getByText('Version 0.2.0').count()) errors.push('what is new shows a version already seen');
    await updated.waitForTimeout(300);
    await shot(updated, '54-whats-new');
    await updated.keyboard.press('Escape');
    await dialog.waitFor({ state: 'detached' });
    await unlock();
    await updated.getByText('Einträge ·').waitFor();
    await updated.waitForTimeout(500);
    if (await dialog.count()) errors.push('what is new appeared twice');
  } catch (e) {
    await shot(updated, 'zz-whatsnew-failure');
    errors.push(`what is new failed: ${(e as Error).message.split('\n')[0]}`);
  }
  await fresh.close();
}

await browser.close();
console.log(errors.length ? `ERRORS:\n${errors.join('\n')}` : 'no page errors');
console.log(`screenshots in ${out}`);
