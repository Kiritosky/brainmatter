# Contributing

Thanks for wanting to help. Bug reports, ideas and pull requests are all welcome. Issues labelled [good first issue](https://github.com/Kiritosky/brainmatter/labels/good%20first%20issue) are small and explained well enough to start without knowing the code.

Found a security problem? Please follow [SECURITY.md](SECURITY.md) instead of opening an issue.

## Set up

You need [Rust](https://rustup.rs) (the version in `rust-toolchain.toml` installs itself) and [Bun](https://bun.sh). The desktop app builds on macOS only for now. The core, the MCP server and the interface also build on Linux, which is what CI uses.

```sh
git clone https://github.com/Kiritosky/brainmatter
cd brainmatter/app
bun install
bun run dev          # the interface in a browser, against an in-memory mock (password demo1234)
bun run tauri dev    # the real app, with a separate development vault and port
```

Most interface work needs only `bun run dev`. Add `?lang=en` or `?lang=de` to the URL to switch the language.

## Where things are

```
crates/bm-core    Encrypted storage, graph, search, history, attachments, backups
crates/bm-embed   The local embedding model (candle on Metal)
crates/bm-mcp     The MCP server: tools, tokens, scopes
crates/bm-cli     brainmatter-mcp, a stdio bridge to that server
crates/bm-macos   Touch ID, Keychain, file and system events
app/src-tauri     The desktop app (Tauri 2): lock session, setup of AI tools, updates
app/src           The interface (Svelte 5, Sigma.js, CodeMirror)
app/src/lib/mock.ts  The in-memory backend behind `bun run dev`
```

## Before you open a pull request

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd app && bun run check && bun run test
```

If you changed the interface, also run `bun scripts/screens.ts` in `app/` with `bun run dev` running. It clicks through the whole app against the mock, once in German and once in English, saves screenshots and fails on page errors or on German words left in the English interface.

## Conventions

- **Both languages.** Every text in the interface comes as a pair, `t('Speichern', 'Save')`, German first. In Rust it is `tr("…", "…")`. If your German is shaky, write your best guess and say so in the pull request.
- **Changelog.** A change people will notice gets a line in both `CHANGELOG.md` (German) and `CHANGELOG.en.md` under the next version. Write it for users, not developers.
- **Commits** start with `feat:`, `fix:`, `perf:`, `docs:` or `refactor:` and say what changed for the user, such as `fix: links ignore case also for umlauts`.
- **Tests** for new behaviour in the core and the MCP server. Tool answers have a size budget (`crates/bm-mcp/tests/token_cost.rs`), so keep them short.
- **Comments** explain why, not what. Plain words over jargon.
- **No new dependency** without a reason in the pull request. The app is meant to stay small and fast.

## Ideas and bigger changes

For anything larger than a fix, please open an issue first and describe the problem before the solution. Brainmatter tries to stay simple: a feature that needs explaining in the settings is often better left out.

By contributing you agree that your work is published under the [MIT license](LICENSE), and to follow the [code of conduct](CODE_OF_CONDUCT.md).
