//! Recursive parallel header/structure inventory. One JSONL writer owns stdout.
use crate::{Error, Result};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, mpsc};

fn io(e: std::io::Error) -> Error {
    Error::new("io", e.to_string())
}
fn failure(path: &Path, e: Error) -> Value {
    json!({"status":if e.kind=="input_changed" {"input_changed"} else {"io_error"},
        "file_size":null,"header_end":null,"extent":null,"header_complete":false,
        "pixel_integrity_verified":false,"placeholder_1x1":false,"parts":[],
        "chunks_declared":null,"chunks_present":null,"bytes_expected":null,"truncated":null,
        "error":{"kind":e.kind,"message":e.message},"path":path.to_string_lossy()})
}
/// Return one consistent facts record even for empty, foreign or broken files.
/// Filesystem/identity failures are distinct from malformed EXR content.
pub fn facts(path: &Path) -> Value {
    match crate::exr_header::read_for_scan(path) {
        Ok(h) => h.json(),
        Err(error) => {
            let mut prefix = [0u8; 8];
            let probe = (|| {
                use std::io::Read;
                let mut f = std::fs::File::open(path)?;
                let size = f.metadata()?.len();
                let n = f.read(&mut prefix)?;
                Ok::<_, std::io::Error>((size, n))
            })();
            let Ok((size, n)) = probe else {
                return failure(path, error);
            };
            if error.kind == "input_changed" {
                return failure(path, error);
            }
            let magic = [0x76, 0x2f, 0x31, 0x01];
            let status = if size == 0 {
                "zero_byte"
            } else if prefix[..n.min(4)] != magic[..n.min(4)] {
                "not_exr"
            } else if n < 8 || error.kind == "exr_header_truncated" {
                "header_truncated"
            } else if error.kind == "io" {
                return failure(path, error);
            } else if error.kind == "exr_unsupported" {
                "exr_unsupported"
            } else {
                "header_invalid"
            };
            json!({"status":status,"file_size":size,"header_end":null,"extent":null,"header_complete":false,
                "pixel_integrity_verified":false,"placeholder_1x1":false,"parts":[],
                "chunks_declared":null,"chunks_present":null,"bytes_expected":null,
                "truncated":status=="header_truncated","error":{"kind":error.kind,"message":error.message}})
        }
    }
}
#[derive(Default)]
struct Work {
    queue: VecDeque<PathBuf>,
    pending: usize,
}
#[derive(Default)]
struct Counts {
    files: u64,
    statuses: BTreeMap<String, u64>,
    placeholders: u64,
    truncated: u64,
    zero_byte: u64,
    not_exr: u64,
}
impl Counts {
    fn record(&mut self, v: &Value) {
        self.files += 1;
        *self
            .statuses
            .entry(v["status"].as_str().unwrap_or("io_error").to_owned())
            .or_default() += 1;
        self.placeholders += u64::from(v["placeholder_1x1"] == true);
        self.truncated += u64::from(v["truncated"] == true);
        self.zero_byte += u64::from(v["status"] == "zero_byte");
        self.not_exr += u64::from(v["status"] == "not_exr");
    }
    fn json(&self) -> Value {
        json!({"files":self.files,"counts_by_status":self.statuses,"placeholders":self.placeholders,"truncated":self.truncated,"zero_byte":self.zero_byte,"non_exr":self.not_exr})
    }
}
fn group(path: &Path) -> (PathBuf, Option<String>) {
    let components = path.components().collect::<Vec<_>>();
    for i in 0..components.len().saturating_sub(2) {
        if components[i].as_os_str() == "render" {
            let version = components[i + 1].as_os_str().to_str().unwrap_or("");
            if version.starts_with('v')
                && version.len() > 1
                && version[1..].bytes().all(|c| c.is_ascii_digit())
                && i + 3 < components.len()
            {
                let tree = components[..i + 3].iter().collect::<PathBuf>();
                return (
                    tree,
                    components[i + 2].as_os_str().to_str().map(str::to_owned),
                );
            }
        }
    }
    (path.parent().unwrap_or(Path::new("")).to_owned(), None)
}
fn line(out: &mut impl Write, v: &Value) -> Result<Vec<String>> {
    let causes = crate::failure_receipt::record_report(v);
    let span = crate::diagnostics::Span::new(
        "serialize",
        json!({"role":"report_jsonl","scope":"caller_writer"}),
    );
    let result = (|| {
        serde_json::to_writer(&mut *out, v).map_err(|e| Error::new("io", e.to_string()))?;
        out.write_all(b"\n").and_then(|_| out.flush()).map_err(io)
    })();
    span.result(result).map(|_| causes)
}
/// One record-producing seam shared by the CLI streaming writer and the
/// native binding collector. Each enriched record is emitted in arrival
/// order, then the summary record; `emit` returns the caller's report-ledger
/// causes for the record. Returns the summary record. Never parses JSONL.
pub fn collect(
    inputs: &[PathBuf],
    jobs: usize,
    emit: &mut dyn FnMut(Value) -> Result<Vec<String>>,
) -> Result<Value> {
    if inputs.is_empty() || jobs == 0 {
        return Err(Error::new(
            "usage",
            "exr-scan DIR|FILES... [--jobs N], N must be positive",
        ));
    }
    let cwd = std::env::current_dir().map_err(io)?;
    let inputs = inputs
        .iter()
        .map(|p| {
            if p.is_absolute() {
                p.clone()
            } else {
                cwd.join(p)
            }
        })
        .collect::<Vec<_>>();
    let mut base = if inputs.len() == 1 && inputs[0].is_dir() {
        inputs[0].clone()
    } else {
        inputs[0].parent().unwrap_or(&cwd).to_owned()
    };
    while !inputs.iter().all(|p| p.starts_with(&base)) {
        if !base.pop() {
            break;
        }
    }
    let state = (
        Mutex::new(Work {
            queue: inputs.iter().cloned().collect(),
            pending: inputs.len(),
        }),
        Condvar::new(),
    );
    // At most one completed record per worker waits on the writer. Backpressure
    // is tied to selected parallelism, not an invented file/byte ceiling.
    let (tx, rx) = mpsc::sync_channel(jobs);
    let parent = crate::diagnostics::parent();
    let mut total = Counts::default();
    let mut groups: BTreeMap<PathBuf, (Option<String>, Counts)> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut complete = true;
    std::thread::scope(|scope| -> Result<Value> {
        for _ in 0..jobs {
            let tx = tx.clone();
            let state = &state;
            let parent = &parent;
            scope.spawn(move || {
                loop {
                    let path = {
                        let mut work = state.0.lock().unwrap();
                        loop {
                            if let Some(p) = work.queue.pop_front() {
                                break Some(p);
                            }
                            if work.pending == 0 {
                                break None;
                            }
                            work = state.1.wait(work).unwrap();
                        }
                    };
                    let Some(path) = path else { break };
                    let mut children = Vec::new();
                    let result = (|| -> std::result::Result<Option<Value>, Error> {
                        let meta = std::fs::symlink_metadata(&path).map_err(io)?;
                        if meta.is_dir() {
                            for child in std::fs::read_dir(&path).map_err(io)? {
                                children.push(child.map_err(io)?.path());
                            }
                            return Ok(None);
                        }
                        if meta.file_type().is_symlink() {
                            // Never open a FIFO/device through a symlink.
                            if !std::fs::metadata(&path).map_err(io)?.is_file() {
                                return Ok(None);
                            }
                        } else if !meta.is_file() {
                            return Ok(None);
                        }
                        let span = crate::diagnostics::detail_enabled().then(|| {
                            crate::diagnostics::Span::under(
                                "exr.file",
                                json!({"path":path.to_string_lossy()}),
                                parent.as_deref(),
                            )
                        });
                        let value = facts(&path);
                        if let Some(span) = span {
                            span.end(if value["status"] == "ok" {
                                "ok"
                            } else {
                                "partial"
                            });
                        }
                        Ok(Some(value))
                    })();
                    let value = match result {
                        Ok(v) => v,
                        Err(e) => Some(failure(&path, e)),
                    };
                    if let Some(v) = value
                        && tx.send((path, v)).is_err()
                    {
                        let mut w = state.0.lock().unwrap();
                        w.queue.clear();
                        w.pending = 0;
                        state.1.notify_all();
                        break;
                    }
                    let mut work = state.0.lock().unwrap();
                    // pending can be zero after a broken writer cancels the queue.
                    if work.pending == 0 {
                        break;
                    }
                    work.pending = work.pending - 1 + children.len();
                    work.queue.extend(children);
                    state.1.notify_all();
                }
            });
        }
        drop(tx);
        let mut scan_causes = Vec::new();
        while let Ok((path, mut value)) = rx.recv() {
            if !seen.insert(path.clone()) {
                continue;
            }
            let (tree, family) = group(&path);
            value["record_type"] = json!("file");
            value["schema"] = json!(1);
            value["absolute_path"] = json!(path.to_str());
            value["absolute_path_hex"] =
                json!(crate::value::hex(path.as_os_str().as_encoded_bytes()));
            value["relative_path"] = json!(path.strip_prefix(&base).unwrap_or(&path).to_str());
            value["output_tree"] = json!(tree.to_str());
            value["family"] = json!(family);
            value["size"] = value["file_size"].clone();
            if matches!(value["status"].as_str(), Some("io_error" | "input_changed")) {
                complete = false;
            }
            total.record(&value);
            groups
                .entry(tree)
                .or_insert_with(|| (family, Counts::default()))
                .1
                .record(&value);
            match emit(value.clone()) {
                Ok(causes) => {
                    if matches!(value["status"].as_str(), Some("io_error" | "input_changed")) {
                        scan_causes.extend(causes);
                    }
                }
                Err(error) => {
                    drop(rx);
                    return Err(error);
                }
            }
        }
        let mut summary = total.json();
        summary["record_type"] = json!("summary");
        summary["schema"] = json!(1);
        summary["complete"] = json!(complete);
        summary["jobs"] = json!(jobs);
        summary["relative_root"] = json!(base.to_str());
        summary["relative_root_hex"] =
            json!(crate::value::hex(base.as_os_str().as_encoded_bytes()));
        summary["groups"]=json!(groups.iter().map(|(tree,(family,counts))|json!({"output_tree":tree.to_str(),"family":family,"counts":counts.json()})).collect::<Vec<_>>());
        emit(summary.clone())?;
        if !complete {
            let error = Error::new(
                "incomplete_exr_scan",
                "filesystem errors or changed inputs; inspect file records",
            );
            crate::failure_receipt::record_terminal(&error, &scan_causes);
            return Err(error);
        }
        Ok(summary)
    })
}

/// `jobs` is caller-selected; CLI defaults to available core parallelism.
/// No extension filtering: every regular file gets a record, including non-EXRs.
/// Directory symlinks are not followed; regular-file symlinks are inspected.
pub fn scan(inputs: &[PathBuf], jobs: usize, out: &mut impl Write) -> Result<()> {
    collect(inputs, jobs, &mut |value| line(out, &value)).map(|_| ())
}
