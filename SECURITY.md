# Security Policy

## Sensitive data

Never commit or publish:

- `settings.json`, SQLite databases, exported `ehviewer-backup-*.json` files
- E-Hentai / ExHentai cookies, H@H client IPs or passkeys
- proxy URLs containing credentials, private keys, API keys or tokens
- `docs/skill-handoff.md`, `release/`, `src-tauri/target/` or a raw workspace ZIP

Portable exports exclude session cookies, H@H credentials, local paths, custom network configuration, downloads and reading progress by default. Enabling the export checkbox includes those values and produces a file that must be treated like a password.

## Building releases

Use:

```powershell
pnpm build:release
```

The script remaps local source paths, scans the executable for the current user/repository paths, and refuses to continue if either remains. It also reports unsigned binaries. A valid Windows code-signing certificate is still required before public distribution.

## Sharing source

Share a Git clone/archive rather than copying the development workspace. Ignored build artifacts and local handoff files are not included by Git but are included by ordinary folder ZIP tools.

## Incident response

If a backup containing cookies was shared, invalidate the E-Hentai session by signing out/all sessions where available, then sign in again. If a proxy credential or API key was exposed, rotate it immediately.

Previously committed author metadata or screenshots remain in Git history even after the working tree is cleaned. Removing them completely requires the repository owner to make a deliberate history-rewrite/visibility decision.
