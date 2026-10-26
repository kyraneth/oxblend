//! Offline invocation-bound structural export. No product rerun or upload.
use crate::{Error, Result, bat_cli::hash_bytes, error_help};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{Read, Seek, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub const HELP: &str = "oxblend report --report SAVED.json [--trace TRACE.jsonl] [--stdout STDOUT] --output NEW.zip --preview\noxblend report --trace TRACE.jsonl --output NEW.zip --preview\noxblend report --run-id ID|--last --trace-dir LOCAL_DIR --output NEW.zip --preview\nAcquisition may explicitly opt into --include-names or --include-file FILE (whole-content disclosure).\noxblend report --create PREVIEW.json --preview-sha256 SHA256 --output NEW.zip\noxblend report --discard-preview PREVIEW.json --preview-sha256 SHA256\nOffline closed structural export; default excludes names/paths, raw messages, argv, payload and scene bytes. Unknown kinds/identities use bundle-local HMAC-SHA-256. Preview freezes every sanitized text byte and any explicitly opted-in whole file; create reads only that exact snapshot and refuses overwrite or changed bytes. Trace selection is restricted to the explicit local directory; --last requires a completed failed/partial run with a bound receipt, never mtime. Missing source/trace/exit evidence remains explicit. --stdout verifies saved stdout against the invocation digest. No automatic browser, upload or filing: review the local ZIP, open the fixed public GitHub prefill yourself and attach manually. Saved addresses/topology can be exposed. Uses the native ZIP64 writer and atomic no-replace publication; no invented workload ceiling.";
fn io(e: std::io::Error) -> Error {
    Error::new("io", e.to_string())
}
fn usage(message: &str) -> Error {
    Error::new("usage", message)
}
pub(crate) fn hash(bytes: &[u8]) -> Result<String> {
    hash_bytes(bytes)
}
pub(crate) fn json_bytes(v: &Value) -> Vec<u8> {
    serde_json::to_vec_pretty(v).expect("JSON value")
}
fn decode_hex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|x| u8::from_str_radix(std::str::from_utf8(x).unwrap(), 16).unwrap())
        .collect()
}
pub(crate) fn token(key: &[u8; 32], source: &str) -> Result<String> {
    // RFC 2104 HMAC, reusing the engine's SHA-256 implementation.
    let mut inner = [0x36; 64].to_vec();
    let mut outer = [0x5c; 64].to_vec();
    for i in 0..32 {
        inner[i] ^= key[i];
        outer[i] ^= key[i];
    }
    inner.extend_from_slice(source.as_bytes());
    outer.extend_from_slice(&decode_hex(&hash(&inner)?));
    Ok(format!("kind:{}", hash(&outer)?))
}
pub(crate) fn kind(v: &Value, key: &[u8; 32]) -> Result<Option<String>> {
    v.as_str()
        .map(|s| {
            if error_help::entry(s).is_some() {
                Ok(s.to_owned())
            } else {
                token(key, s)
            }
        })
        .transpose()
}
fn project_warning(v: &Value, key: &[u8; 32]) -> Result<Value> {
    let mut projected = project(v, key)?;
    if let Some(kind) = crate::report_privacy::warning_kind(&v["kind"], key)? {
        if crate::error_help::entry(&kind).is_none() {
            projected["explanation"] = json!("Producer-reported warning; raw message withheld.");
        }
        projected["kind"] = json!(kind);
    }
    Ok(projected)
}
pub(crate) fn project(v: &Value, key: &[u8; 32]) -> Result<Value> {
    let mut result = serde_json::Map::new();
    for (name, field) in [
        ("output_token", "output_path_hex"),
        ("path_token", "path_hex"),
        ("path_token", "path_path_hex"),
        ("destination_token", "destination_path_hex"),
    ] {
        if let Some(value) = v[field].as_str() {
            result.insert(name.into(), json!(token(key, value)?));
        }
    }
    if v["origin"] == "native_atomic_publish" {
        result.insert(
            "publication_scope".into(),
            json!("atomic_file_step; may be staging"),
        );
        result.insert("final_package_certified".into(), json!(false));
    }
    for field in ["cause_id", "cause_run_id"] {
        if let Some(value) = v[field].as_str() {
            result.insert(field.into(), json!(token(key, value)?));
        }
    }
    // No arbitrary keys, messages, path hex aliases or unknown schema strings.
    for name in [
        "complete",
        "planning_complete",
        "output_written",
        "archive_written",
        "directory_created",
        "durability_completed",
        "output_decoder_verified",
        "little_endian",
        "diagnostic_stderr_delivered",
        "stdout_delivery_complete",
    ] {
        if let Some(value) = v.get(name).and_then(Value::as_bool) {
            result.insert(name.into(), json!(value));
        }
    }
    for name in [
        "block_index",
        "element",
        "sdna_index",
        "count",
        "payload_size",
        "data_offset",
        "offset_in_block",
        "decoded_file_offset",
        "blend_version",
        "width",
    ] {
        if let Some(value) = v.get(name).and_then(Value::as_u64) {
            result.insert(name.into(), json!(value));
        }
    }
    if let Some(site) = v["site"].as_str() {
        let structural = site.is_empty()
            || (site.starts_with('/')
                && site.split('/').all(|part| {
                    part.is_empty()
                        || [
                            "files",
                            "roots",
                            "root_traces",
                            "trace",
                            "rewrites",
                            "rewrite",
                        ]
                        .contains(&part)
                        || part.parse::<usize>().is_ok()
                }));
        result.insert(
            "site".into(),
            json!(if structural {
                site.to_owned()
            } else {
                token(key, site)?
            }),
        );
    }
    if let Some(k) = kind(&v["kind"], key)? {
        let explanation = error_help::entry(&k)
            .and_then(|e| e["explanation"].as_str().map(str::to_owned))
            .unwrap_or_else(|| "Unknown producer error; raw kind/message withheld.".into());
        let catalogue = error_help::entry(&k).unwrap_or(Value::Null);
        result.insert("kind".into(), json!(k));
        result.insert("explanation".into(), json!(explanation));
        for field in ["likely_causes", "next_steps", "known_issue"] {
            result.insert(field.into(), catalogue[field].clone());
        }
        result.insert("message_redacted".into(), json!(true));
    }
    for name in [
        "error",
        "errors",
        "warnings",
        "causes",
        "failures",
        "root_traces",
        "diagnostic",
        "reports",
        "operation_state",
        "stdout_report",
        "stderr_report",
        "saved_report",
        "trace",
    ] {
        if let Some(value) = v.get(name) {
            // Preserve historical no-warning derivatives exactly. Only actual
            // nonempty producer warning arrays add diagnostic facts.
            if name == "warnings" && value.as_array().is_none_or(|a| a.is_empty()) {
                continue;
            }
            if let Some(array) = value.as_array() {
                result.insert(
                    name.into(),
                    Value::Array(
                        array
                            .iter()
                            .map(|x| {
                                if name == "warnings" {
                                    project_warning(x, key)
                                } else {
                                    project(x, key)
                                }
                            })
                            .collect::<Result<_>>()?,
                    ),
                );
            } else if value.is_object() {
                result.insert(name.into(), project(value, key)?);
            }
        }
    }
    Ok(Value::Object(result))
}
pub(crate) fn read_regular(path: &Path) -> Result<Vec<u8>> {
    if !std::fs::symlink_metadata(path).map_err(io)?.is_file() {
        return Err(usage(
            "evidence must be an explicit regular file, not a symlink, FIFO or device",
        ));
    }
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| {
            if e.raw_os_error() == Some(libc::ELOOP) {
                usage("symlink evidence refused; supply its explicit regular-file destination")
            } else {
                io(e)
            }
        })?;
    let before = file.metadata().map_err(io)?;
    if !before.is_file() {
        return Err(usage("regular file required"));
    }
    let signature = |m: &std::fs::Metadata| {
        (
            m.dev(),
            m.ino(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
        )
    };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(io)?;
    if signature(&before) != signature(&file.metadata().map_err(io)?)
        || signature(&before) != signature(&std::fs::symlink_metadata(path).map_err(io)?)
        || before.len() != bytes.len() as u64
    {
        return Err(Error::new(
            "input_changed",
            "evidence source changed during capture; nothing published",
        ));
    }
    Ok(bytes)
}
fn create_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(io)?;
    file.write_all(bytes).map_err(io)?;
    file.flush().map_err(io)
}
fn issue_url() -> &'static str {
    "https://github.com/kyraneth/oxblend/issues/new?title=Oxblend%20diagnostic%20report&body=Please%20review%20the%20local%20structural%20bundle%20before%20manually%20attaching%20it.%20No%20scene%20file%20is%20included%20by%20default."
}
fn snapshot(path: &Path, expected: &str) -> Result<Value> {
    if path.file_name().and_then(|s| s.to_str()) != Some("preview.json") {
        return Err(usage(
            "preview.json in an owner-private report stage required",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| usage("snapshot parent required"))?;
    if !parent
        .file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.starts_with(".oxblend-report-"))
    {
        return Err(usage("owner-private report stage name required"));
    }
    let meta = std::fs::symlink_metadata(parent).map_err(io)?;
    if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
        return Err(usage("owner-private snapshot required"));
    }
    let bytes = read_regular(path)?;
    if hash(&bytes)? != expected {
        return Err(Error::new(
            "input_changed",
            "preview digest differs; review a fresh preview",
        ));
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| usage("invalid preview JSON"))?;
    if value["schema"] != "oxblend.report-preview/v1" {
        return Err(usage("unsupported preview schema"));
    }
    Ok(value)
}
pub fn run(args: &[OsString]) -> Result<Value> {
    let mut options = BTreeMap::new();
    let mut flags = std::collections::BTreeSet::new();
    let mut i = 0;
    while i < args.len() {
        let name = args[i]
            .to_str()
            .ok_or_else(|| usage("option must be UTF-8"))?;
        if ["--preview", "--last", "--include-names"].contains(&name) {
            if !flags.insert(name) {
                return Err(usage("duplicate flag"));
            }
            i += 1;
            continue;
        }
        if ![
            "--report",
            "--trace",
            "--trace-dir",
            "--run-id",
            "--include-file",
            "--stdout",
            "--output",
            "--create",
            "--discard-preview",
            "--preview-sha256",
        ]
        .contains(&name)
        {
            return Err(usage(HELP));
        }
        let value = args
            .get(i + 1)
            .ok_or_else(|| usage("option value missing"))?;
        if options.insert(name, value).is_some() {
            return Err(usage("duplicate option"));
        }
        i += 2;
    }
    let get = |name: &str| options.get(name).map(PathBuf::from);
    let acquiring = [
        "--report",
        "--trace",
        "--trace-dir",
        "--run-id",
        "--include-file",
        "--stdout",
    ];
    if options.contains_key("--create") || options.contains_key("--discard-preview") {
        let discard = options.contains_key("--discard-preview");
        if !flags.is_empty()
            || acquiring.iter().any(|k| options.contains_key(k))
            || (discard && options.contains_key("--create"))
            || (discard && options.contains_key("--output"))
        {
            return Err(usage(
                "create/discard accepts only the reviewed receipt, digest and (create) fresh output",
            ));
        }
        let receipt = get(if discard {
            "--discard-preview"
        } else {
            "--create"
        })
        .unwrap();
        let expected = options
            .get("--preview-sha256")
            .and_then(|s| s.to_str())
            .ok_or_else(|| usage("explicit --preview-sha256 required"))?;
        let value = snapshot(&receipt, expected)?;
        let stage = receipt.parent().unwrap();
        let attachment = stage.join("attachment.bin");
        for entry in std::fs::read_dir(stage).map_err(io)? {
            let entry = entry.map_err(io)?;
            let allowed = entry.file_name() == "preview.json"
                || (value["privacy"]["include_file"] == true
                    && entry.file_name() == "attachment.bin");
            if !allowed || !entry.file_type().map_err(io)?.is_file() {
                return Err(usage(
                    "unlisted member or symlink in preview stage; nothing published",
                ));
            }
        }
        if discard {
            if value["privacy"]["include_file"] == true {
                std::fs::remove_file(&attachment).map_err(io)?;
            }
            std::fs::remove_file(&receipt).map_err(io)?;
            std::fs::remove_dir(stage).map_err(io)?;
            return Ok(json!({"discarded":true}));
        }
        let output = get("--output").ok_or_else(|| usage("--output required"))?;
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        if std::fs::canonicalize(parent).map_err(io)?
            != std::fs::canonicalize(stage.parent().unwrap()).map_err(io)?
            || output.file_name().and_then(|s| s.to_str()) != value["output_basename"].as_str()
        {
            return Err(usage("output must match the reviewed parent and basename"));
        }
        let allowed = [
            "manifest.json",
            "README.md",
            "issue.md",
            "error-report.json",
            "error-source.json",
            "build.json",
            "trace/events.jsonl",
            "attachments/user-supplied.bin",
        ];
        let mut members = Vec::new();
        for member in value["members"]
            .as_array()
            .ok_or_else(|| usage("missing member inventory"))?
        {
            let name = member["path"]
                .as_str()
                .ok_or_else(|| usage("missing member path"))?;
            if !allowed.contains(&name) || members.iter().any(|(n, _)| n == name) {
                return Err(usage("unsafe/duplicate snapshot member"));
            }
            let bytes = if name == "attachments/user-supplied.bin" {
                if value["privacy"]["include_file"] != true || !member["preview_text"].is_null() {
                    return Err(usage("attachment inclusion differs from preview"));
                }
                read_regular(&attachment)?
            } else {
                member["preview_text"]
                    .as_str()
                    .ok_or_else(|| usage("text member required"))?
                    .as_bytes()
                    .to_vec()
            };
            if member["bytes"].as_u64() != Some(bytes.len() as u64)
                || member["sha256"] != hash(&bytes)?
            {
                return Err(Error::new(
                    "input_changed",
                    "snapshot member differs from reviewed inventory",
                ));
            }
            members.push((name.to_owned(), bytes));
        }
        for required in &allowed[..6] {
            if !members.iter().any(|(n, _)| n == required) {
                return Err(usage("missing required member"));
            }
        }
        if members
            .iter()
            .any(|(n, _)| n == "attachments/user-supplied.bin")
            != (value["privacy"]["include_file"] == true)
        {
            return Err(usage("file consent/inventory mismatch"));
        }
        let metadata = std::fs::metadata(&receipt).map_err(io)?;
        let mut archive = crate::edit_output::AtomicOutput::create(&metadata, &output)?;
        archive
            .file
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(io)?;
        crate::pack_zip::emit_bytes(&members, &mut archive.file)?;
        archive.file.flush().map_err(io)?;
        archive.file.seek(std::io::SeekFrom::Start(0)).map_err(io)?;
        let archive_sha256 = crate::bat_cli::hash_reader(&mut archive.file)?;
        let publication = archive.publish(crate::edit_output::Durability::None);
        crate::failure_receipt::record_report(
            &json!({"output_written":archive.published,"archive_written":archive.published,"durability_completed":archive.durability_completed,"complete":publication.is_ok()}),
        );
        publication?;
        let cleaned = (|| -> std::io::Result<()> {
            if value["privacy"]["include_file"] == true {
                std::fs::remove_file(&attachment)?;
            }
            std::fs::remove_file(&receipt)?;
            std::fs::remove_dir(stage)
        })()
        .is_ok();
        let inventory=value["members"].as_array().expect("validated snapshot members").iter().map(|m|json!({"path":m["path"],"bytes":m["bytes"],"sha256":m["sha256"],"media_type":m["media_type"]})).collect::<Vec<_>>();
        return Ok(
            json!({"schema":"oxblend.report-created/v1","members":inventory,"output_written":true,"durability_completed":false,"archive_sha256":archive_sha256,"output_path":output.to_str(),"output_path_hex":crate::value::hex(output.as_os_str().as_encoded_bytes()),"snapshot_consumed":cleaned,"preview_authenticity":"caller-reviewed bytes only; not a sanitizer or tool-origin authentication gate","issue_url":issue_url(),"privacy":value["privacy"],"warnings":value["warnings"],"filing":"manual; nothing uploaded"}),
        );
    }
    if !flags.contains("--preview") || options.contains_key("--preview-sha256") {
        return Err(usage(
            "acquisition requires --preview; publication requires reviewed --create receipt and --preview-sha256",
        ));
    }
    let output = get("--output").ok_or_else(|| usage("--output required"))?;
    if output.symlink_metadata().is_ok() {
        return Err(Error::new("output_exists", "fresh report output required"));
    }
    let selected = options.contains_key("--run-id") || flags.contains("--last");
    if selected && (options.contains_key("--report") || options.contains_key("--trace")) {
        return Err(usage(
            "run selection and explicit report/trace are alternative inputs",
        ));
    }
    if flags.contains("--last") && options.contains_key("--run-id") {
        return Err(usage("select either --last or --run-id"));
    }
    if options.contains_key("--trace-dir") != selected {
        return Err(usage(
            "--trace-dir requires explicit --last or --run-id, and vice versa",
        ));
    }
    let mut evidence = if selected {
        crate::report_input::select(
            &get("--trace-dir").unwrap(),
            options.get("--run-id").and_then(|s| s.to_str()),
            flags.contains("--last"),
        )?
    } else {
        let saved = get("--report")
            .map(|p| crate::report_input::saved(&p))
            .transpose()?;
        if let Some(trace) = get("--trace") {
            crate::report_input::trace(&trace, saved)?
        } else {
            saved.ok_or_else(||usage("explicit saved --report, --trace, or selected run required; no rerun/last-failure fallback"))?
        }
    };
    if let Some(stdout) = get("--stdout") {
        let receipt = evidence
            .receipt
            .as_ref()
            .ok_or_else(|| usage("--stdout verification requires an invocation receipt"))?;
        let bytes = read_regular(&stdout)?;
        if receipt["binding"]["stdout_sha256"] != hash(&bytes)?
            || receipt["binding"]["stdout_bytes"].as_u64() != Some(bytes.len() as u64)
        {
            return Err(Error::new(
                "input_changed",
                "saved stdout does not match the invocation receipt",
            ));
        }
        if let Ok(stdout_report) = serde_json::from_slice::<Value>(&bytes) {
            evidence.report = json!({"saved_report":evidence.report,"stdout_report":stdout_report});
        }
        if evidence.events.is_empty() && evidence.saved_kind == "error_envelope" {
            let delivered = receipt["binding"]["stdout_delivery_complete"] == true;
            evidence.binding = if delivered {
                "verified_saved_streams"
            } else {
                "verified_partial_streams"
            };
            evidence.completion = if delivered { "complete" } else { "partial" };
        }
    }
    preview_from_evidence(
        &output,
        &evidence,
        flags.contains("--include-names"),
        get("--include-file").as_deref(),
    )
}
/// Shared preview core for CLI `report --preview` and direct typed library
/// callers: one sanitizer path, one frozen owner-private preview lifecycle,
/// identical member bytes and receipts. `attachment` passes the same explicit
/// regular-file checks the CLI applies to --include-file. CLI serialization
/// and outputs are unchanged; direct callers simply supply typed evidence
/// with honest library provenance instead of selected saved files.
pub(crate) fn preview_from_evidence(
    output: &Path,
    evidence: &crate::report_input::Evidence,
    names: bool,
    attachment: Option<&Path>,
) -> Result<Value> {
    if output.symlink_metadata().is_ok() {
        return Err(Error::new("output_exists", "fresh report output required"));
    }
    let attached = attachment.map(read_regular).transpose()?;
    let mut key = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .map_err(io)?
        .read_exact(&mut key)
        .map_err(io)?;
    let bundle_id = hash(&key)?;
    let projected = crate::report_privacy::evidence(evidence, &key, &bundle_id, names, attached)?;
    key.fill(0);
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let stage = parent.join(format!(".oxblend-report-{bundle_id}"));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&stage)
        .map_err(io)?;
    let receipt = stage.join("preview.json");
    let mut members = Vec::new();
    for (name, bytes) in &projected.files {
        let binary = *name == "attachments/user-supplied.bin";
        if binary {
            create_file(&stage.join("attachment.bin"), bytes)?;
        }
        members.push(json!({"path":name,"bytes":bytes.len(),"sha256":hash(bytes)?,"media_type":if binary{"application/octet-stream"}else if name.ends_with(".jsonl"){"application/x-ndjson"}else if name.ends_with(".json"){"application/json"}else{"text/markdown"},"preview_text":if binary{None}else{Some(std::str::from_utf8(bytes).expect("generated UTF-8"))},"whole_content_disclosure":binary}));
    }
    let frozen = json!({"schema":"oxblend.report-preview/v1","bundle_id":bundle_id,"output_basename":output.file_name().and_then(|s|s.to_str()).ok_or_else(||usage("UTF-8 output basename required"))?,"privacy":projected.privacy,"warnings":projected.warnings,"members":members});
    let bytes = json_bytes(&frozen);
    create_file(&receipt, &bytes)?;
    if read_regular(&receipt)? != bytes {
        return Err(Error::new(
            "input_changed",
            "new preview receipt did not retain the exact frozen bytes",
        ));
    }
    let mut returned = frozen;
    returned["receipt_bytes"] = json!(bytes.len());
    returned["receipt_path"] = json!(receipt.to_str());
    returned["receipt_path_hex"] = json!(crate::value::hex(receipt.as_os_str().as_encoded_bytes()));
    returned["receipt_sha256"] = json!(hash(&bytes)?);
    returned["issue_url"] = json!(issue_url());
    returned["local_input_diagnostics"] = json!(evidence.local_diagnostics);
    returned["preview_authenticity"] = json!(
        "not authenticated; only publish a snapshot obtained directly from your own report --preview, never an untrusted party's preview+digest"
    );
    Ok(returned)
}
/// Owned caller-typed error evidence, without any invocation/receipt claim.
/// Source coordinates come only from the owned snapshot or carried report.
#[derive(Debug, Clone)]
pub struct DirectErrorEvidence {
    pub kind: String,
    pub message: String,
    pub source_snapshot: Option<crate::failure_receipt::SourceSnapshot>,
    pub diagnostics: Vec<Value>,
    pub report: Option<Value>,
}
/// Direct typed-evidence preview from an error's owned snapshot and retained
/// partial report: no CLI invocation, ledger, receipt or saved stream is
/// fabricated. Missing source facts are never recaptured or guessed.
pub fn direct_error_preview(
    output: &Path,
    error: DirectErrorEvidence,
    names: bool,
    attachment: Option<&Path>,
) -> Result<Value> {
    preview_from_evidence(
        output,
        &crate::report_input::direct_error(
            &error.kind,
            &error.message,
            error.source_snapshot.as_ref(),
            &error.diagnostics,
            error.report,
        ),
        names,
        attachment,
    )
}
/// Direct typed-evidence preview from an owned result report (for example one
/// carrying warnings): the same closed sanitizers as a saved CLI report, with
/// library-call provenance instead of any invocation claim.
pub fn direct_result_preview(
    output: &Path,
    report: Value,
    names: bool,
    attachment: Option<&Path>,
) -> Result<Value> {
    preview_from_evidence(
        output,
        &crate::report_input::direct_result(report),
        names,
        attachment,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn hmac_sha256_matches_independent_python_vector() {
        assert_eq!(
            super::token(&[11u8; 32], "Hi There").unwrap(),
            "kind:198a607eb44bfbc69903a0f1cf2bbdc5ba0aa3f3d9ae3c1c7a3b1696a0b68cf7"
        );
    }
}
