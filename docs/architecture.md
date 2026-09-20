# Architecture

High-level overview of the Chinotto desktop app.

## Stack

- **Tauri 2** — desktop shell, Rust backend
- **React + TypeScript** — UI (Vite)
- **SQLite + FTS5** — local storage and full-text search

## Data model

One entity: **Entry** — `id`, `text`, `created_at` (ISO 8601 UTC).

Optional **Spaces** (Inbox, Work, Personal) lens the stream without folders or documents. Spaces are stored locally in SQLite.

Optional **recall themes** label entries for search and context. User-defined themes live in SQLite; when sync is on, the theme catalog and unlocked assignments can sync via Firestore.

## Local-first

SQLite on your machine is the source of truth for capture and search. Entry text is not sent to analytics.

**Optional sync:** when Firebase is configured and you sign in with Apple, entries (and recall themes when enabled) can sync with the Chinotto mobile app. Not required for core use.

**Optional sharing:** creating a share link uploads a time-limited snapshot of selected entries to the hosted read service (`getchinotto.app`). Your full local database is not uploaded.

## Project layout

```
src/           React UI (features, lib, components)
src-tauri/     Rust app entry, Tauri commands, db/
docs/          Public documentation
```

Maintainer-only specs and release checklists stay local (not in the public tree).

## Frontend ↔ backend

The UI calls Rust via Tauri `invoke()`. Core flows: create and list entries, full-text search, optional space filter, jump-to-date, export and backup.

## Updates

Production builds check [GitHub Releases](https://github.com/probnaya-work/chinotto/releases/latest) for signed in-app updates.
