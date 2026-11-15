use super::archive::*;
use super::Db;
use serde_json::json;
use std::{cell::Cell, fs, path::PathBuf};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("chinotto-archive-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn db(&self) -> Db {
        Db::open(self.0.join(format!("{}.db", uuid::Uuid::new_v4()))).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn moment(id: &str) -> serde_json::Value {
    json!({"version":1,"id":id,"body":id,"captured_at":"2020-01-01T10:00:00.000Z","capture_method":"typed"})
}
fn archive() -> Archive {
    let mut first = moment("first");
    first["body"] = json!("new words");
    first["correction_count"] = json!(1);
    first["corrected_at"] = json!("2020-02-01T10:00:00Z");
    first["revisions"] =
        json!([{"body":"old words","superseded_at":"2020-02-01T10:00:00Z","revision_index":0}]);
    first["voice_capture"] = json!({"recorded_at":"2020-01-01T10:00:00Z","audio_incomplete":1});
    first["audio"] = json!({"path":"chinotto-record/audio/first.caf","bytes":5});
    first["encounters"] = json!([{"url_raw":"https://example.com","url_key":"example.com","shared_at":"2020-01-01T10:00:00Z","enrichment":{"state":"ok","title":"original source"}}]);
    let mut later = moment("later");
    later["continuations"] =
        json!([{"continues_id":"first","linked_at":"2021-01-01T10:00:00Z","origin":"explicit"}]);
    Archive::parse(vec![later,first],vec![json!({"id":"old","kind":"theme","archived_at":"2020-01-01T00:00:00Z","body":"legacy words"})]).unwrap()
}
#[test]
fn archive_roundtrip_preserves_identity_history_source_original_audio_and_is_idempotent() {
    let f = Fixture::new();
    let db = f.db();
    let archive = archive();
    let stage = f.0.join("stage");
    fs::create_dir(&stage).unwrap();
    fs::write(stage.join("1.audio"), b"audio").unwrap();
    let plan = db.preview_archive(&archive).unwrap();
    assert_eq!(plan.new_moments, 2);
    db.restore_archive(
        &archive,
        &plan.fingerprint,
        &stage,
        &f.0.join("audio"),
        &f.0.join("recovery"),
        &|| Ok(()),
    )
    .unwrap();
    let conn = db.0.lock().unwrap();
    assert_eq!(
        rows(
            &conn,
            "SELECT body,captured_at,correction_count FROM fragments WHERE id='first'",
            &[]
        )
        .unwrap(),
        vec![
            json!({"body":"new words","captured_at":"2020-01-01T10:00:00.000Z","correction_count":1})
        ]
    );
    assert_eq!(
        rows(&conn, "SELECT continues_id FROM continuations", &[]).unwrap()[0]["continues_id"],
        "first"
    );
    assert_eq!(
        rows(&conn, "SELECT title FROM encounter_enrichment", &[]).unwrap()[0]["title"],
        "original source"
    );
    drop(conn);
    let again = db.preview_archive(&archive).unwrap();
    assert_eq!(again.duplicates, 2);
    assert_eq!(again.new_moments, 0);
    db.restore_archive(
        &archive,
        &again.fingerprint,
        &stage,
        &f.0.join("audio"),
        &f.0.join("recovery"),
        &|| Ok(()),
    )
    .unwrap();
    assert_eq!(fs::read_dir(f.0.join("audio")).unwrap().count(), 1);
    let zip = f.0.join("record.zip");
    db.write_record_archive(&zip, &f.0.join("audio"), &|| Ok(()))
        .unwrap();
    let reimport = read_zip(&zip, &f.0.join("again"), &|| Ok(())).unwrap();
    assert_eq!(reimport.moments.len(), 2);
    assert_eq!(reimport.archived.len(), 1);
    assert!(reimport
        .moments
        .iter()
        .find(|m| m.fragment["id"] == "first")
        .unwrap()
        .audio
        .as_ref()
        .unwrap()
        .0
        .ends_with(".caf"));
}
#[test]
fn failure_and_relock_roll_back_database_and_only_new_audio() {
    for fail_at in [3, 5] {
        let f = Fixture::new();
        let db = f.db();
        let archive = archive();
        let stage = f.0.join("stage");
        fs::create_dir(&stage).unwrap();
        fs::write(stage.join("1.audio"), b"audio").unwrap();
        let audio = f.0.join("audio");
        fs::create_dir(&audio).unwrap();
        fs::write(audio.join("original.caf"), b"keep").unwrap();
        let count = Cell::new(0);
        let plan = db.preview_archive(&archive).unwrap();
        let result = db.restore_archive(
            &archive,
            &plan.fingerprint,
            &stage,
            &audio,
            &f.0.join("recovery"),
            &|| {
                count.set(count.get() + 1);
                if count.get() >= fail_at {
                    Err("record_locked".into())
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err());
        assert_eq!(
            rows(&db.0.lock().unwrap(), "SELECT id FROM fragments", &[])
                .unwrap()
                .len(),
            0
        );
        assert_eq!(fs::read(audio.join("original.caf")).unwrap(), b"keep");
        assert_eq!(fs::read_dir(audio).unwrap().count(), 1);
        assert!(fs::read_dir(f.0.join("recovery")).unwrap().any(|e| e
            .unwrap()
            .path()
            .extension()
            .is_some_and(|x| x == "db")));
    }
}
#[test]
fn rejects_bad_metadata_cycles_and_changed_preview() {
    let mut bad = moment("bad");
    bad["captured_at"] = json!("2026-02-30T00:00:00Z");
    assert!(Archive::parse(vec![bad], vec![]).is_err());
    let mut a = moment("a");
    let mut b = moment("b");
    a["continuations"] =
        json!([{"continues_id":"b","linked_at":"2020-01-01T00:00:00Z","origin":"explicit"}]);
    b["continuations"] =
        json!([{"continues_id":"a","linked_at":"2020-01-01T00:00:00Z","origin":"explicit"}]);
    assert!(Archive::parse(vec![a, b], vec![]).is_err());
    let f = Fixture::new();
    let db = f.db();
    let archive = Archive::parse(vec![moment("first")], vec![]).unwrap();
    let preview = db.preview_archive(&archive).unwrap();
    db.0.lock().unwrap().execute("INSERT INTO fragments(id,body,captured_at,capture_method) VALUES('first','current','2020-01-01T10:00:00.000Z','typed')",[]).unwrap();
    assert!(db
        .restore_archive(
            &archive,
            &preview.fingerprint,
            &f.0,
            &f.0.join("audio"),
            &f.0.join("recovery"),
            &|| Ok(())
        )
        .unwrap_err()
        .contains("changed"));
}
#[test]
fn crash_recovery_removes_uncommitted_audio_and_keeps_committed_recordings() {
    let f = Fixture::new();
    let db = f.db();
    let audio = f.0.join("audio");
    let recovery = f.0.join("recovery");
    fs::create_dir(&audio).unwrap();
    fs::create_dir(&recovery).unwrap();
    let keep = audio.join("restored-keep.caf");
    let orphan = audio.join("restored-orphan.caf");
    fs::write(&keep, b"keep").unwrap();
    fs::write(&orphan, b"orphan").unwrap();
    let conn = db.0.lock().unwrap();
    conn.execute("INSERT INTO fragments(id,body,captured_at,capture_method) VALUES('voice','','2020-01-01T00:00:00Z','voice')",[]).unwrap();
    conn.execute("INSERT INTO voice_captures(fragment_id,audio_path,duration_ms,recorded_at) VALUES('voice',?1,1,'2020-01-01T00:00:00Z')",[keep.to_string_lossy().as_ref()]).unwrap();
    drop(conn);
    fs::write(
        recovery.join("restore-crash.json"),
        serde_json::to_vec(&vec![&keep, &orphan]).unwrap(),
    )
    .unwrap();
    db.recover_restore_audio(&recovery, &audio).unwrap();
    assert!(keep.exists());
    assert!(!orphan.exists());
}
#[test]
fn imports_mobile_zip_and_exports_for_mobile_cross_runtime_acceptance() {
    let source = std::env::var("CHINOTTO_MOBILE_ARCHIVE_FIXTURE").unwrap_or_else(|_|concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mobile-record-v1.zip").into());
    let f = Fixture::new();
    let db = f.db();
    let stage = f.0.join("stage");
    let archive = read_zip(std::path::Path::new(&source), &stage, &|| Ok(())).unwrap();
    let plan = db.preview_archive(&archive).unwrap();
    assert!(plan.new_moments >= 2);
    assert!(plan.audio_files >= 1);
    db.restore_archive(
        &archive,
        &plan.fingerprint,
        &stage,
        &f.0.join("audio"),
        &f.0.join("recovery"),
        &|| Ok(()),
    )
    .unwrap();
    let mut caf = moment("native-caf");
    caf["voice_capture"] = json!({"recorded_at":"2020-01-01T10:00:00Z"});
    caf["audio"] = json!({"path":"chinotto-record/audio/native.caf","bytes":5});
    let caf = Archive::parse(vec![caf], vec![]).unwrap();
    fs::write(stage.join("0.audio"), b"audio").unwrap();
    db.restore_archive(
        &caf,
        &db.preview_archive(&caf).unwrap().fingerprint,
        &stage,
        &f.0.join("audio"),
        &f.0.join("recovery"),
        &|| Ok(()),
    )
    .unwrap();
    if let Ok(destination) = std::env::var("CHINOTTO_NATIVE_ARCHIVE_OUTPUT") {
        db.write_record_archive(
            std::path::Path::new(&destination),
            &f.0.join("audio"),
            &|| Ok(()),
        )
        .unwrap();
    }
}

#[test]
fn exporting_before_sync_keeps_the_latest_authored_correction_restorable() {
    use sha2::{Digest, Sha256};
    let f = Fixture::new();
    let db = f.db();
    let mut root = moment("offline");
    let at = "2020-01-01T10:00:00.000Z";
    let signature = json!(["offline", "offline", at, "capture", []]).to_string();
    let key = format!("{:x}", Sha256::digest(signature.as_bytes()));
    root["wording_versions"] = json!({"selected":key,"versions":{key: {"body":"offline","at":at,"kind":"capture","parents":[]}}});
    let archive = Archive::parse(vec![root], vec![]).unwrap();
    let plan = db.preview_archive(&archive).unwrap();
    db.restore_archive(
        &archive,
        &plan.fingerprint,
        &f.0,
        &f.0.join("audio"),
        &f.0.join("recovery"),
        &|| Ok(()),
    )
    .unwrap();
    db.correct_fragment("offline", "latest unsent words")
        .unwrap();
    let zip = f.0.join("export.zip");
    db.write_record_archive(&zip, &f.0.join("audio"), &|| Ok(()))
        .unwrap();
    let imported = read_zip(&zip, &f.0.join("stage"), &|| Ok(())).unwrap();
    assert_eq!(imported.moments[0].fragment["body"], "latest unsent words");
    let state: serde_json::Value =
        serde_json::from_str(imported.moments[0].wording.as_ref().unwrap()).unwrap();
    assert_eq!(
        state["wire"]["wordings"][state["selected"].as_str().unwrap()]["kind"],
        "correction"
    );
}

#[test]
fn a_missing_staged_file_cannot_leave_a_partial_record_or_recording() {
    let f = Fixture::new();
    let db = f.db();
    let archive = archive();
    let plan = db.preview_archive(&archive).unwrap();
    assert!(db
        .restore_archive(
            &archive,
            &plan.fingerprint,
            &f.0.join("missing"),
            &f.0.join("audio"),
            &f.0.join("recovery"),
            &|| Ok(())
        )
        .is_err());
    assert!(rows(&db.0.lock().unwrap(), "SELECT id FROM fragments", &[])
        .unwrap()
        .is_empty());
    assert_eq!(fs::read_dir(f.0.join("audio")).unwrap().count(), 0);
}

#[test]
fn rejects_traversal_corrupt_zip_content_and_readable_legacy_exports() {
    use std::io::{Read, Seek, SeekFrom, Write};
    let f = Fixture::new();
    for (name, body) in [
        ("chinotto-record/audio/../outside.caf", "audio"),
        ("chinotto-record/record.txt", "old readable export"),
    ] {
        let path = f.0.join("invalid.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(body.as_bytes()).unwrap();
        zip.finish().unwrap();
        assert!(read_zip(&path, &f.0.join("stage"), &|| Ok(())).is_err());
    }
    let path = f.0.join("bad-crc.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    zip.start_file(
        "chinotto-record/record.jsonl",
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
    zip.write_all(moment("one").to_string().as_bytes()).unwrap();
    zip.finish().unwrap();
    let offset = {
        let mut zip = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
        let offset = zip
            .by_name("chinotto-record/record.jsonl")
            .unwrap()
            .data_start();
        offset
    };
    let mut file = fs::File::options()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    file.seek(SeekFrom::Start(offset)).unwrap();
    let mut byte = [0];
    file.read_exact(&mut byte).unwrap();
    file.seek(SeekFrom::Start(offset)).unwrap();
    file.write_all(&[byte[0] ^ 1]).unwrap();
    drop(file);
    assert!(read_zip(&path, &f.0.join("stage"), &|| Ok(())).is_err());
}
