# Security

Brainmatter holds what people would rather keep private, and lets AI tools read it. This page says how it is protected, what it does not protect against, and how to report a problem.

**No external audit has been done.**

## Reporting a vulnerability

Please use [GitHub's private reporting](https://github.com/Kiritosky/brainmatter/security/advisories/new), or write to [brainmatter.dev@icloud.com](mailto:brainmatter.dev@icloud.com), not a public issue. Say what an attacker needs (local access, a connected AI client, a crafted note …) and what they get.

Only the latest release is supported. Updates are signed and install from inside the app.

## How the vault is protected

```
password ──Argon2id──► KEK ─wrap─┐
recovery key ────────► RK  ─wrap─┼──► data key (256 bit, random)
Touch ID key ────────► BK  ─wrap─┘        │
                                    HKDF ├──► SQLCipher key
                                         └──► attachment key
```

| | |
|---|---|
| Database | SQLCipher (AES-256), including the full-text index and the embeddings |
| Attachments | XChaCha20-Poly1305 with a key of their own; file names are keyed hashes of the content |
| Key derivation | Argon2id, 128 MiB, 3 passes, 4 lanes |
| Key hierarchy | One random data key, wrapped separately for password, recovery key and Touch ID. Changing the password re-wraps it; the database is not re-encrypted |
| Keys in memory | Held in `zeroize` types. Locking closes every connection, drops graph, vectors and caches, wipes the keys and clears the interface |
| Backups | Encrypted copies of the database, verified after writing |
| Network | Only update checks, which can be turned off, the optional model download, and reading a public GitHub repository when the user imports a skill from it. Entries never leave the Mac. No telemetry |

The plaintext `vault.header` holds the KDF parameters and the wrapped keys, never content.

## AI access

- The MCP server listens on `127.0.0.1` only. Requests with a browser `Origin` header are rejected, against DNS rebinding.
- Each client has its own token (`bm_…`, 256 bit). Only its SHA-256 hash is stored, and the app never shows it after setup.
- Tokens are limited to `read`, `write` and `delete`, and optionally to certain projects.
- While the vault is locked, a request can ask the user for Touch ID or, without it or after the prompt's password button, for the master password in a window of the app. The Mac's login password never opens the vault. Only one request is open at a time, and a declined client is not asked again for a while.
- Entry content is untrusted data. Markdown is sanitized before it is shown, and clients are told not to follow instructions inside entries.
- What an AI writes is attributed to it and marked as unreviewed. Deleting is always a soft delete, and every change set can be undone.
- A file link an AI creates stays unreadable until the user approves it. Clients can read only linked text files.
- Only the user can mark entries as reviewed, erase them for good, manage backups or change settings.
- The skill menu writes only into folders named with Brainmatter's prefix whose `SKILL.md` carries its mark, so the user's own skills are never touched. Files are replaced atomically and never written through a symbolic link, paths with `..` are refused, and a skill's `.claude-plugin` folder is left out, so a skill cannot arrive as a plugin.
- Logs never contain entry content.

## Touch ID without an Apple Developer account

Releases from GitHub are signed ad hoc, without an Apple Developer ID. Builds made with `scripts/build-release.sh` use the self-signed identity from `scripts/create-signing-cert.sh`. The Touch ID key sits in the login Keychain, and the app reads it only after `LocalAuthentication` has confirmed a fingerprint. This is a software gate, not a Secure Enclave binding: a process running as you that may read that Keychain item could skip the fingerprint. The cryptographic root stays the password.

macOS binds such Keychain items to the binary, so after every update the item can no longer be read. The app then asks for the password once and sets Touch ID up again.

## What it does not protect against

- Malware running as your user while the vault is unlocked.
- A forgotten password together with a lost recovery key. Nobody can recover the data then, by design.
- A Markdown export, which is plain text. The app says so before writing one.
- The skill menu, which writes into the skill folders of connected apps as plain files: the names and descriptions of commands, templates and skills, and skills set to go directly as files in full. Any program that reads those folders sees them.
- Losing the Mac, unless backups are also copied to a second place (Settings → Backups).
- An AI client you connected doing what its token allows. Limit tokens to what each client needs.
