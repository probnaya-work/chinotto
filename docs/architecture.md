# Architecture

High-level overview of the Chinotto desktop app.

## Stack

- **Tauri 2** — desktop shell, Rust backend
- **React + TypeScript** — UI (Vite)
- **SQLite + FTS5** — local storage and full-text search

## Data model

The current Record stores **fragments** with immutable capture times, correction history, continuations, encounters and retained voice audio. The older **Entry** shape (`id`, `text`, `created_at`) remains in SQLite as a compatibility bridge for the deployed text sync contract. The current Record interface does not expose Spaces or recall themes.

## Local-first

SQLite on your machine is the source of truth for capture and search. Entry text is not sent to analytics.

**Optional sync:** when Firebase is configured and you sign in with Apple, text can sync with Chinotto on iPhone. Original audio and correction history remain local. Sync is not required for core use.

## Project layout

```
src/           React UI (features, lib, components)
src-tauri/     Rust app entry, Tauri commands, db/
docs/          Public documentation
```

Maintainer-only specs and release checklists stay local (not in the public tree).

## Frontend ↔ backend

The UI calls Rust via Tauri `invoke()`. Core flows: capture and list fragments, Find, jump-to-date, export and backup. Legacy Entry commands remain for sync compatibility.

## Updates

Production builds check [GitHub Releases](https://github.com/probnaya-work/chinotto/releases/latest) for signed in-app updates.
