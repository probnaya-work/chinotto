# AGENTS.md — Contract for coding agents

This file defines how agents must behave when working in the Chinotto repository. Deviations are bugs.

Chinotto is an instrument of PROBNAYA, an independent computational laboratory. PROBNAYA is the
maker and the repository owner; Chinotto keeps its own product identity, and laboratory-wide
repository conventions are recorded in `probnaya-work/.github` (`PROBNAYA.md`).

**Agents must read this file when working here.** It overrides generic tooling defaults (batch commits, branch-name templates, “commit all and push” shortcuts). For commits and git history, **the commit convention below is binding** before writing any commit message or deciding commit boundaries.

**`docs/internal/` is maintainer-local and is not published** (see `.gitignore`). A clone of this
repository does not contain it. References to `docs/internal/...` below are for the maintainer's
working copy; everything an outside contributor needs is in this file and in published `docs/`.

## Commit convention

`type(scope): imperative subject`, optional body. Types: `feat` | `fix` | `refactor` | `perf` |
`chore` | `docs` | `style` | `test` (`ci` is not used here — use `chore`). One logical change per
commit; if the subject needs “and”, split it. Imperative and present tense, lowercase after the
colon, no trailing period, ~72 chars. No vague subjects (“fix bug”, “update stuff”) and no filler
(“WIP”, “quick”, “small”, “hopefully”). Scope only when it locates the change (`entries`, `search`,
`tauri`, `db`). The maintainer's full version, with granularity rules and examples, is
`docs/internal/commit-convention.md`.

---

## What Chinotto is

- **Desktop-first personal thinking tool.** Fast capture, context return, personal knowledge grounding.
- **Place to offload:** thoughts, notes, plans, fragments, work context. One stream, one entity (Entry), no upfront structure.
- **Philosophy:** capture first, structure later. No workspace overhead, no document mindset, no manual organization at write time.
- **Stack:** Tauri 2 (desktop shell), React + TypeScript (UI), SQLite + FTS5 (local storage and full-text search). See `docs/architecture.md`.
- **MVP scope:** Capture (type → Enter → entry), stream (reverse chronological), search (FTS over entries). Desktop only, local-first, single-user.

---

## What Chinotto is not

- A productivity suite, task manager, or project tool.
- A notes app with folders, pages, or documents.
- A markdown editor, kanban, or template system.
- A sync/collab/cloud product as the default (optional Firebase pull sync may be configured; core capture/search stay local-first).
- An AI-first product. AI may be used where it helps; the app must remain useful and understandable without it. No gimmicky or vague AI features that obscure core behavior.

Do not propose or implement features that contradict the above. When in doubt, prefer the minimal interpretation.

---

## Engineering principles

1. **Simplicity and debuggability.** Code and architecture stay simple. A human must be able to reason about data flow and state without tracing through layers of indirection. Prefer explicit over clever.
2. **Local-first.** Data lives in SQLite on the user’s machine. No network for core flows. No dependency on external services for capture or search.
3. **Minimal UI.** UI supports capture and search. No chrome that doesn’t serve those. No “nice to have” UI without a clear product justification.
4. **Boring tech.** Use the stack that’s already there. New dependencies and new patterns need justification; default is “no.”
5. **One logical change per unit of work.** Commits and PRs follow the commit convention above. One feature slice, one fix, one refactor—no bundling unrelated changes.

---

## Behavior expectations for agents

- **Read before changing.** Use the codebase and `docs/` to understand current behavior and constraints. Do not assume; verify paths, APIs, and data shapes.
- **Preserve existing contracts.** Frontend invokes Tauri commands (`create_entry`, `list_entries`, `search_entries`, and others in `docs/internal/architecture.md`). Entry has `id`, `text`, `created_at`. Do not change the entry/search trio or Entry shape without explicit requirement and approval.
- **Follow the commit convention.** Before any commit: re-read the commit convention above (format, types, **granularity** — when to split commits). Every message must match it: `type(scope): imperative subject`, one logical change, no vague or emotional wording. **Unrelated work (e.g. schema + UI polish + unrelated fix) → separate commits.** Do not squash distinct changes into one commit to satisfy a push workflow or automation unless the user **explicitly** asks for a single commit.
- **Do not invent product scope.** Do not add features (e.g. tags, folders, AI chat, sync) unless the user explicitly asks. If the user’s request conflicts with product constraints, state the conflict and ask.
- **Prefer the smallest change.** Fix or add what’s asked. Avoid “while I’m here” refactors or scope creep. Refactors are separate from feature work unless the user asks for both.
- **Leave the codebase buildable and runnable.** Do not leave broken imports, commented-out code that should be removed, or half-finished work. If something is intentionally incomplete (e.g. stub), say so in the change or a short comment.
- **Releases via Git tag / CI.** Shipping a cut is **push `main` and attach the semver tag** to the intended commit (`git tag -f vX.Y.Z`, `git push origin vX.Y.Z --force` when moving the tag). GitHub Actions on tag produces DMG/signing artifacts. **Do not** run `npm run build:macos-app` or other local packaging as part of “doing the release” **unless the user explicitly asks** for a local bundle.
- **Local macOS .app.** Run `npm run build:macos-app` (and copy `Chinotto.app` to `builds/Chinotto-local.app` when that workflow is used) **only when the user asks** for a packaged smoke test or local `.app`—not by default after bundle-affecting edits, and not as a duplicate release step when they rely on the tag workflow.

