CREATE TABLE IF NOT EXISTS record_sync_account (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  uid TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS record_sync_state (
  fragment_id TEXT PRIMARY KEY NOT NULL REFERENCES fragments(id) ON DELETE CASCADE,
  state TEXT,
  generation INTEGER NOT NULL DEFAULT 1,
  dirty INTEGER NOT NULL DEFAULT 1,
  touched_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  error TEXT
);
CREATE TABLE IF NOT EXISTS record_sync_links (
  fragment_id TEXT PRIMARY KEY NOT NULL REFERENCES fragments(id) ON DELETE CASCADE,
  continues_id TEXT NOT NULL,
  linked_at TEXT NOT NULL,
  origin TEXT NOT NULL CHECK (origin IN ('explicit','accepted_suggestion'))
);
INSERT OR IGNORE INTO record_sync_state (fragment_id, touched_at)
  SELECT id, captured_at FROM fragments WHERE removed_at IS NULL;
CREATE TRIGGER IF NOT EXISTS record_sync_capture AFTER INSERT ON fragments
BEGIN
  INSERT OR IGNORE INTO record_sync_state (fragment_id) VALUES (NEW.id);
END;
CREATE TRIGGER IF NOT EXISTS record_sync_wording AFTER UPDATE OF body, correction_count ON fragments
WHEN NEW.removed_at IS NULL AND (NEW.body <> OLD.body OR NEW.correction_count <> OLD.correction_count)
BEGIN
  INSERT OR IGNORE INTO record_sync_state (fragment_id) VALUES (NEW.id);
  UPDATE record_sync_state SET generation = generation + 1, dirty = 1, error = NULL,
    touched_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE fragment_id = NEW.id;
END;
CREATE TRIGGER IF NOT EXISTS record_sync_continuation AFTER INSERT ON continuations
BEGIN
  UPDATE record_sync_state SET generation = generation + 1, dirty = 1, error = NULL,
    touched_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE fragment_id = NEW.fragment_id;
END;
CREATE TRIGGER IF NOT EXISTS record_sync_resolution AFTER UPDATE OF resolved_at ON wording_conflicts
WHEN NEW.resolved_at IS NOT NULL AND OLD.resolved_at IS NOT NEW.resolved_at
BEGIN
  UPDATE record_sync_state SET generation = generation + 1, dirty = 1, error = NULL,
    touched_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE fragment_id = NEW.fragment_id;
END;
CREATE TRIGGER IF NOT EXISTS record_sync_erasure AFTER UPDATE OF body ON fragments
WHEN NEW.removed_at IS NOT NULL AND NEW.body = ''
BEGIN
  DELETE FROM record_sync_state WHERE fragment_id = NEW.id;
  DELETE FROM record_sync_links WHERE fragment_id = NEW.id OR continues_id = NEW.id;
  UPDATE record_sync_state SET state = json_set(state, '$.wire.continuation', NULL),
    generation = generation + 1, dirty = 1, error = NULL
    WHERE json_extract(state, '$.wire.continuation.continuesId') = NEW.id;
END;
CREATE TRIGGER IF NOT EXISTS record_sync_forget_link AFTER DELETE ON fragments
BEGIN
  DELETE FROM record_sync_links WHERE continues_id = OLD.id;
  UPDATE record_sync_state SET state = json_set(state, '$.wire.continuation', NULL),
    generation = generation + 1, dirty = 1, error = NULL
    WHERE json_extract(state, '$.wire.continuation.continuesId') = OLD.id;
END;

