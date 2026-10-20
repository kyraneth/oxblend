use crate::trace::{self, FailureKind, PointerMode, TraceMode, TraceOptions};
use crate::value::PathStep;
use crate::{BlendFile, Error, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
macro_rules! print {
    ($($arg:tt)*) => { write!(std::io::stdout().lock(), $($arg)*).map_err(crate::edit_output::io)? };
}
macro_rules! println {
    () => { writeln!(std::io::stdout().lock()).map_err(crate::edit_output::io)? };
    ($($arg:tt)*) => { writeln!(std::io::stdout().lock(), $($arg)*).map_err(crate::edit_output::io)? };
}
macro_rules! eprintln {
    ($($arg:tt)*) => { writeln!(std::io::stderr().lock(), $($arg)*).map_err(crate::edit_output::io)? };
}
fn usage(s: &str) -> Error {
    Error::new("usage", s)
}
fn arg(s: &OsStr) -> Result<&str> {
    s.to_str().ok_or_else(|| usage("option must be UTF8"))
}
fn absolute(p: &Path) -> Result<PathBuf> {
    Ok(trace::lexical(
        &std::path::absolute(p).map_err(crate::edit_output::io)?,
    ))
}
pub fn pack_arguments(
    command: &str,
    args: &[OsString],
) -> Result<(Vec<PathBuf>, PathBuf, crate::pack::PackOptions)> {
    let sequence = command == "pack-sequence";
    let mut options = crate::pack::PackOptions {
        keep_hierarchy: sequence,
        ..Default::default()
    };
    let (mut target, mut project) = (None, None);
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let s = args[i].to_str();
        match s {
            Some("--") => {
                positionals.extend(args[i + 1..].iter().map(PathBuf::from));
                break;
            }
            Some("-p" | "--project" | "-t" | "--target" | "--durability") => {
                let key = s.unwrap();
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| usage("pack option needs a value"))?;
                match key {
                    "-p" | "--project" => project = Some(PathBuf::from(v)),
                    "-t" | "--target" => target = Some(PathBuf::from(v)),
                    _ => options.durability = arg(v)?.parse()?,
                }
            }
            Some("-e" | "--exclude") => {
                let before = options.exclude.len();
                while let Some(v) = args.get(i + 1) {
                    if v.to_str().is_some_and(|s| s.starts_with('-')) {
                        break;
                    }
                    options
                        .exclude
                        .extend(arg(v)?.split_whitespace().map(str::to_owned));
                    i += 1;
                }
                if options.exclude.len() == before {
                    return Err(usage("--exclude needs glob patterns"));
                }
            }
            Some(v) if v.starts_with("--exclude=") => {
                options
                    .exclude
                    .extend(v[10..].split_whitespace().map(str::to_owned));
                if v.len() == 10 {
                    return Err(usage("--exclude needs glob patterns"));
                }
            }
            Some(v) if v.starts_with("--project=") => project = Some(PathBuf::from(&v[10..])),
            Some(v) if v.starts_with("--target=") => target = Some(PathBuf::from(&v[9..])),
            Some(v) if v.starts_with("--durability=") => options.durability = v[13..].parse()?,
            Some(v)
                if v.starts_with('-')
                    && !v.starts_with("--")
                    && v.len() > 2
                    && v[1..].bytes().all(|c| b"nrc".contains(&c)) =>
            {
                options.noop |= v.contains('n');
                options.relative_only |= v.contains('r');
                options.compress |= v.contains('c');
            }
            Some("-n" | "--noop" | "--dry-run") => options.noop = true,
            Some("-r" | "--relative-only") => options.relative_only = true,
            Some("-c" | "--compress") => options.compress = true,
            Some("--keep-hierarchy") => options.keep_hierarchy = true,
            Some(v) if v.starts_with('-') => return Err(usage("unknown pack option")),
            _ => positionals.push(PathBuf::from(&args[i])),
        }
        i += 1;
    }
    if !sequence {
        if positionals.len() != 2 || target.is_some() {
            return Err(usage("pack INPUT TARGET [-p PROJECT] [-e GLOBS...] [-nrc]"));
        }
        target = positionals.pop();
    }
    let target = target.ok_or_else(|| usage("pack-sequence requires -t TARGET"))?;
    if target.to_str().is_some_and(|s| {
        ["s3://", "http://", "https://"]
            .iter()
            .any(|prefix| s.starts_with(prefix))
    }) {
        return Err(usage(
            "Shaman/S3 transport is not implemented; use a filesystem target",
        ));
    }
    if positionals.is_empty() {
        return Err(usage("pack requires a selected blend input"));
    }
    let inputs = positionals
        .iter()
        .map(|p| absolute(p))
        .collect::<Result<Vec<_>>>()?;
    for p in &inputs {
        if !p.is_file() {
            return Err(Error::new(
                "pack_input",
                "selected input missing or a directory",
            ));
        }
    }
    let project = if let Some(p) = project {
        absolute(&p)?
    } else {
        let mut p = inputs[0]
            .parent()
            .ok_or_else(|| usage("input has no parent"))?
            .to_path_buf();
        for input in &inputs[1..] {
            while !input.parent().unwrap().starts_with(&p) {
                p = p
                    .parent()
                    .ok_or_else(|| usage("no common project"))?
                    .to_path_buf();
            }
        }
        if sequence && inputs.len() > 1 && p.parent().is_none() {
            return Err(usage("common project is filesystem root; provide -p"));
        }
        p
    };
    if !project.exists() {
        return Err(Error::new(
            "pack_project",
            format!(
                "Project directory {} does not exist; nothing was written",
                project.display()
            ),
        ));
    }
    options.project_root = if project.is_file() {
        project.parent().unwrap().to_path_buf()
    } else {
        project
    };
    Ok((inputs, target, options))
}
fn shorten(p: &Path, cwd: &Path) -> String {
    p.strip_prefix(cwd).unwrap_or(p).display().to_string()
}
pub fn run(command: &str, args: &[OsString]) -> Result<()> {
    let mut input = None;
    let mut options = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let v = &args[i];
        if v == "--" {
            if input.is_some() || args.len() != i + 2 {
                return Err(usage("one selected input required"));
            }
            input = Some(&args[i + 1]);
            break;
        }
        if v.to_str().is_some_and(|s| s.starts_with('-')) {
            if command == "blocks" && (v == "-l" || v == "--limit") {
                options.push(v.clone());
                i += 1;
                options.push(
                    args.get(i)
                        .ok_or_else(|| usage("--limit needs a value"))?
                        .clone(),
                );
            } else if command == "blocks" && v.to_str().is_some_and(|s| s.starts_with("--limit=")) {
                options.push(OsString::from("--limit"));
                options.push(OsString::from(&arg(v)?[8..]));
            } else {
                options.push(v.clone());
            }
        } else if input.replace(v).is_some() {
            return Err(usage("one selected input required"));
        }
        i += 1;
    }
    let input = input.ok_or_else(|| usage("list|blocks INPUT [options]"))?;
    if !Path::new(input).exists() {
        return Err(Error::new("missing_input", "selected blend input missing"));
    }
    match command {
        "list" => listing(Path::new(input), &options),
        "blocks" => blocks(Path::new(input), &options),
        _ => Err(usage("unknown command")),
    }
}

