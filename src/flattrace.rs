// Vendored flattrace writer (MIT); checked against the upstream writer by
// scripts/check-writer-drift.py. Only level parser/warning visibility is adapted.
#![allow(
    dead_code,
    clippy::manual_is_multiple_of,
    clippy::too_many_arguments,
    clippy::collapsible_if
)]
// SPDX-License-Identifier: MIT
//! Synchronous Linux writer: std + serde_json; each process owns its file.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

pub const CONTEXT_ENV: &str = "FLATTRACE_TRACEPARENT";
pub const LEVEL_ENV: &str = "FLATTRACE_LEVEL";
pub(super) fn level_from_env() -> (u64, Option<String>) {
    let raw = std::env::var_os(LEVEL_ENV).unwrap_or_default();
    let raw = raw.to_string_lossy();
    // Unicode White_Space plus information separators: Python's str.strip set.
    let raw = raw.trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'));
    if raw.is_empty() {
        return (1, None);
    }
    if raw.bytes().all(|b| b.is_ascii_digit()) {
        // u64 is the integer representation used by this writer and serde_json.
        if let Ok(level) = raw.parse::<u64>() {
            return (level, None);
        }
    }
    (1, Some(format!("invalid {LEVEL_ENV} '{raw}' treated as 1")))
}
static WARNED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub(super) fn warn(error: &io::Error) {
    warn_text(&format!("tracing disabled: {error}"));
}
pub(super) fn warn_text(message: &str) {
    let pid = std::process::id();
    if WARNED.swap(pid, std::sync::atomic::Ordering::Relaxed) != pid {
        let _ = writeln!(io::stderr(), "flattrace: {message}");
    }
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn id(bytes: usize) -> io::Result<String> {
    read_id(&mut File::open("/dev/urandom")?, bytes)
}
fn read_id(source: &mut impl Read, bytes: usize) -> io::Result<String> {
    // W3C trace/span ID widths; OS entropy on Linux, never a userspace PRNG.
    const HEX: &[u8] = b"0123456789abcdef";
    let mut raw = vec![0u8; bytes];
    loop {
        source.read_exact(&mut raw)?;
        if raw.iter().any(|v| *v != 0) {
            let mut value = String::with_capacity(bytes * 2);
            for byte in raw {
                value.push(HEX[(byte >> 4) as usize] as char);
                value.push(HEX[(byte & 15) as usize] as char);
            }
            return Ok(value);
        }
    }
}
fn hex(s: &str, width: usize, nonzero: bool) -> bool {
    s.len() == width
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        && (!nonzero || s.bytes().any(|c| c != b'0'))
}
fn context(value: &str) -> io::Result<(String, Option<String>, String)> {
    let p: Vec<_> = value.split('-').collect();
    if p.len() != 4
        || p[0] != "00"
        || !hex(p[1], 32, true)
        || !hex(p[2], 16, true)
        || !hex(p[3], 2, false)
    {
        return Err(invalid("invalid FLATTRACE_TRACEPARENT"));
    }
    Ok((p[1].into(), Some(p[2].into()), p[3].into()))
}
fn wall() -> io::Result<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid("clock before Unix epoch"))?;
    // Gregorian UTC calendar constants, not workload limits.
    let seconds = now.as_secs();
    let mut days = seconds / 86400;
    let mut year = 1970u64;
    let leap = |y: u64| y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    loop {
        let n = if leap(year) { 366 } else { 365 };
        if days < n {
            break;
        }
        days -= n;
        year += 1;
    }
    let months = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0;
    while days >= months[month] {
        days -= months[month];
        month += 1;
    }
    Ok(format!(
        "{year:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:09}Z",
        month + 1,
        days + 1,
        seconds % 86400 / 3600,
        seconds % 3600 / 60,
        seconds % 60,
        now.subsec_nanos()
    ))
}

