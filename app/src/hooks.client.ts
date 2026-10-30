// Outside Tauri (plain browser during `bun run dev`), install the in-memory
// mock backend before any component talks to the IPC layer. The import is
// dead code in production builds and gets dropped.
export async function init() {
  if (import.meta.env.DEV && !('__TAURI_INTERNALS__' in window)) {
    const { install } = await import('#lib/mock.ts');
    install();
  }
}
