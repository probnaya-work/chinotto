-- Chinotto v2 — the Record.
--
-- Five objects the user must understand: Fragment, Line, Record, Trace, Return.
-- Governing promise: capture without classification; return with evidence;
-- continue without erasing the earlier moment.
--
-- CANONICAL vs DERIVED is load-bearing here. Canonical tables hold user material and
-- user judgements and are never regenerated. Derived tables are caches: they may be
-- dropped and rebuilt at any time without losing anything a person wrote or decided.
--
-- v1 tables (entries, spaces, pinned_entries, user_themes, entry_themes) are NOT touched
-- by this file. migrate.rs copies out of them and leaves them in place so the migration
-- stays reversible. See migrate.rs.

------------------------------------------------------------------ CANONICAL

-- One captured moment. Small and unclassified by construction: "dinner friday" and a
-- considered paragraph have identical intake status — there is no type, title, space,
-- theme or destination column, and there never should be.
--
-- `body` is the CURRENT wording. Every wording it replaced lives in fragment_revisions,
-- so a correction never destroys what was said. `captured_at` is immutable: correction
-- does not move a moment in time.
CREATE TABLE IF NOT EXISTS fragments (
  id                  TEXT PRIMARY KEY,
  body                TEXT NOT NULL,
  captured_at         TEXT NOT NULL,
  capture_method      TEXT NOT NULL DEFAULT 'typed'
                        CHECK (capture_method IN ('typed','voice','url','shared','imported')),
  capture_origin      TEXT,                       -- 'desktop' | 'tray' | 'mobile' | 'share' | 'legacy'
  corrected_at        TEXT,                       -- NULL = never corrected
  correction_count    INTEGER NOT NULL DEFAULT 0,
  removed_at          TEXT,                       -- soft removal; the Record does not silently lose material

  -- provenance back to v1, preserved and NOT reinterpreted.
  -- legacy_continuation_offset is a UTF-16 offset INSIDE this fragment's own text, which is
  -- what v1 called a "continuation". It is not a Line edge and must never be promoted to one.
  legacy_entry_id             TEXT,
  legacy_continuation_offset  INTEGER,
  legacy_continuation_at      TEXT,
  -- v1 edited entry text in place, so the earlier wordings are already gone and cannot be
  -- recovered. That count is recorded here rather than in correction_count, which must keep
  -- its invariant: correction_count == COUNT(fragment_revisions). Never fabricate a revision.
  legacy_edit_count           INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS fragments_captured_at ON fragments (captured_at DESC);
CREATE INDEX IF NOT EXISTS fragments_legacy_entry ON fragments (legacy_entry_id);

-- Append-only correction history. One row per superseded wording, oldest first.
-- Nothing in the product ever UPDATEs or DELETEs a row here.
CREATE TABLE IF NOT EXISTS fragment_revisions (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  fragment_id     TEXT NOT NULL REFERENCES fragments(id) ON DELETE CASCADE,
  body            TEXT NOT NULL,                  -- the wording BEFORE the correction that created this row
  superseded_at   TEXT NOT NULL,
  revision_index  INTEGER NOT NULL,
  UNIQUE (fragment_id, revision_index)
);

CREATE INDEX IF NOT EXISTS fragment_revisions_fragment
  ON fragment_revisions (fragment_id, revision_index);

-- A Line is not an entity. It emerges from Continue as the transitive closure of these
-- edges, which is why there is no `lines` table and nothing for a user to name or manage.
-- fragment_id is the PRIMARY KEY, so a fragment continues at most one earlier fragment:
-- the closure is always a chain, never a graph.
CREATE TABLE IF NOT EXISTS continuations (
  fragment_id   TEXT PRIMARY KEY REFERENCES fragments(id) ON DELETE CASCADE,  -- the later moment
  continues_id  TEXT NOT NULL REFERENCES fragments(id) ON DELETE CASCADE,     -- what it continues
  linked_at     TEXT NOT NULL,
  origin        TEXT NOT NULL CHECK (origin IN ('explicit','accepted_suggestion')),
  CHECK (fragment_id <> continues_id)
);

CREATE INDEX IF NOT EXISTS continuations_continues ON continuations (continues_id);

-- Keep present. Replaces pins: bounded, with no category and no ordering system.
-- Released rows are kept so a Return can say "you kept this present".
CREATE TABLE IF NOT EXISTS holds (
  fragment_id  TEXT PRIMARY KEY REFERENCES fragments(id) ON DELETE CASCADE,
  held_at      TEXT NOT NULL,
  released_at  TEXT                               -- NULL = currently held
);

-- External material as an ENCOUNTER, not a string.
--
-- This table holds ONLY what actually arrived: the URL exactly as received, whatever the
-- OS share path chose to tell us, and any text the source itself supplied. None of it can
-- fail, because none of it requires the network. A naked URL is a complete fragment.
--
-- `url_key` is a deterministic, offline normalisation of `url_raw` (host + path, lowercased,
-- no scheme, no "www.", no fragment, no tracking parameters). It is the durable identity
-- used to notice that the same source has been met before, computed once at capture rather
-- than re-derived by fuzzy matching later. `url_raw` is never rewritten.
CREATE TABLE IF NOT EXISTS encounters (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  fragment_id    TEXT NOT NULL REFERENCES fragments(id) ON DELETE CASCADE,
  url_raw        TEXT NOT NULL,   -- byte-for-byte as received, before any canonicalisation
  url_key        TEXT NOT NULL,   -- deterministic local normalisation of url_raw
  source_app     TEXT,            -- ONLY when the OS/share path actually provided one
  shared_at      TEXT NOT NULL,
  selected_text  TEXT             -- material the source supplied, distinct from the person's words
);

CREATE INDEX IF NOT EXISTS encounters_fragment ON encounters (fragment_id);
CREATE INDEX IF NOT EXISTS encounters_key ON encounters (url_key);

-- Voice. The AUDIO is the primary source and the only canonical part.
--
-- Capture cannot fail, so a voice fragment exists the moment recording stops, whether or
-- not anything is ever transcribed. Correcting a transcript changes the fragment's body and
-- its revision history; it never touches this table.
CREATE TABLE IF NOT EXISTS voice_captures (
  fragment_id    TEXT PRIMARY KEY REFERENCES fragments(id) ON DELETE CASCADE,
  audio_path     TEXT NOT NULL,
  duration_ms    INTEGER NOT NULL DEFAULT 0,
  recorded_at    TEXT NOT NULL,
  -- Set when the file has been looked for and was not there. The fragment survives; the
  -- interface says the audio is gone rather than pretending it still has it.
  audio_missing  INTEGER NOT NULL DEFAULT 0
);

-- Return history with outcomes. `reason` is a closed set because a Return must be able to
-- state why it came back; opaque engagement scoring may break ties but can never be the reason.
CREATE TABLE IF NOT EXISTS returns (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  fragment_id  TEXT NOT NULL REFERENCES fragments(id) ON DELETE CASCADE,
  surfaced_at  TEXT NOT NULL,
  reason       TEXT NOT NULL CHECK (reason IN (
                 'interval','repeated_language','same_source','line_continued','kept_present','interrupted')),
  outcome      TEXT CHECK (outcome IN ('opened','continued','let_go','expired')),
  outcome_at   TEXT
);

CREATE INDEX IF NOT EXISTS returns_fragment ON returns (fragment_id, surfaced_at DESC);
CREATE INDEX IF NOT EXISTS returns_surfaced ON returns (surfaced_at DESC);

-- The material behind a Return's reason: the shared phrase, the URL, the two dates.
-- A Return with no evidence row is a bug, not a quiet fallback.
CREATE TABLE IF NOT EXISTS return_evidence (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  return_id    INTEGER NOT NULL REFERENCES returns(id) ON DELETE CASCADE,
  kind         TEXT NOT NULL CHECK (kind IN ('shared_phrase','url','date','continuation','hold')),
  detail       TEXT NOT NULL,
  occurred_at  TEXT,
  related_id   TEXT REFERENCES fragments(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS return_evidence_return ON return_evidence (return_id);

-- A user's verdict on an inferred Trace ("not this"). This is a judgement a person made,
-- so it is canonical and survives every recomputation of the traces cache below.
CREATE TABLE IF NOT EXISTS trace_judgements (
  fragment_id  TEXT NOT NULL REFERENCES fragments(id) ON DELETE CASCADE,
  related_id   TEXT NOT NULL REFERENCES fragments(id) ON DELETE CASCADE,
  kind         TEXT NOT NULL,
  judgement    TEXT NOT NULL CHECK (judgement IN ('confirmed','rejected')),
  judged_at    TEXT NOT NULL,
  PRIMARY KEY (fragment_id, related_id, kind)
);

------------------------------------------------------------------ DERIVED (rebuildable)

-- Position of each fragment within its Line. Pure function of `continuations`; kept as a
-- table only so the Record can order a Line at 50k fragments without walking edges.
-- Rebuild with rebuild_line_index().
CREATE TABLE IF NOT EXISTS line_index (
  fragment_id  TEXT PRIMARY KEY REFERENCES fragments(id) ON DELETE CASCADE,
  root_id      TEXT NOT NULL,
  position     INTEGER NOT NULL,
  line_length  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS line_index_root ON line_index (root_id, position);

-- Computed relationships, cached with their derivation metadata so a Trace can always show
-- its evidence. `kind` carries the epistemic status: explicit/same_source/repeated_language
-- are observed; `inferred` is a guess and the interface must say so.
CREATE TABLE IF NOT EXISTS traces (
  fragment_id  TEXT NOT NULL REFERENCES fragments(id) ON DELETE CASCADE,
  related_id   TEXT NOT NULL REFERENCES fragments(id) ON DELETE CASCADE,
  kind         TEXT NOT NULL CHECK (kind IN (
                 'continuation','same_source','repeated_language','inferred','temporal')),
  evidence     TEXT,                              -- the shared words / url / dates themselves
  score        REAL,
  computed_at  TEXT NOT NULL,
  PRIMARY KEY (fragment_id, related_id, kind)
);

CREATE INDEX IF NOT EXISTS traces_fragment ON traces (fragment_id, kind);

-- What the network said about an encounter. Derived: it may be pending, may fail, may
-- change on a later attempt, and may never arrive at all. Nothing here is required for the
-- fragment to be complete.
CREATE TABLE IF NOT EXISTS encounter_enrichment (
  encounter_id   INTEGER PRIMARY KEY REFERENCES encounters(id) ON DELETE CASCADE,
  url_canonical  TEXT,
  domain         TEXT,
  title          TEXT,
  site_name      TEXT,
  extraction     TEXT,            -- optional locally retained source text
  state          TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending','ok','failed')),
  attempted_at   TEXT,
  fetched_at     TEXT,
  failure        TEXT
);

-- What a machine heard. Derived from the audio, which remains the source of truth, and
-- reproducible by transcribing again. A person's corrections do NOT live here — those edit
-- the fragment body and are kept in fragment_revisions like any other wording.
CREATE TABLE IF NOT EXISTS voice_transcripts (
  fragment_id        TEXT PRIMARY KEY REFERENCES fragments(id) ON DELETE CASCADE,
  machine_transcript TEXT,
  state              TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending','ok','failed')),
  model              TEXT,
  transcribed_at     TEXT,
  failure            TEXT
);

-- Find. One index over everything retrievable: what the person wrote, what a transcript says,
-- and what a source was called — kept in separate columns so exact text can outrank a title.
-- Maintained explicitly from Rust (see fts.rs); there are deliberately no triggers, because
-- a fragment's searchable text is assembled from four tables.
CREATE VIRTUAL TABLE IF NOT EXISTS fragments_fts USING fts5(
  body,
  transcript,
  source,
  fragment_id UNINDEXED,
  tokenize = 'unicode61 remove_diacritics 2'
);