fn add_note(data: &mut Value, note: &str) {
    if data.get("notes").is_none() {
        data["notes"] = json!([]);
    }
    if !data["notes"].is_array() {
        let previous = data["notes"].take();
        data["notes"] = json!([previous]);
    }
    data["notes"].as_array_mut().unwrap().push(json!(note));
}
/// Capture floats before serde_json::Value erases non-finite values into null.
pub fn set_number(data: &mut Value, key: &str, value: f64) -> io::Result<()> {
    if !data.is_object() {
        return Err(invalid("data must be an object"));
    }
    if !value.is_finite() {
        add_note(data, "non-finite number replaced with null");
    }
    data[key] = json!(value);
    Ok(())
}
// The same masking set as Python; regexes compile only once, on first argv use.
static CREDENTIALS: std::sync::LazyLock<[regex::Regex; 5]> = std::sync::LazyLock::new(|| {
    // Python re.I also folds dotted/dotless i. Python's word set is L/N/underscore;
    // its whitespace set adds the four information separators to Unicode White_Space.
    let name = r"(?:token|password|passwd|secret|ap[iİı][-_]?key|access[-_]?key|key)";
    let space = r"\s\x1c-\x1f";
    let boundary = r"(^|[^\p{L}\p{N}_])";
    [
        format!(r"(?i)^-*{name}$"),
        format!(r"(?i){boundary}(Author[iİı]zat[iİı]on[{space}]*:[{space}]*)(?:Bearer|Bas[iİı]c)[{space}]+[^{space},;]+"),
        format!(r"(?i)(https?://)[^/{space}:@]+:[^/{space}@]+@"),
        format!(r#"(?i){boundary}({name}[{space}]*[=:][{space}]*)(?:"[^"]*"|'[^']*'|[^{space},;&]+)"#),
        format!(r#"(?i)(-+{name}[{space}]+)(?:"[^"]*"|'[^']*'|[^{space},;]+)"#),
    ].map(|pattern| regex::Regex::new(&pattern).expect("static credential pattern"))
});
pub fn sanitize_argv(argv: Vec<String>) -> Vec<String> {
    let mut hide_next = false;
    argv.into_iter()
        .map(|value| {
            if hide_next {
                hide_next = false;
                return "<redacted>".into();
            }
            let rules = &*CREDENTIALS;
            if let Some((name, _)) = value.split_once('=') {
                if rules[0].is_match(name) {
                    return format!("{name}=<redacted>");
                }
            }
            hide_next = rules[0].is_match(&value);
            let value = rules[1].replace_all(&value, "${1}${2}<redacted>");
            let value = rules[2].replace_all(&value, "${1}<redacted>@");
            let value = rules[3].replace_all(&value, "${1}${2}<redacted>");
            rules[4].replace_all(&value, "${1}<redacted>").into_owned()
        })
        .collect()
}
fn object_data(data: Value) -> Value {
    if data.is_object() {
        data
    } else {
        json!({"value":data, "notes":["non-object data wrapped in value"]})
    }
}

pub struct Writer {
    file: Option<File>,
    best_effort: bool,
    tool: String,
    version: String,
    run: String,
    trace: String,
    root: String,
    flags: String,
    pid: u32,
    seq: u64,
    level: u64,
    random: File,
    active: BTreeMap<String, (String, Option<String>, Instant)>,
}
impl Writer {
    pub fn create(
        path: &Path,
        tool: &str,
        version: &str,
        argv: Vec<String>,
        host: &str,
    ) -> io::Result<Self> {
        Self::create_with_data(path, tool, version, argv, host, json!({}))
    }
    pub fn create_with_data(
        path: &Path,
        tool: &str,
        version: &str,
        argv: Vec<String>,
        host: &str,
        data: Value,
    ) -> io::Result<Self> {
        Self::create_at(path, tool, version, argv, host, data, id(16)?, false)
    }
    /// Products use this entry point: off without FLATTRACE_DIR or at level 0;
    /// sink failures never fail the product.
    pub fn from_env(
        tool: &str,
        version: &str,
        argv: Vec<String>,
        host: &str,
        mut data: Value,
    ) -> io::Result<Option<Self>> {
        let (level, note) = level_from_env();
        if level == 0 {
            return Ok(None);
        }
        let directory = match std::env::var_os("FLATTRACE_DIR") {
            Some(v) if !v.is_empty() => std::path::PathBuf::from(v),
            _ => return Ok(None),
        };
        data = object_data(data);
        data["level"] = json!(level);
        if let Some(note) = note {
            add_note(&mut data, &note);
        }
        if tool.is_empty()
            || !tool
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
        {
            return Err(invalid("environment writer tool must be a lowercase slug"));
        }
        if let Err(e) = std::fs::create_dir_all(&directory) {
            warn(&e);
            return Ok(None);
        }
        loop {
            let run = id(16)?;
            let mut stamp = wall()?.replace(['-', ':'], "");
            // Shared filename precision is milliseconds, the Node Date resolution.
            stamp.truncate(stamp.len() - 7); // Remove six sub-ms digits and Z.
            stamp.push('Z');
            let path = directory.join(format!("{stamp}-{tool}-{run}.jsonl"));
            match Self::create_at(
                &path,
                tool,
                version,
                argv.clone(),
                host,
                data.clone(),
                run,
                true,
            ) {
                Ok(mut w) => {
                    w.level = level;
                    return Ok(Some(w));
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) if e.kind() != io::ErrorKind::InvalidInput => {
                    warn(&e);
                    return Ok(None);
                }
                Err(e) => return Err(e),
            }
        }
    }
    fn create_at(
        path: &Path,
        tool: &str,
        version: &str,
        argv: Vec<String>,
        host: &str,
        mut data: Value,
        run: String,
        best_effort: bool,
    ) -> io::Result<Self> {
        data = object_data(data);
        let (trace, parent, flags) = match std::env::var(CONTEXT_ENV) {
            Ok(v) => context(&v)?,
            Err(std::env::VarError::NotPresent) => (id(16)?, None, "00".into()),
            Err(_) => return Err(invalid("invalid FLATTRACE_TRACEPARENT")),
        };
        let mut w = Self {
            file: Some(
                OpenOptions::new()
                    .create_new(true)
                    .append(true)
                    .open(path)?,
            ),
            best_effort,
            tool: tool.into(),
            version: version.into(),
            run,
            trace,
            root: id(8)?,
            flags,
            pid: std::process::id(),
            seq: 0,
            level: 1,
            active: BTreeMap::new(),
            random: File::open("/dev/urandom")?,
        };
        let root = w.root.clone();
        data["argv"] = json!(sanitize_argv(argv));
        data["pid"] = json!(w.pid);
        data["host"] = json!(host);
        data["version"] = json!(version);
        w.begin("run", data, parent, root)?;
        Ok(w)
    }
    fn emit(
        &mut self,
        event: &str,
        state: &str,
        span: &str,
        parent: Option<&str>,
        data: Value,
        end: Option<(&str, f64)>,
        error: Option<Value>,
    ) -> io::Result<()> {
        if std::process::id() != self.pid {
            warn(&invalid(
                "writer inherited across fork; create a new writer",
            ));
            self.file = None;
            return Ok(());
        }
        let data = object_data(data);
        if let Some(e) = &error {
            if !e.is_object() || !e["kind"].is_string() || !e["message"].is_string() {
                return Err(invalid("error requires string kind and message"));
            }
        }
        let mut row = json!({"v":1, "seq":self.seq, "tool":self.tool, "tool_version":self.version,
            "run":self.run, "trace":self.trace, "span":span, "parent":parent,
            "event":event, "state":state, "t":wall()?, "data":data});
        if let Some((outcome, took)) = end {
            row["outcome"] = json!(outcome);
            row["took_ms"] = json!(took);
        }
        if let Some(error) = error {
            row["error"] = error;
        }
        let mut line = serde_json::to_vec(&row)?;
        line.push(b'\n');
        self.seq += 1;
        if let Some(file) = &mut self.file {
            if let Err(e) = file.write_all(&line).and_then(|_| file.flush()) {
                if !self.best_effort {
                    return Err(e);
                }
                warn(&e);
                self.file = None;
            }
        }
        Ok(())
    }
    fn begin(
        &mut self,
        event: &str,
        data: Value,
        parent: Option<String>,
        span: String,
    ) -> io::Result<String> {
        self.emit(event, "start", &span, parent.as_deref(), data, None, None)?;
        self.active
            .insert(span.clone(), (event.into(), parent, Instant::now()));
        Ok(span)
    }
    pub fn level(&self) -> u64 {
        self.level
    }
    pub fn root_span(&self) -> &str {
        &self.root
    }
    pub fn trace_id(&self) -> &str {
        &self.trace
    }
    pub fn traceparent(&self, span: Option<&str>) -> io::Result<String> {
        let span = span.unwrap_or(&self.root);
        if !self.active.contains_key(span) {
            return Err(invalid("inactive span"));
        }
        Ok(format!("00-{}-{span}-{}", self.trace, self.flags))
    }
    pub fn start(&mut self, event: &str, data: Value, parent: Option<&str>) -> io::Result<String> {
        if event == "run" {
            return Err(invalid("run is reserved"));
        }
        let parent = parent.unwrap_or(&self.root).to_owned();
        if !self.active.contains_key(&parent) {
            return Err(invalid("inactive parent"));
        }
        let span = read_id(&mut self.random, 8)?;
        self.begin(event, data, Some(parent), span)
    }
    pub fn point(
        &mut self,
        event: &str,
        mut data: Value,
        span: Option<&str>,
        error: Option<Value>,
    ) -> io::Result<()> {
        if event == "run" {
            return Err(invalid("run is reserved"));
        }
        let mut span = span.unwrap_or(&self.root).to_owned();
        if !self.active.contains_key(&span) {
            if !self.active.contains_key(&self.root) {
                return Ok(());
            }
            data = object_data(data);
            data["late_span"] = json!(span);
            add_note(&mut data, "point after span end");
            span = self.root.clone();
        }
        let parent = self
            .active
            .get(&span)
            .ok_or_else(|| invalid("inactive span"))?
            .1
            .clone();
        self.emit(event, "point", &span, parent.as_deref(), data, None, error)
    }
    pub fn end(
        &mut self,
        span: &str,
        outcome: &str,
        data: Value,
        error: Option<Value>,
    ) -> io::Result<()> {
        if span == self.root {
            let message = "run root can only end through finish()";
            if !self.best_effort {
                return Err(invalid(message));
            }
            warn_text(message);
            return Ok(());
        }
        self.end_span(span, outcome, data, error)
    }
    fn end_span(
        &mut self,
        span: &str,
        outcome: &str,
        data: Value,
        error: Option<Value>,
    ) -> io::Result<()> {
        let (event, parent, begun) = self
            .active
            .get(span)
            .ok_or_else(|| invalid("inactive span"))?
            .clone();
        self.emit(
            &event,
            "end",
            span,
            parent.as_deref(),
            data,
            Some((outcome, begun.elapsed().as_secs_f64() * 1000.0)),
            error,
        )?;
        self.active.remove(span);
        Ok(())
    }
    pub fn finish(mut self, outcome: &str) -> io::Result<()> {
        if self.active.keys().any(|span| span != &self.root) {
            return Err(invalid("local spans still active"));
        }
        self.end_span(&self.root.clone(), outcome, json!({}), None)
    }
    // Dropping a writer closes the file without inventing an end.
}

#[cfg(test)]
#[path = "flattrace_tests.rs"]
mod tests;
