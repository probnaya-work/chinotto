//! Portable Record archives. Only allowlisted canonical fields can enter SQLite.
use super::{record, Db};
use rusqlite::{params, types::ValueRef, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_META: u64 = 32 * 1024 * 1024;
const INVALID: &str = "The archive contains invalid Record data. Nothing has been restored.";
type Result<T> = std::result::Result<T, String>;
fn sql<T>(r: std::result::Result<T, rusqlite::Error>) -> Result<T> {
    r.map_err(|e| e.to_string())
}
pub fn rows(conn: &Connection, query: &str, args: &[&dyn rusqlite::ToSql]) -> Result<Vec<Value>> {
    let mut stmt = sql(conn.prepare(query))?;
    let names = stmt
        .column_names()
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let mut cursor = sql(stmt.query(args))?;
    let mut output = vec![];
    while let Some(row) = sql(cursor.next())? {
        let mut value = serde_json::Map::new();
        for (i, name) in names.iter().enumerate() {
            let v = match sql(row.get_ref(i))? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => json!(n),
                ValueRef::Real(n) => json!(n),
                ValueRef::Text(t) => {
                    Value::String(std::str::from_utf8(t).map_err(|_| INVALID)?.into())
                }
                _ => return Err(INVALID.into()),
            };
            value.insert(name.clone(), v);
        }
        output.push(Value::Object(value));
    }
    Ok(output)
}
fn string(v: &Value) -> Result<&str> {
    v.as_str().ok_or_else(|| INVALID.into())
}
fn date(v: &Value) -> bool {
    v.as_str()
        .map(|s| chrono::DateTime::parse_from_rfc3339(s).is_ok())
        .unwrap_or(false)
}
fn normalize(value: &Value, name: &str, owner: Option<&str>) -> Result<Value> {
    static FIELDS: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!("archive_fields.json")).expect("archive schema")
    });
    let fields = &*FIELDS;
    if !value.is_object()
        || owner
            .map(|id| {
                value
                    .get("fragment_id")
                    .map(|v| v.as_str() != Some(id))
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    {
        return Err(INVALID.into());
    }
    let mut row = serde_json::Map::new();
    for (key, spec) in fields[name].as_object().unwrap() {
        let rule = spec[0].as_str().unwrap();
        let kind = rule.trim_start_matches('?');
        let v = value
            .get(key)
            .or_else(|| spec.get(1))
            .cloned()
            .unwrap_or(Value::Null);
        let valid = if v.is_null() && rule.starts_with('?') {
            true
        } else if kind == "uint" || kind == "flag" {
            v.as_u64()
                .map(|n| n <= 9_007_199_254_740_991 && (kind != "flag" || n <= 1))
                .unwrap_or(false)
        } else if kind == "date" {
            date(&v)
        } else {
            v.as_str()
                .map(|s| {
                    !s.contains('\0')
                        && if let Some(choices) = kind.strip_prefix("choice:") {
                            choices.split(',').any(|c| c == s)
                        } else {
                            s.chars().count()
                                <= if kind == "text" {
                                    1024 * 1024
                                } else if kind == "id" {
                                    1024
                                } else {
                                    4096
                                }
                                && (kind != "id" || !s.is_empty())
                        }
                })
                .unwrap_or(false)
        };
        if !valid {
            return Err(INVALID.into());
        }
        row.insert(key.clone(), v);
    }
    Ok(Value::Object(row))
}
fn list<'a>(v: &'a Value, key: &str, max: usize) -> Result<Vec<&'a Value>> {
    match v.get(key) {
        None => Ok(vec![]),
        Some(Value::Array(a)) if a.len() <= max => Ok(a.iter().collect()),
        _ => Err(INVALID.into()),
    }
}
fn safe_name(name: &str) -> bool {
    name.starts_with("chinotto-record/")
        && !name.contains(['\\', '\0'])
        && !name.split('/').any(|p| p == ".." || p == ".")
}
#[derive(Clone)]
pub struct Moment {
    pub fragment: Value,
    pub children: BTreeMap<String, Vec<Value>>,
    pub voice: Option<Value>,
    pub transcript: Option<Value>,
    pub conflict: Option<Value>,
    pub audio: Option<(String, u64)>,
    pub wording: Option<String>,
}
#[derive(Clone)]
pub struct Archive {
    pub moments: Vec<Moment>,
    pub archived: Vec<Value>,
}
fn wording(
    input: &Value,
    fragment: &Value,
    revisions: &[Value],
    links: &[Value],
    conflict: &Option<Value>,
) -> Result<Option<String>> {
    if input.is_null() {
        return Ok(None);
    }
    let selected = string(&input["selected"])?;
    let versions = input["versions"].as_object().ok_or(INVALID)?;
    if versions.is_empty()
        || versions.len() > 2048
        || serde_json::to_vec(versions).unwrap().len() > 1_000_000
    {
        return Err(INVALID.into());
    }
    let mut normalized = serde_json::Map::new();
    for (key, node) in versions {
        let body = string(&node["body"])?;
        let at = chrono::DateTime::parse_from_rfc3339(string(&node["at"])?)
            .map_err(|_| INVALID)?
            .with_timezone(&chrono::Utc)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let kind = string(&node["kind"])?;
        let mut parents = node["parents"]
            .as_array()
            .ok_or(INVALID)?
            .iter()
            .map(string)
            .collect::<Result<Vec<_>>>()?;
        if parents.len() > 32
            || body.len() > 1024 * 1024
            || !["capture", "correction", "legacy"].contains(&kind)
        {
            return Err(INVALID.into());
        }
        parents.sort_unstable();
        let signature =
            serde_json::to_string(&json!([fragment["id"], body, at, kind, parents])).unwrap();
        if format!("{:x}", Sha256::digest(signature.as_bytes())) != *key {
            return Err("Record wording identity mismatch".into());
        }
        normalized.insert(
            key.clone(),
            json!({"body":body,"at":at,"kind":kind,"parents":parents}),
        );
    }
    let mut visited = HashSet::new();
    let mut visiting = HashSet::new();
    for key in normalized.keys() {
        let mut stack = vec![(key.as_str(), false)];
        while let Some((key, done)) = stack.pop() {
            if done {
                visiting.remove(key);
                visited.insert(key);
                continue;
            }
            if visited.contains(key) {
                continue;
            }
            if !normalized.contains_key(key) || !visiting.insert(key) {
                return Err(INVALID.into());
            }
            stack.push((key, true));
            for p in normalized[key]["parents"].as_array().unwrap() {
                stack.push((p.as_str().unwrap(), false));
            }
        }
    }
    if normalized.get(selected).map(|v| &v["body"]) != Some(&fragment["body"]) {
        return Err(INVALID.into());
    }
    let revision_keys = normalized
        .iter()
        .filter(|(key, node)| {
            key.as_str() != selected && revisions.iter().any(|r| r["body"] == node["body"])
        })
        .map(|(key, _)| key)
        .collect::<Vec<_>>();
    let link = links.first().map(
        |l| json!({"continuesId":l["continues_id"],"linkedAt":l["linked_at"],"origin":l["origin"]}),
    );
    Ok(Some(json!({"wire":{"version":1,"capturedAt":fragment["captured_at"],"wordings":normalized,"continuation":link},"selected":selected,"revisionCount":revisions.len(),"revisions":revision_keys,"resolvedAt":conflict.as_ref().map(|c|&c["resolved_at"])}).to_string()))
}
impl Archive {
    pub fn parse(lines: Vec<Value>, archived: Vec<Value>) -> Result<Self> {
        if lines.len() > 100000 || archived.len() > 100000 {
            return Err(INVALID.into());
        }
        let mut moments = vec![];
        let mut ids = HashSet::new();
        for input in lines {
            if input["version"] != 1 {
                return Err("This Record archive version is not supported.".into());
            }
            let fragment = normalize(&input, "fragment", None)?;
            let id = string(&fragment["id"])?;
            if !ids.insert(id.to_string()) {
                return Err("The archive contains duplicate moment IDs.".into());
            }
            let mut children = BTreeMap::new();
            for (key, max) in [
                ("revisions", 10000),
                ("continuations", 1),
                ("holds", 1),
                ("encounters", 64),
                ("trace_judgements", 10000),
            ] {
                let mut values = vec![];
                for row in list(&input, key, max)? {
                    let mut normalized = normalize(row, key, Some(id))?;
                    if key == "encounters" && !row["enrichment"].is_null() {
                        normalized["enrichment"] =
                            normalize(&row["enrichment"], "enrichment", None)?;
                    }
                    values.push(normalized);
                }
                children.insert(key.to_string(), values);
            }
            let revisions = children.get_mut("revisions").unwrap();
            revisions.sort_by_key(|r| r["revision_index"].as_u64());
            if revisions.len() as u64 != fragment["correction_count"].as_u64().unwrap()
                || revisions
                    .iter()
                    .enumerate()
                    .any(|(i, r)| r["revision_index"] != i)
                || (!revisions.is_empty() && fragment["corrected_at"].is_null())
            {
                return Err(INVALID.into());
            }
            if children["continuations"]
                .iter()
                .any(|c| c["continues_id"] == id)
            {
                return Err(INVALID.into());
            }
            let optional = |key: &str| -> Result<Option<Value>> {
                if input[key].is_null() {
                    Ok(None)
                } else {
                    Ok(Some(normalize(&input[key], key, Some(id))?))
                }
            };
            let voice = optional("voice_capture")?;
            let transcript = optional("transcript")?;
            let conflict = optional("wording_conflict")?;
            let audio = if input["audio"].is_null() {
                None
            } else {
                let path = string(&input["audio"]["path"])?;
                let size = input["audio"]["bytes"].as_u64().ok_or(INVALID)?;
                if !safe_name(path)
                    || !path.starts_with("chinotto-record/audio/")
                    || path["chinotto-record/audio/".len()..].contains('/')
                    || !(path.ends_with(".m4a") || path.ends_with(".caf"))
                    || size == 0
                    || size > MAX_BYTES
                {
                    return Err(INVALID.into());
                }
                Some((path.to_string(), size))
            };
            if (audio.is_some() && voice.is_none())
                || (voice.is_some()
                    && audio.is_none()
                    && voice.as_ref().unwrap()["audio_missing"] != 1)
            {
                return Err(INVALID.into());
            }
            let wording = wording(
                &input["wording_versions"],
                &fragment,
                &children["revisions"],
                &children["continuations"],
                &conflict,
            )?;
            moments.push(Moment {
                fragment,
                children,
                voice,
                transcript,
                conflict,
                audio,
                wording,
            });
        }
        let parents = moments
            .iter()
            .filter_map(|m| {
                m.children["continuations"].first().map(|c| {
                    (
                        m.fragment["id"].as_str().unwrap(),
                        c["continues_id"].as_str().unwrap(),
                    )
                })
            })
            .collect::<HashMap<_, _>>();
        let mut visited = HashSet::new();
        for id in parents.keys() {
            let mut path = HashSet::new();
            let mut next = Some(*id);
            while let Some(id) = next {
                if !parents.contains_key(id) || visited.contains(id) {
                    break;
                }
                if !path.insert(id) {
                    return Err("The archive contains a continuation cycle.".into());
                }
                next = parents.get(id).copied();
            }
            visited.extend(path);
        }
        let archived = archived
            .iter()
            .map(|v| normalize(v, "archived", None))
            .collect::<Result<Vec<_>>>()?;
        if archived
            .iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect::<HashSet<_>>()
            .len()
            != archived.len()
        {
            return Err(INVALID.into());
        }
        Ok(Self { moments, archived })
    }
}
fn read_lines<R: Read>(file: R) -> Result<Vec<Value>> {
    let mut bytes = vec![];
    file.take(MAX_META + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_META {
        return Err("Archive metadata exceeds the supported 32 MB limit.".into());
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| INVALID)?;
    text.lines()
        .filter(|s| !s.trim().is_empty())
        .map(|line| {
            if line.len() > 2 * 1024 * 1024 {
                return Err(INVALID.into());
            }
            serde_json::from_str(line).map_err(|_| INVALID.into())
        })
        .collect()
}
pub fn read_zip(path: &Path, stage: &Path, check: &impl Fn() -> Result<()>) -> Result<Archive> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > MAX_BYTES {
        return Err("The archive exceeds the supported 2 GB limit.".into());
    }
    let mut zip = zip::ZipArchive::new(File::open(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut names = HashSet::new();
    let mut expanded = 0;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if !safe_name(entry.name())
            || !names.insert(entry.name().to_string())
            || entry.encrypted()
            || entry
                .unix_mode()
                .map(|m| m & 0xf000 == 0xa000)
                .unwrap_or(false)
            || ![
                zip::CompressionMethod::Stored,
                zip::CompressionMethod::Deflated,
            ]
            .contains(&entry.compression())
        {
            return Err(INVALID.into());
        }
        expanded += entry.size();
        if expanded > MAX_BYTES {
            return Err("The expanded archive exceeds the supported 2 GB limit.".into());
        }
        if !entry.name().starts_with("chinotto-record/audio/") && entry.size() > MAX_META {
            return Err(INVALID.into());
        }
    }
    if !names.contains("chinotto-record/record.jsonl") {
        return Err("This older export contains readable text only. A full Record archive is required for restoration.".into());
    }
    if names.contains("chinotto-record/manifest.json") {
        let mut text = String::new();
        zip.by_name("chinotto-record/manifest.json")
            .map_err(|e| e.to_string())?
            .take(4097)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        let manifest: Value = serde_json::from_str(&text).map_err(|_| INVALID)?;
        if manifest["format"] != "chinotto-record" || manifest["version"] != 1 {
            return Err("This Record archive version is not supported.".into());
        }
    }
    check()?;
    let lines = read_lines(
        zip.by_name("chinotto-record/record.jsonl")
            .map_err(|e| e.to_string())?,
    )?;
    let archived = if names.contains("chinotto-record/archived.jsonl") {
        read_lines(
            zip.by_name("chinotto-record/archived.jsonl")
                .map_err(|e| e.to_string())?,
        )?
    } else {
        vec![]
    };
    let archive = Archive::parse(lines, archived)?;
    fs::create_dir_all(stage).map_err(|e| e.to_string())?;
    for (i, m) in archive.moments.iter().enumerate() {
        if let Some((name, size)) = &m.audio {
            let mut entry = zip
                .by_name(name)
                .map_err(|_| "A recording listed in the archive is missing.")?;
            if entry.size() != *size {
                return Err(INVALID.into());
            }
            let mut file =
                File::create(stage.join(format!("{i}.audio"))).map_err(|e| e.to_string())?;
            let mut copied = 0;
            let mut buffer = [0u8; 65536];
            loop {
                check()?;
                let n = entry.read(&mut buffer).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                copied += n as u64;
                if copied > *size {
                    return Err(INVALID.into());
                }
                file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
            }
            if copied != *size {
                return Err(INVALID.into());
            }
        }
    }
    Ok(archive)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub new_moments: usize,
    pub duplicates: usize,
    pub changed: usize,
    pub removed: usize,
    pub waiting_links: usize,
    pub skipped_judgements: usize,
    pub audio_files: usize,
    pub new_archived: usize,
    pub from: Option<String>,
    pub to: Option<String>,
    pub sample: Vec<Value>,
    pub fingerprint: String,
}
fn preview(conn: &Connection, archive: &Archive) -> Result<Preview> {
    let existing = rows(
        conn,
        "SELECT id,body,captured_at,removed_at,correction_count FROM fragments",
        &[],
    )?
    .into_iter()
    .map(|r| (r["id"].as_str().unwrap().to_string(), r))
    .collect::<BTreeMap<_, _>>();
    let blocked = rows(conn, "SELECT id FROM firestore_ingest_suppressed_ids", &[])?
        .into_iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect::<BTreeSet<_>>();
    let old = rows(conn, "SELECT id FROM archived_material", &[])?
        .into_iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect::<BTreeSet<_>>();
    let held: i64 = sql(conn.query_row(
        "SELECT COUNT(*) FROM holds WHERE released_at IS NULL",
        [],
        |r| r.get(0),
    ))?;
    let added = archive
        .moments
        .iter()
        .filter(|m| {
            let id = m.fragment["id"].as_str().unwrap();
            !existing.contains_key(id) && !blocked.contains(id)
        })
        .collect::<Vec<_>>();
    let mut live = existing
        .iter()
        .filter(|(id, r)| r["removed_at"].is_null() && !blocked.contains(*id))
        .map(|(id, _)| id.as_str())
        .collect::<HashSet<_>>();
    for m in &added {
        live.insert(m.fragment["id"].as_str().unwrap());
    }
    for m in &archive.moments {
        let id = m.fragment["id"].as_str().unwrap();
        if let Some(r) = existing.get(id) {
            if chrono::DateTime::parse_from_rfc3339(r["captured_at"].as_str().unwrap()).ok()
                != chrono::DateTime::parse_from_rfc3339(m.fragment["captured_at"].as_str().unwrap())
                    .ok()
            {
                return Err("A moment ID belongs to a different capture. The archive cannot be combined safely.".into());
            }
        }
    }
    let new_holds = added
        .iter()
        .filter(|m| {
            m.children["holds"]
                .iter()
                .any(|h| h["released_at"].is_null())
        })
        .count();
    if held + new_holds as i64 > 5 {
        return Err(format!(
            "Release {} held thoughts before restoring this archive.",
            held + new_holds as i64 - 5
        ));
    }
    let mut dates = added
        .iter()
        .map(|m| m.fragment["captured_at"].as_str().unwrap())
        .collect::<Vec<_>>();
    dates.sort_by_key(|s| chrono::DateTime::parse_from_rfc3339(s).unwrap());
    let fingerprint = json!([
        archive
            .moments
            .iter()
            .map(|m| {
                let id = m.fragment["id"].as_str().unwrap();
                json!([id, existing.get(id), blocked.contains(id)])
            })
            .collect::<Vec<_>>(),
        held,
        old
    ]);
    Ok(Preview {
        new_moments:added.len(),duplicates:archive.moments.iter().filter(|m|{let id=m.fragment["id"].as_str().unwrap();existing.get(id).map(|r|r["removed_at"].is_null()&&!blocked.contains(id)).unwrap_or(false)}).count(),
        changed:archive.moments.iter().filter(|m|existing.get(m.fragment["id"].as_str().unwrap()).map(|r|r["removed_at"].is_null()&&(r["body"]!=m.fragment["body"]||r["correction_count"]!=m.fragment["correction_count"])).unwrap_or(false)).count(),
        removed:archive.moments.iter().filter(|m|{let id=m.fragment["id"].as_str().unwrap();blocked.contains(id)||existing.get(id).map(|r|!r["removed_at"].is_null()).unwrap_or(false)}).count(),
        waiting_links:added.iter().filter(|m|m.children["continuations"].iter().any(|c|!live.contains(c["continues_id"].as_str().unwrap()))).count(),
        skipped_judgements:added.iter().map(|m|m.children["trace_judgements"].iter().filter(|j|!live.contains(j["related_id"].as_str().unwrap())).count()).sum(),
        audio_files:added.iter().filter(|m|m.audio.is_some()).count(),new_archived:archive.archived.iter().filter(|r|!old.contains(r["id"].as_str().unwrap())).count(),
        from:dates.first().map(|s|s.to_string()),to:dates.last().map(|s|s.to_string()),
        sample:added.iter().take(3).map(|m|json!({"body":m.fragment["body"].as_str().unwrap().chars().take(220).collect::<String>(),"at":m.fragment["captured_at"]})).collect(),
        fingerprint:format!("{:x}",Sha256::digest(fingerprint.to_string().as_bytes())),
    })
}
fn insert(conn: &Connection, table: &str, row: &Value) -> Result<i64> {
    let row = row.as_object().ok_or(INVALID)?;
    let fields = row.keys().cloned().collect::<Vec<_>>();
    let values = row
        .values()
        .map(|v| match v {
            Value::Null => Ok(rusqlite::types::Value::Null),
            Value::String(s) => Ok(rusqlite::types::Value::Text(s.clone())),
            Value::Number(n) => Ok(rusqlite::types::Value::Integer(n.as_i64().ok_or(INVALID)?)),
            _ => Err(INVALID.to_string()),
        })
        .collect::<Result<Vec<_>>>()?;
    sql(conn.execute(
        &format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            fields.join(","),
            vec!["?"; fields.len()].join(",")
        ),
        rusqlite::params_from_iter(values),
    ))?;
    Ok(conn.last_insert_rowid())
}
fn owned(row: &Value, id: &str) -> Value {
    let mut row = row.clone();
    row["fragment_id"] = json!(id);
    row
}
// Include unsent corrections without changing live sync state or losing concurrent versions.
fn export_wordings(fragment: &Value, state: &Value) -> Result<Value> {
    let mut versions = state["wire"]["wordings"]
        .as_object()
        .ok_or(INVALID)?
        .clone();
    let mut selected = string(&state["selected"])?.to_string();
    let revisions = fragment["revisions"].as_array().ok_or(INVALID)?;
    let count = state["revisionCount"].as_u64().ok_or(INVALID)? as usize;
    if count > revisions.len() || !versions.contains_key(&selected) {
        return Err(INVALID.into());
    }
    fn heads(versions: &serde_json::Map<String, Value>) -> Vec<String> {
        let parents = versions
            .values()
            .filter_map(|n| n["parents"].as_array())
            .flatten()
            .filter_map(Value::as_str)
            .collect::<HashSet<_>>();
        versions
            .keys()
            .filter(|key| !parents.contains(key.as_str()))
            .cloned()
            .collect()
    }
    let add = |versions: &mut serde_json::Map<String, Value>,
               body: &Value,
               at: &Value,
               kind: &str,
               mut parents: Vec<String>|
     -> Result<String> {
        let at = chrono::DateTime::parse_from_rfc3339(string(at)?)
            .map_err(|_| INVALID)?
            .with_timezone(&chrono::Utc)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        parents.sort();
        parents.dedup();
        let key = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&json!([fragment["id"], body, at, kind, parents])).unwrap()
            )
        );
        versions.insert(
            key.clone(),
            json!({"body":body,"at":at,"kind":kind,"parents":parents}),
        );
        Ok(key)
    };
    for index in count..revisions.len() {
        if versions[&selected]["body"] != revisions[index]["body"] {
            return Err(INVALID.into());
        }
        let mut parents = heads(&versions)
            .into_iter()
            .filter(|key| versions[key]["body"] == revisions[index]["body"])
            .collect::<Vec<_>>();
        parents.push(selected);
        selected = add(
            &mut versions,
            revisions
                .get(index + 1)
                .map(|r| &r["body"])
                .unwrap_or(&fragment["body"]),
            &revisions[index]["superseded_at"],
            "correction",
            parents,
        )?;
    }
    if versions[&selected]["body"] != fragment["body"] {
        selected = add(
            &mut versions,
            &fragment["body"],
            &fragment["captured_at"],
            "legacy",
            vec![],
        )?;
    }
    let resolved = &fragment["wording_conflict"]["resolved_at"];
    if !resolved.is_null() && resolved != &state["resolvedAt"] {
        let parents = heads(&versions);
        selected = add(
            &mut versions,
            &fragment["body"],
            resolved,
            "correction",
            parents,
        )?;
    }
    Ok(json!({"selected":selected,"versions":versions}))
}
fn write_journal(path: &Path, paths: &[PathBuf]) -> Result<()> {
    let temporary = path.with_extension("pending");
    let mut file = File::create(&temporary).map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec(paths).unwrap())
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    fs::rename(temporary, path).map_err(|e| e.to_string())
}
impl Db {
    pub fn preview_archive(&self, archive: &Archive) -> Result<Preview> {
        preview(&self.0.lock().unwrap(), archive)
    }
    pub fn restore_archive(
        &self,
        archive: &Archive,
        expected: &str,
        stage: &Path,
        audio_dir: &Path,
        recovery_dir: &Path,
        check: &impl Fn() -> Result<()>,
    ) -> Result<Preview> {
        let mut conn = self.0.lock().unwrap();
        check()?;
        let plan = preview(&conn, archive)?;
        if plan.fingerprint != expected {
            return Err(
                "The Record changed after the preview. Choose the archive again to review it."
                    .into(),
            );
        }
        if plan.new_moments == 0 && plan.new_archived == 0 {
            return Ok(plan);
        }
        fs::create_dir_all(recovery_dir).map_err(|e| e.to_string())?;
        fs::create_dir_all(audio_dir).map_err(|e| e.to_string())?;
        let recovery = recovery_dir.join(format!("before-restore-{}.db", uuid::Uuid::new_v4()));
        sql(conn.execute("VACUUM INTO ?1", [recovery.to_string_lossy().as_ref()]))?;
        check()?;
        let batch = uuid::Uuid::new_v4().to_string();
        let journal = recovery_dir.join(format!("restore-{batch}.json"));
        let mut installed: Vec<PathBuf> = vec![];
        let result = (|| -> Result<()> {
            let tx = sql(conn.transaction())?;
            let existing = rows(
                &tx,
                "SELECT id FROM fragments UNION SELECT id FROM firestore_ingest_suppressed_ids",
                &[],
            )?
            .into_iter()
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect::<HashSet<_>>();
            let added = archive
                .moments
                .iter()
                .enumerate()
                .filter(|(_, m)| !existing.contains(m.fragment["id"].as_str().unwrap()))
                .collect::<Vec<_>>();
            for (i, m) in &added {
                check()?;
                let id = m.fragment["id"].as_str().unwrap();
                insert(&tx, "fragments", &m.fragment)?;
                for (key, table) in [("revisions", "fragment_revisions"), ("holds", "holds")] {
                    for row in &m.children[key] {
                        insert(&tx, table, &owned(row, id))?;
                    }
                }
                for row in &m.children["encounters"] {
                    let mut encounter = owned(row, id);
                    let enrichment = encounter.as_object_mut().unwrap().remove("enrichment");
                    let made = insert(&tx, "encounters", &encounter)?;
                    if let Some(mut enrichment) = enrichment {
                        enrichment["encounter_id"] = json!(made);
                        insert(&tx, "encounter_enrichment", &enrichment)?;
                    }
                }
                if let Some(voice) = &m.voice {
                    let extension = m
                        .audio
                        .as_ref()
                        .map(|(p, _)| if p.ends_with(".caf") { "caf" } else { "m4a" })
                        .unwrap_or("m4a");
                    let destination = audio_dir.join(format!("restored-{batch}-{i}.{extension}"));
                    if m.audio.is_some() {
                        installed.push(destination.clone());
                        write_journal(&journal, &installed)?;
                        // A distinct generated name never overwrites an existing recording.
                        let output = match File::options()
                            .write(true)
                            .create_new(true)
                            .open(&destination)
                        {
                            Ok(file) => file,
                            Err(error) => {
                                installed.pop();
                                write_journal(&journal, &installed)?;
                                return Err(error.to_string());
                            }
                        };
                        drop(output);
                        let source = stage.join(format!("{i}.audio"));
                        // Cache and app data normally share a volume: install verified bytes
                        // with a rename instead of copying a long recording under the DB lock.
                        if fs::rename(&source, &destination).is_err() {
                            let mut input = File::open(source).map_err(|e| e.to_string())?;
                            let mut output = File::options()
                                .write(true)
                                .open(&destination)
                                .map_err(|e| e.to_string())?;
                            let mut buffer = [0u8; 65536];
                            loop {
                                check()?;
                                let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
                                if n == 0 {
                                    break;
                                }
                                output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
                            }
                        }
                        let complete = File::open(&destination).map_err(|e| e.to_string())?;
                        if complete.metadata().map_err(|e| e.to_string())?.len()
                            != m.audio.as_ref().unwrap().1
                        {
                            return Err(INVALID.into());
                        }
                        complete.sync_all().map_err(|e| e.to_string())?;
                    }
                    let mut row = owned(voice, id);
                    row["audio_path"] = json!(destination.to_string_lossy());
                    insert(&tx, "voice_captures", &row)?;
                }
                if let Some(row) = &m.transcript {
                    insert(&tx, "voice_transcripts", &owned(row, id))?;
                }
                if let Some(row) = &m.conflict {
                    insert(&tx, "wording_conflicts", &owned(row, id))?;
                }
                if let Some(state) = &m.wording {
                    sql(tx.execute(
                        "UPDATE record_sync_state SET state=?1,dirty=1 WHERE fragment_id=?2",
                        params![state, id],
                    ))?;
                }
                sql(tx.execute(
                    "INSERT OR IGNORE INTO entries(id,text,created_at,updated_at) VALUES(?1,?2,?3,?4)",
                    params![
                        id,
                        m.fragment["body"].as_str(),
                        m.fragment["captured_at"].as_str(),
                        chrono::Utc::now().to_rfc3339()
                    ],
                ))?;
            }
            let live=rows(&tx,"SELECT id FROM fragments WHERE removed_at IS NULL AND id NOT IN (SELECT id FROM firestore_ingest_suppressed_ids)",&[])?.into_iter().map(|r|r["id"].as_str().unwrap().to_string()).collect::<HashSet<_>>();
            for (_, m) in &added {
                check()?;
                let id = m.fragment["id"].as_str().unwrap();
                for row in &m.children["continuations"] {
                    let parent = row["continues_id"].as_str().unwrap();
                    let removed:bool=sql(tx.query_row("SELECT EXISTS(SELECT 1 FROM fragments WHERE id=?1 AND removed_at IS NOT NULL) OR EXISTS(SELECT 1 FROM firestore_ingest_suppressed_ids WHERE id=?1)",[parent],|r|r.get(0)))?;
                    if !removed {
                        insert(&tx, "record_sync_links", &owned(row, id))?;
                        if live.contains(parent) {
                            insert(&tx, "continuations", &owned(row, id))?;
                        }
                    }
                }
                for row in &m.children["trace_judgements"] {
                    if live.contains(row["related_id"].as_str().unwrap()) {
                        insert(&tx, "trace_judgements", &owned(row, id))?;
                    }
                }
                sql(record::sync_fts(&tx, id))?;
            }
            let pending = rows(&tx, "SELECT l.* FROM record_sync_links l JOIN fragments f ON f.id=l.fragment_id AND f.removed_at IS NULL
                JOIN fragments p ON p.id=l.continues_id AND p.removed_at IS NULL
                WHERE NOT EXISTS(SELECT 1 FROM continuations c WHERE c.fragment_id=l.fragment_id)
                AND NOT EXISTS(SELECT 1 FROM firestore_ingest_suppressed_ids s WHERE s.id=l.fragment_id OR s.id=l.continues_id)", &[])?;
            for link in pending {
                let cycle: bool = sql(tx.query_row("WITH RECURSIVE ancestors(id) AS (
                    SELECT ?1 UNION SELECT c.continues_id FROM continuations c JOIN ancestors a ON c.fragment_id=a.id
                  ) SELECT EXISTS(SELECT 1 FROM ancestors WHERE id=?2)", params![link["continues_id"].as_str(),link["fragment_id"].as_str()], |r|r.get(0)))?;
                if !cycle {
                    insert(&tx, "continuations", &link)?;
                }
            }
            for row in &archive.archived {
                let exists: bool = sql(tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM archived_material WHERE id=?1)",
                    [row["id"].as_str().unwrap()],
                    |r| r.get(0),
                ))?;
                if !exists {
                    insert(&tx, "archived_material", row)?;
                }
            }
            sql(super::migrate::rebuild_line_index(&tx))?;
            check()?;
            sql(tx.commit())?;
            Ok(())
        })();
        if result.is_err() {
            for path in &installed {
                let _ = fs::remove_file(path);
            }
        }
        if result.is_ok() || installed.iter().all(|p| !p.exists()) {
            let _ = fs::remove_file(journal);
        }
        result?;
        Ok(plan)
    }
    /// A crash between copying audio and committing SQLite must not create orphan captures.
    pub fn recover_restore_audio(&self, recovery_dir: &Path, audio_dir: &Path) -> Result<()> {
        let conn = self.0.lock().unwrap();
        let Ok(entries) = fs::read_dir(recovery_dir) else {
            return Ok(());
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("restore-") || !name.ends_with(".json") {
                continue;
            }
            let paths: Vec<PathBuf> =
                serde_json::from_slice(&fs::read(entry.path()).map_err(|e| e.to_string())?)
                    .map_err(|_| INVALID)?;
            for path in paths {
                if path.parent() != Some(audio_dir)
                    || !path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .starts_with("restored-")
                {
                    return Err(INVALID.into());
                }
                let used: bool = sql(conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM voice_captures WHERE audio_path=?1)",
                    [path.to_string_lossy().as_ref()],
                    |r| r.get(0),
                ))?;
                if !used && path.exists() {
                    fs::remove_file(path).map_err(|e| e.to_string())?;
                }
            }
            fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn write_record_archive(
        &self,
        path: &Path,
        audio_dir: &Path,
        check: &impl Fn() -> Result<()>,
    ) -> Result<()> {
        let conn = self.0.lock().unwrap();
        let file = File::create(path).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let mut write = |name: &str, bytes: &[u8]| -> Result<()> {
            zip.start_file(name, options).map_err(|e| e.to_string())?;
            zip.write_all(bytes).map_err(|e| e.to_string())
        };
        write(
            "chinotto-record/manifest.json",
            br#"{"format":"chinotto-record","version":1}"#,
        )?;
        write("chinotto-record/README.txt",b"Chinotto Record archive. record.txt is readable. record.jsonl, archived.jsonl and audio/ can be restored in a current Chinotto build. Existing thoughts are not replaced. This is a private, unencrypted copy.\n")?;
        let fragments=rows(&conn,"SELECT id,body,captured_at,capture_method,capture_origin,corrected_at,correction_count,legacy_entry_id,legacy_edit_count,legacy_continuation_offset,legacy_continuation_at FROM fragments WHERE removed_at IS NULL ORDER BY captured_at,id",&[])?;
        let mut jsonl = Vec::new();
        let mut text = Vec::new();
        let mut recordings = vec![];
        for (i, mut fragment) in fragments.into_iter().enumerate() {
            check()?;
            let id = fragment["id"].as_str().unwrap().to_string();
            fragment["version"] = json!(1);
            for (key, table) in [
                ("revisions", "fragment_revisions"),
                ("holds", "holds"),
                ("trace_judgements", "trace_judgements"),
            ] {
                let order = if key == "revisions" {
                    " ORDER BY revision_index"
                } else {
                    ""
                };
                fragment[key] = json!(rows(
                    &conn,
                    &format!("SELECT * FROM {table} WHERE fragment_id=?1{order}"),
                    &[&id]
                )?);
            }
            fragment["continuations"]=json!(rows(&conn,"SELECT fragment_id,continues_id,linked_at,origin FROM continuations WHERE fragment_id=?1 UNION ALL SELECT fragment_id,continues_id,linked_at,origin FROM record_sync_links WHERE fragment_id=?1 AND NOT EXISTS(SELECT 1 FROM continuations WHERE fragment_id=?1) LIMIT 1",&[&id])?);
            let mut encounters = rows(
                &conn,
                "SELECT * FROM encounters WHERE fragment_id=?1 ORDER BY id",
                &[&id],
            )?;
            for encounter in &mut encounters {
                encounter["enrichment"] = rows(
                    &conn,
                    "SELECT * FROM encounter_enrichment WHERE encounter_id=?1",
                    &[&encounter["id"].as_i64()],
                )?
                .into_iter()
                .next()
                .unwrap_or(Value::Null);
            }
            fragment["encounters"] = json!(encounters);
            for (key, table) in [
                ("voice_capture", "voice_captures"),
                ("transcript", "voice_transcripts"),
                ("wording_conflict", "wording_conflicts"),
            ] {
                fragment[key] = rows(
                    &conn,
                    &format!("SELECT * FROM {table} WHERE fragment_id=?1"),
                    &[&id],
                )?
                .into_iter()
                .next()
                .unwrap_or(Value::Null);
            }
            let state: Option<String> = sql(conn
                .query_row(
                    "SELECT state FROM record_sync_state WHERE fragment_id=?1",
                    [&id],
                    |r| r.get(0),
                )
                .optional())?
            .flatten();
            fragment["wording_versions"] = if let Some(state) = state {
                let state: Value = serde_json::from_str(&state).map_err(|_| INVALID)?;
                export_wordings(&fragment, &state)?
            } else {
                Value::Null
            };
            fragment["audio"] = Value::Null;
            if !fragment["voice_capture"].is_null() {
                let source = PathBuf::from(string(&fragment["voice_capture"]["audio_path"])?);
                match fs::metadata(&source) {
                    Ok(metadata) => {
                        let extension =
                            source.extension().and_then(|e| e.to_str()).ok_or(INVALID)?;
                        if !["m4a", "caf"].contains(&extension)
                            || fs::canonicalize(&source)
                                .map_err(|e| e.to_string())?
                                .parent()
                                != Some(
                                    fs::canonicalize(audio_dir)
                                        .map_err(|e| e.to_string())?
                                        .as_path(),
                                )
                            || !metadata.is_file()
                            || metadata.len() == 0
                        {
                            return Err(INVALID.into());
                        }
                        let name = format!("chinotto-record/audio/{i}.{extension}");
                        fragment["audio"] = json!({"path":name,"bytes":metadata.len()});
                        recordings.push((source, name, metadata.len(), metadata.modified().ok()));
                    }
                    Err(_) if fragment["voice_capture"]["audio_missing"] == 1 => {}
                    Err(_) => {
                        return Err(
                            "A retained recording is missing. The export was not written.".into(),
                        )
                    }
                }
            }
            if let Some(voice) = fragment["voice_capture"].as_object_mut() {
                voice.remove("audio_path");
            }
            text.extend_from_slice(
                format!(
                    "{} · {}{}\n{}\n\n",
                    string(&fragment["captured_at"])?,
                    string(&fragment["capture_method"])?,
                    fragment["capture_origin"]
                        .as_str()
                        .map(|s| format!(" · {s}"))
                        .unwrap_or_default(),
                    string(&fragment["body"])?
                )
                .as_bytes(),
            );
            serde_json::to_writer(&mut jsonl, &fragment).map_err(|e| e.to_string())?;
            jsonl.push(b'\n');
        }
        write("chinotto-record/record.jsonl", &jsonl)?;
        write("chinotto-record/record.txt", &text)?;
        let mut archived = Vec::new();
        for row in rows(
            &conn,
            "SELECT id,kind,archived_at,body FROM archived_material ORDER BY archived_at,id",
            &[],
        )? {
            serde_json::to_writer(&mut archived, &row).map_err(|e| e.to_string())?;
            archived.push(b'\n');
        }
        write("chinotto-record/archived.jsonl", &archived)?;
        drop(write);
        drop(conn); // Audio compression must not hold up a new capture.
        for (source, name, size, modified) in recordings {
            check()?;
            zip.start_file(name, options).map_err(|e| e.to_string())?;
            let mut file = File::open(&source).map_err(|e| e.to_string())?;
            let mut copied = 0;
            let mut buffer = [0; 65536];
            loop {
                check()?;
                let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                copied += n as u64;
                zip.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
            }
            let after = fs::metadata(source).map_err(|e| e.to_string())?;
            if copied != size || after.len() != size || after.modified().ok() != modified {
                return Err("A recording changed during export.".into());
            }
        }
        zip.finish().map_err(|e| e.to_string())?;
        Ok(())
    }
}
