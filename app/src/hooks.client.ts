// Outside Tauri (plain browser during `bun run dev`), install the in-memory
// mock backend before any component talks to the IPC layer. The import is
// dead code in production builds and gets dropped.
export async function init() {
  const inTauri = '__TAURI_INTERNALS__' in window;
  // Transparent window background for macOS vibrancy (see app.css).
  if (inTauri) document.documentElement.classList.add('tauri');
  if (import.meta.env.DEV && !inTauri) {
    const { install } = await import('#lib/mock.ts');
    install();
  }
}