fn directory_listing(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(directory)
        .map_err(crate::edit_output::io)?
        .map(|e| e.map(|e| e.path()).map_err(crate::edit_output::io))
        .collect::<Result<Vec<_>>>()?;
    for p in &entries {
        if p.is_file() {
            files.push(p.clone());
        }
    }
    for p in entries {
        if p.is_dir() && !p.is_symlink() {
            directory_listing(&p, files)?;
        }
    }
    Ok(())
}
fn listing(input: &Path, args: &[OsString]) -> Result<()> {
    let started = std::time::Instant::now();
    let (mut json, mut sha, mut timing, mut full, mut low_memory) =
        (false, false, false, false, false);
    for a in args {
        match arg(a)? {
            "--json" => json = true,
            "--sha256" => sha = true,
            "--timing" => timing = true,
            "--full-info" => full = true,
            "--low-memory" if !low_memory => low_memory = true,
            _ => {
                return Err(usage(
                    "list INPUT [--json] [--sha256] [--timing] [--full-info] [--low-memory]",
                ));
            }
        }
    }
    let (report, file_paths) = trace::listing_with_file_paths(
        input,
        TraceOptions {
            mode: TraceMode::Recursive,
            pointers: PointerMode::NonStrict,
            low_memory,
        },
    )?;
    // Keep the stable member-only stdout shape; native-null compatibility is
    // explicit on stderr even for ordinary (non-full-info) listings.
    for warning in &report.warnings {
        if matches!(warning, trace::TraceWarning::LibraryNullPointer { .. }) {
            eprintln!(
                "{}",
                serde_json::json!({"schema":1,"warning":warning.json()})
            );
        }
    }
    for e in &report.errors {
        // Missing physical declarations permit an incomplete listing below.
        // Other incomplete serialized traces still refuse; trace exposes details.
        if !matches!(e.kind, FailureKind::MissingLibrary) {
            return Err(Error::new(
                "incomplete_trace",
                "list refuses incomplete serialized trace",
            ));
        }
    }
    let cwd = std::env::current_dir().map_err(crate::edit_output::io)?;
    let mut groups: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
    let mut text_members: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    let mut directories: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
    let mut missing: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
    let mut missing_error = None;
    for u in &report.usages {
        let owner = PathBuf::from(OsString::from_vec(u.owner_file.clone()));
        let resource = trace::resolve(&owner, &u.asset_path);
        let members = if resource.is_dir() {
            directories
                .entry(owner.clone())
                .or_default()
                .insert(resource.clone());
            let mut members = Vec::new();
            directory_listing(&resource, &mut members)?;
            members
        } else {
            match crate::pack::expand(&resource, u.is_sequence) {
                Ok(members) => members,
                Err(mut error) if error.kind == "missing_asset" => {
                    if u.block_code == b"LI" {
                        error.kind = "missing_library";
                    }
                    error.message = format!(
                        "{}; referenced by block {} in '{}'; use trace for per-usage availability",
                        error.message,
                        u.block_address,
                        crate::value::display_bytes(&u.owner_file)
                    );
                    missing_error.get_or_insert(error);
                    missing
                        .entry(owner.clone())
                        .or_default()
                        .insert(resource.clone());
                    vec![resource]
                }
                Err(error) => return Err(error),
            }
        };
        text_members
            .entry(owner.clone())
            .or_default()
            .extend(members.iter().cloned());
        if !members.is_empty() {
            groups.entry(owner).or_default().extend(members);
        }
    }
    if json {
        if sha || timing {
            eprintln!("--sha256/--timing cannot be used with --json; reporting JSON");
        }
        let mut value = serde_json::Map::new();
        for (owner, paths) in &groups {
            value.insert(
                json_path(owner),
                serde_json::Value::Array(
                    paths
                        .iter()
                        .map(|p| {
                            if missing_error.is_some() {
                                serde_json::json!({"path":json_path(p),"missing":missing.get(owner).is_some_and(|paths| paths.contains(p))})
                            } else {
                                serde_json::Value::String(json_path(p))
                            }
                        })
                        .collect(),
                ),
            );
        }
        if full {
            let mut value = serde_json::json!({"files":value,"usages":report.usages.iter().map(trace::Usage::json).collect::<Vec<_>>(),"packed":packed_resources(input, low_memory)?,"warnings":report.warnings.iter().map(trace::TraceWarning::json).collect::<Vec<_>>()});
            if missing_error.is_some() {
                value["complete"] = serde_json::json!(false);
            }
            println!("{value}");
        } else {
            let value = if missing_error.is_some() {
                serde_json::json!({"complete":false,"files":value})
            } else {
                serde_json::Value::Object(value)
            };
            let pretty = serde_json::to_string_pretty(&value)
                .map_err(|e| Error::new("json", e.to_string()))?;
            // serde_json owns escaping/layout. Double only its structural
            // leading indentation; JSON string whitespace is already escaped.
            let encoded = pretty
                .lines()
                .map(|line| {
                    let indentation = line.bytes().take_while(|&byte| byte == b' ').count();
                    format!("{}{line}", " ".repeat(indentation))
                })
                .collect::<Vec<_>>()
                .join("\n");
            // All four ordinary JSON flag combinations retain UTF-8, use
            // four-space indentation and omit the terminal newline.
            print!("{encoded}");
        }
    } else {
        // Compatibility text follows the root-first selected traversal, not
        // the lexical order of the JSON dependency-map keys.
        // Text has a heading for each owner with an external usage, even
        // when every declared member is missing. JSON has only nonempty groups.
        let mut hash_duration = std::time::Duration::ZERO;
        let mut timed_hash = |path: &Path| -> Result<String> {
            let hash_started = std::time::Instant::now();
            let digest = hash_file(path)?.0;
            hash_duration += hash_started.elapsed();
            Ok(digest)
        };
        for owner in file_paths.iter().filter(|p| text_members.contains_key(*p)) {
            print!("{}", shorten(owner, &cwd));
            if sha {
                print!(" {}", timed_hash(owner)?);
            }
            println!();
            let mut displayed = BTreeSet::new();
            let mut ordered = Vec::new();
            for p in text_members.get(owner).into_iter().flatten() {
                // Directory expansion can supply existing members, never an
                // absent declaration beneath it. Keep that declaration visible.
                if missing.get(owner).is_some_and(|paths| paths.contains(p))
                    || !directories
                        .get(owner)
                        .is_some_and(|dirs| dirs.iter().any(|d| p.starts_with(d)))
                {
                    ordered.push(p.clone());
                }
            }
            if let Some(dirs) = directories.get(owner) {
                for d in dirs {
                    directory_listing(d, &mut ordered)?;
                }
            }
            for p in ordered {
                if !displayed.insert(p.clone()) {
                    continue;
                }
                print!("    {}", shorten(&p, &cwd));
                if missing.get(owner).is_some_and(|paths| paths.contains(&p)) {
                    print!(" [missing]");
                }
                if sha && p.is_file() {
                    print!(" {}", timed_hash(&p)?);
                }
                println!();
            }
        }
        if full {
            for u in &report.usages {
                println!(
                    "    usage {} at {:#x}: {}",
                    crate::value::display_bytes(&u.owner_file),
                    u.block_address,
                    crate::value::display_bytes(&u.asset_path)
                );
            }
        }
        if timing {
            let total = started.elapsed().as_secs_f64();
            println!("Spent {total:.2} seconds on producing this listing");
            if sha {
                let hash_seconds = hash_duration.as_secs_f64();
                println!("Spent {hash_seconds:.2} seconds on calculating SHA sums");
                let percent = if total == 0.0 {
                    0.0
                } else {
                    100.0 * hash_seconds / total
                };
                println!("  (that is {percent:.0}% of the total time");
            }
        }
    }
    match missing_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
fn packed_resources(input: &Path, low_memory: bool) -> Result<Vec<serde_json::Value>> {
    let file = if low_memory {
        BlendFile::open_trace_low_memory(input)?
    } else {
        BlendFile::open_trace(input)?
    };
    let mut out = Vec::new();
    for (i, block) in file.blocks().iter().enumerate() {
        if &block.code == b"DNA1" {
            continue;
        }
        for field in ["packedfile", "packedfiles"] {
            let path = if field == "packedfiles" {
                vec![
                    PathStep::Field(field.as_bytes().to_vec()),
                    PathStep::Field(b"first".to_vec()),
                ]
            } else {
                vec![PathStep::Field(field.as_bytes().to_vec())]
            };
            if let Ok(crate::value::Value::Pointer(address)) = file.get(i, 0, &path, true)
                && address != 0
            {
                out.push(serde_json::json!({"owner_file":json_path(input),"block_address":block.old_address.to_string(),"packed":true,"field":field}));
            }
        }
    }
    file.check_source()?;
    file.data.report(input);
    Ok(out)
}
// Valid UTF-8 keeps the compatibility map shape. Invalid native paths use an
// explicit hex tag, with the tag itself escaped to keep decoding reversible.
fn json_path(path: &Path) -> String {
    let bytes = path.as_os_str().as_encoded_bytes();
    match std::str::from_utf8(bytes) {
        Ok(s) if !s.starts_with("bytes-hex:") && !s.starts_with("utf8:") => s.to_owned(),
        Ok(s) => format!("utf8:{s}"),
        Err(_) => format!("bytes-hex:{}", crate::value::hex(bytes)),
    }
}
fn human(n: usize) -> String {
    if n < 1024 {
        return format!("{n} B");
    }
    let mut value = n as f64;
    for unit in ["kB", "MB", "GB", "TB", "PB", "EB"] {
        value /= 1024.0;
        if value < 1024.0 || unit == "EB" {
            return format!("{value:.1} {unit}");
        }
    }
    unreachable!()
}
fn block_name(file: &BlendFile, i: usize) -> Result<String> {
    let block = file.block(i)?;
    let ty = String::from_utf8_lossy(file.dna_type_id(i)?);
    let end = block.code.iter().position(|&b| b == 0).unwrap_or(4);
    Ok(format!(
        "{ty}-{}",
        String::from_utf8_lossy(&block.code[..end])
    ))
}
fn byte_repr(bytes: &[u8]) -> String {
    let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'\"') {
        b'\"'
    } else {
        b'\''
    };
    let mut out = format!("b{}", quote as char);
    for &b in bytes {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            b if b == quote => {
                out.push('\\');
                out.push(b as char);
            }
            32..=126 => out.push(b as char),
            _ => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    out.push(quote as char);
    out
}
fn path_repr(path: &[PathStep]) -> String {
    let entries = path
        .iter()
        .map(|s| match s {
            PathStep::Field(v) => byte_repr(v),
            PathStep::Index(v) => v.to_string(),
        })
        .collect::<Vec<_>>();
    if entries.len() == 1 {
        entries[0].clone()
    } else {
        format!("({})", entries.join(", "))
    }
}
fn references<'a>(
    file: &'a BlendFile,
    block: usize,
    si: usize,
    source: (usize, &[u8]),
    prefix: &mut Vec<&'a [u8]>,
    address: u64,
    found: &mut bool,
) -> Result<()> {
    let (base, bytes) = source;
    for field in &file.schema().structs[si].fields {
        let offset = crate::checked::add(base, field.offset)?;
        prefix.push(&field.name);
        for element in 0..field.array_len {
            let at =
                crate::checked::add(offset, crate::checked::mul(element, field.element_size)?)?;
            if field.pointer {
                let end = crate::checked::add(at, file.header().pointer_size)?;
                let raw = bytes
                    .get(at..end)
                    .ok_or_else(|| Error::new("truncated", "reference field outside record"))?;
                let value = raw.iter().enumerate().fold(0u64, |n, (i, &b)| {
                    let shift = if file.header().little_endian {
                        i
                    } else {
                        raw.len() - 1 - i
                    } * 8;
                    n | (u64::from(b) << shift)
                });
                if value == address {
                    let b = file.block(block)?;
                    let ty = String::from_utf8_lossy(file.dna_type_id(block)?);
                    let end = b.code.iter().position(|&b| b == 0).unwrap_or(4);
                    let path = prefix
                        .iter()
                        .map(|s| PathStep::Field(s.to_vec()))
                        .collect::<Vec<_>>();
                    println!(
                        "     <BlendFileBlock.{ty} ({}), size={} at {:#x}> {}",
                        String::from_utf8_lossy(&b.code[..end]),
                        b.size,
                        b.old_address,
                        path_repr(&path)
                    );
                    *found = true;
                }
            } else if let Some(&child) = file.schema().by_type.get(&field.type_index) {
                references(file, block, child, (at, bytes), prefix, address, found)?;
            }
        }
        prefix.pop();
    }
    Ok(())
}
fn blocks(input: &Path, args: &[OsString]) -> Result<()> {
    // The ten-row default is the observed public `blocks` presentation;
    // explicit --limit controls display only, never the record scan.
    let (mut dump, mut limit, mut requested) = (false, 10i64, None);
    let mut i = 0;
    while i < args.len() {
        match arg(&args[i])? {
            "--dump" => dump = true,
            "--limit" => {
                i += 1;
                let n = arg(args
                    .get(i)
                    .ok_or_else(|| usage("--limit requires an integer"))?)?
                .parse::<i64>()
                .map_err(|_| usage("invalid block limit"))?;
                limit = n;
            }
            value if value.starts_with("--address=") => {
                let value = &value[10..];
                requested = Some(
                    if let Some(hex) = value.strip_prefix("0x") {
                        u64::from_str_radix(hex, 16)
                    } else {
                        value.parse()
                    }
                    .map_err(|_| usage("invalid block address"))?,
                );
            }
            _ => {
                return Err(usage(
                    "blocks INPUT [--dump] [--limit N] [--address=ADDRESS]",
                ));
            }
        }
        i += 1;
    }
    println!("Opening {}", input.display());
    let file = BlendFile::open_blocks(input)?;
    println!("Inspecting {}", input.display());
    let mut groups: Vec<(String, usize, Vec<usize>)> = Vec::new();
    let mut indices = BTreeMap::new();
    for (index, b) in file.blocks().iter().enumerate() {
        if &b.code == b"DNA1" {
            continue;
        }
        let name = block_name(&file, index)?;
        let entry = *indices.entry(name.clone()).or_insert_with(|| {
            groups.push((name, 0, Vec::new()));
            groups.len() - 1
        });
        groups[entry].1 = crate::checked::add(groups[entry].1, b.size)?;
        groups[entry].2.push(index);
    }
    groups.sort_by(|a, b| b.1.cmp(&a.1));
    println!("Block type                          Total Size Num blocks   Avg Size     Median");
    println!("----------------------------------- ---------- ---------- ---------- ----------");
    // A negative display limit uses the observed compatibility slice semantics:
    // -1 omits the final group; it is not an unlimited scan or processing bound.
    let displayed = if limit < 0 {
        groups
            .len()
            .saturating_sub(usize::try_from(limit.unsigned_abs()).unwrap_or(usize::MAX))
    } else {
        usize::try_from(limit).unwrap_or(usize::MAX)
    };
    for (name, total, members) in groups.iter().take(displayed) {
        let mut sizes = members
            .iter()
            .map(|&i| file.blocks()[i].size)
            .collect::<Vec<_>>();
        sizes.sort_unstable();
        let middle = sizes.len() / 2;
        // Black-box even-length vectors [54,55], [55,56], [1,2], [2,3]
        // qualify the compatibility upper median (not an averaged median).
        let median = sizes[middle];
        println!(
            "{name:<35} {:>10} {:>10} {:>10} {:>10}",
            human(*total),
            members.len(),
            human(total / members.len()),
            human(median)
        );
    }
    println!("----------------------------------------------------------------------");
    let selected =
        if let Some(address) = requested {
            Some(file.dereference(address)?.ok_or_else(|| {
                Error::new("missing_address", "selected address is null or absent")
            })?)
        } else {
            groups.first().and_then(|g| {
                g.2.iter().copied().max_by(|&a, &b| {
                    file.blocks()[a]
                        .size
                        .cmp(&file.blocks()[b].size)
                        .then_with(|| b.cmp(&a))
                })
            })
        };
    if let Some(index) = selected {
        let b = file.block(index)?;
        println!(
            "Biggest {} block is {} at address {:#x}",
            block_name(&file, index)?,
            human(b.size),
            b.old_address
        );
        println!("Finding what points there");
        let mut found = false;
        file.scan_records(|other, bytes| {
            references(
                &file,
                other,
                file.blocks()[other].sdna_index,
                (0, bytes),
                &mut Vec::new(),
                b.old_address,
                &mut found,
            )
        })?;
        if !found {
            println!("Nothing points there");
        }
        if dump {
            println!("Hexdump:");
            // Compatibility hexdump line width is an observed presentation
            // format, not a processing limit; every payload byte is emitted.
            let payload = file.payload(index)?;
            for (line, chunk) in payload.chunks(32).enumerate() {
                println!(
                    "{:>6} - {}",
                    line * 32,
                    chunk
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
            }
        }
    }
    file.check_source()?;
    Ok(())
}

pub(crate) fn hash_file(path: &Path) -> Result<(String, f64)> {
    use std::io::Read;
    let start = std::time::Instant::now();
    use std::os::unix::fs::MetadataExt;
    let file = std::fs::File::open(path).map_err(crate::edit_output::io)?;

    let capacity = usize::try_from(file.metadata().map_err(crate::edit_output::io)?.blksize())
        .map_err(|_| Error::new("overflow", "filesystem I/O block size"))?;
    let mut source = std::io::BufReader::with_capacity(capacity, file);
    let mut h = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut block = [0u8; 64];
    let compress = sha_compressor();
    let mut length = 0u64;
    loop {
        let mut n = 0;
        while n < 64 {
            let got = source
                .read(&mut block[n..])
                .map_err(crate::edit_output::io)?;
            if got == 0 {
                break;
            }
            n += got;
        }
        length = length
            .checked_add(n as u64)
            .ok_or_else(|| Error::new("sha256_width", "message length overflow"))?;
        if n == 64 {
            compress(&mut h, &block);
            continue;
        }
        block[n] = 0x80;
        block[n + 1..].fill(0);
        if n >= 56 {
            compress(&mut h, &block);
            block.fill(0);
        }
        block[56..].copy_from_slice(
            &length
                .checked_mul(8)
                .ok_or_else(|| Error::new("sha256_width", "bit length overflow"))?
                .to_be_bytes(),
        );
        compress(&mut h, &block);
        break;
    }
    Ok((
        h.iter().map(|v| format!("{v:08x}")).collect::<String>(),
        start.elapsed().as_secs_f64(),
    ))
}
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];
fn sha_block(h: &mut [u32; 8], b: &[u8; 64]) {
    let mut w = [0u32; 64];
    for (i, c) in b.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes(c.try_into().unwrap());
    }
    for i in 16..64 {
        let x = w[i - 15];
        let y = w[i - 2];
        w[i] = w[i - 16]
            .wrapping_add(x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3))
            .wrapping_add(w[i - 7])
            .wrapping_add(y.rotate_right(17) ^ y.rotate_right(19) ^ (y >> 10));
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
    for i in 0..64 {
        let t = hh
            .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
            .wrapping_add((e & f) ^ (!e & g))
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let t2 = (a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22))
            .wrapping_add((a & b) ^ (a & c) ^ (b & c));
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(t);
        d = c;
        c = b;
        b = a;
        a = t.wrapping_add(t2);
    }
    for (x, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
        *x = x.wrapping_add(v);
    }
}
fn sha_compressor() -> fn(&mut [u32; 8], &[u8; 64]) {
    #[cfg(target_arch = "x86_64")]
    if std::arch::is_x86_feature_detected!("sha")
        && std::arch::is_x86_feature_detected!("ssse3")
        && std::arch::is_x86_feature_detected!("sse4.1")
    {
        return |h, b| unsafe { sha_block_x86(h, b) };
    }
    sha_block
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sha,ssse3,sse4.1")]
unsafe fn sha_block_x86(h: &mut [u32; 8], b: &[u8; 64]) {
    use std::arch::x86_64::*;

    unsafe {
        let tmp = _mm_shuffle_epi32::<0xb1>(_mm_loadu_si128(h.as_ptr().cast()));
        let mut s1 = _mm_shuffle_epi32::<0x1b>(_mm_loadu_si128(h.as_ptr().add(4).cast()));
        let mut s0 = _mm_alignr_epi8::<8>(tmp, s1);
        s1 = _mm_blend_epi16::<0xf0>(s1, tmp);
        let saved0 = s0;
        let saved1 = s1;
        let mask = _mm_set_epi64x(0x0c0d0e0f08090a0b, 0x0405060700010203);
        let mut words = [_mm_setzero_si128(); 4];
        for (i, word) in words.iter_mut().enumerate() {
            *word = _mm_shuffle_epi8(_mm_loadu_si128(b.as_ptr().add(i * 16).cast()), mask);
        }
        for group in 0..16 {
            let j = group % 4;
            if group >= 4 {
                let first = _mm_sha256msg1_epu32(words[j], words[(j + 1) % 4]);
                let cross = _mm_alignr_epi8::<4>(words[(j + 3) % 4], words[(j + 2) % 4]);
                words[j] = _mm_sha256msg2_epu32(_mm_add_epi32(first, cross), words[(j + 3) % 4]);
            }
            let msg = _mm_add_epi32(words[j], _mm_loadu_si128(K.as_ptr().add(group * 4).cast()));
            s1 = _mm_sha256rnds2_epu32(s1, s0, msg);
            s0 = _mm_sha256rnds2_epu32(s0, s1, _mm_shuffle_epi32::<0x0e>(msg));
        }
        s0 = _mm_add_epi32(s0, saved0);
        s1 = _mm_add_epi32(s1, saved1);
        let tmp = _mm_shuffle_epi32::<0x1b>(s0);
        s1 = _mm_shuffle_epi32::<0xb1>(s1);
        _mm_storeu_si128(h.as_mut_ptr().cast(), _mm_blend_epi16::<0xf0>(tmp, s1));
        _mm_storeu_si128(h.as_mut_ptr().add(4).cast(), _mm_alignr_epi8::<8>(s1, tmp));
    }
}
#[cfg(test)]
mod hash_tests {
    #[test]
    fn native_path_json_encoding_is_reversible_and_tags_do_not_collide() {
        use std::os::unix::ffi::OsStringExt;
        let raw = std::path::PathBuf::from(std::ffi::OsString::from_vec(b"a\"\xff".to_vec()));
        assert_eq!(super::json_path(&raw), "bytes-hex:6122ff");
        assert_eq!(
            super::json_path(std::path::Path::new("bytes-hex:6122ff")),
            "utf8:bytes-hex:6122ff"
        );
        assert_eq!(
            super::json_path(std::path::Path::new("utf8:hello")),
            "utf8:utf8:hello"
        );
        let json = serde_json::to_string(&super::json_path(&raw)).unwrap();
        assert_eq!(
            serde_json::from_str::<String>(&json).unwrap(),
            "bytes-hex:6122ff"
        );
    }
    #[test]
    fn human_and_field_path_public_presentation_vectors() {
        use crate::value::PathStep;
        assert_eq!(super::human(19), "19 B");
        assert_eq!(super::human(1024), "1.0 kB");
        assert_eq!(
            super::path_repr(&[PathStep::Field(b"data".to_vec())]),
            "b'data'"
        );
        assert_eq!(
            super::path_repr(&[
                PathStep::Field(b"regionbase".to_vec()),
                PathStep::Field(b"first".to_vec())
            ]),
            "(b'regionbase', b'first')"
        );
        assert_eq!(super::byte_repr(b"a\xff"), "b'a\\xff'");
    }
    #[test]
    fn accelerated_rounds_match_scalar_for_every_byte_seed() {
        for seed in 0..=u8::MAX {
            let block = std::array::from_fn(|i| seed.wrapping_add(i as u8));
            let mut scalar = std::array::from_fn(|i| (seed as u32).wrapping_mul(i as u32 + 1));
            let mut actual = scalar;
            super::sha_block(&mut scalar, &block);
            super::sha_compressor()(&mut actual, &block);
            assert_eq!(actual, scalar);
        }
    }
}