---

## How agents should make decisions

1. **Product and scope:** Resolve against `docs/internal/product-spec.md` and `docs/architecture.md` (public overview) or `docs/internal/architecture.md` (detail). If the request is ambiguous, choose the option that fits MVP and local-first; ask only when the choice materially affects outcome.
2. **Implementation:** Prefer existing patterns (e.g. how entries are created, how search is invoked). New patterns require a reason (e.g. “current approach doesn’t support X”).
3. **Dependencies:** See “Dependencies and abstractions” below. Default is no new dependency. If a new dep is needed, name it and state why the current stack is insufficient.
4. **Naming and structure:** Follow existing layout (`src/features/entries`, `src-tauri`, `docs/`). New modules go in a logical place; do not proliferate top-level folders or generic names (“utils”, “helpers”) without clear boundaries.
5. **Errors and edge cases:** Handle errors in a way that keeps the app usable (e.g. surface a clear state or message). Do not silently swallow failures that the user should notice. Do not add speculative edge-case handling that isn’t required by the current feature or a known bug.

---

## What agents must avoid

- **Scope creep.** No “and we could also…” unless the user asked for it. No adding docs, tests, or refactors “for completeness” unless that was the task.
- **Vague or gimmicky AI.** No placeholders like “AI-powered search” or “smart suggestions” without a concrete design and acceptance that the app stays useful without them. No wording that sounds like marketing.
- **Over-engineering.** No premature abstractions, no “framework” patterns, no layers that don’t yet have a concrete use. No generic “service” or “manager” classes unless they consolidate real duplication.
- **Ignoring constraints.** No mandatory cloud, no Chinotto accounts, no collaboration. Optional Firestore sync when env is set (`docs/internal/sync.md`; contract `docs/internal/sync/sync.md` in the `probnaya-work/chinotto-mobile` repository). No pages, folders, documents, tasks, kanban, or templates in MVP. No new runtimes or targets (e.g. mobile shell) unless explicitly requested.
- **Breaking the contract.** Do not change Tauri command names, Entry shape, or frontend–backend invocation pattern without explicit requirement. Do not suggest commits that violate the commit convention above.
- **Motivational or filler language.** In code comments, commits, and docs: no “awesome,” “nice,” “simple but powerful,” or similar. Be factual.

---

## Documentation expectations

- **In-code:** Comment only when the “why” or contract is not obvious from the code. No comments that restate what the code does. No TODOs without an owner or next step if they are long-lived.
- **Repo docs:** Public: `docs/privacy.md`, `docs/architecture.md`, `docs/development.md`, README. Internal: `docs/internal/` (product spec, detailed architecture, sync, release). Update public `docs/architecture.md` when stack or high-level design changes; update `docs/internal/product-spec.md` when scope or constraints change. Keep README aligned with run instructions and MVP scope.
- **App Store product URL (for user-facing links):** `https://apps.apple.com/us/app/chinotto-for-mac/id6763830030` — `CHINOTTO_MAC_APP_STORE_URL` in `src/lib/chinottoLinks.ts` (menu **Chinotto → View on Mac App Store…**, Hosting). Mac and iOS are **separate** App Store Connect app records: Mac is "Chinotto for Mac" (`id6763830030`, sku `chinotto-macos`); iOS is "Chinotto" (`id6761345307`, sku `chinotto-ios`). Verified directly against the App Store Connect API on 2026-09-23 — this corrects an earlier, inaccurate note that a single record covered both platforms.
- **Release version bumps:** Follow `docs/release-process.md` (maintainer-local; see `.gitignore`): same semver in `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, and **`src-tauri/Info.plist`** (`CFBundleShortVersionString`, `CFBundleVersion`).
- **AGENTS.md and the commit convention:** Treat these as binding. Do not water them down or add generic “best practices” that duplicate them. When changing them, preserve strictness and practicality.

---

## Dependencies, abstractions, and architecture changes

**Dependencies**

- Adding a dependency (npm or Cargo) requires justification: what problem it solves and why the current stack cannot. Prefer standard library or existing deps. Avoid heavy or opinionated frameworks. Check license and maintenance status.
- Do not add dependencies “for future use.” Add when a concrete feature or fix needs them.

**Abstractions**

- Introduce an abstraction when there is repeated logic or a clear boundary (e.g. API layer, DB layer). Do not abstract “in case we need it later.” Name abstractions by what they do, not by pattern (“EntryRepository” over “DataAccessLayer” if it’s entry-specific).
- Prefer functions and small modules over deep class hierarchies. React components stay focused; shared logic can live in `lib/` or feature-local modules.

**Architecture changes**

- Structural changes (new layers, new runtimes, splitting the backend, changing data model) are out of scope unless the user explicitly requests them. If a task implies such a change, describe the implication and confirm before proceeding.
- When changing architecture, update `docs/architecture.md` and `docs/internal/architecture.md`, and any affected product or commit conventions in the same pass. Do not leave the docs out of date.

---

## Summary

Agents work in Chinotto under a strict, product-aligned contract: minimal scope, local-first, debuggable code, no fluff. **Re-read this file when committing.** Follow product/architecture docs and the “what is / what is not” boundaries. Prefer the smallest change; avoid new deps and abstractions unless justified. Documentation stays accurate and minimal. This file is the source of truth for agent behavior in this repo.
