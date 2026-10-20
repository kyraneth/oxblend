//! Thin serialized reader/editor CLI. Complete output or typed partial report.
use oxblend::value::{PathStep, Value, hex};
use oxblend::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};
use std::ffi::{OsStr, OsString};
use std::io::Write;

fn output_error(error: std::io::Error) -> Error {
    Error::new(
        if error.kind() == std::io::ErrorKind::BrokenPipe {
            "broken_pipe"
        } else {
            "io"
        },
        error.to_string(),
    )
}
macro_rules! println {
    ($($arg:tt)*) => {{
        let mut out = std::io::stdout().lock();
        writeln!(out, $($arg)*).map_err(output_error)?;
        out.flush().map_err(output_error)?;
    }};
}
const SKILL: &str = include_str!("../skills/oxblend/SKILL.md");
const COMMANDS: &[&str] = &[
    "summary",
    "inspect",
    "get",
    "pointer",
    "payload",
    "source-facts",
    "library-facts",
    "trace",
    "list",
    "blocks",
    "overrides",
    "diff",
    "edit",
    "rewrite",
    "pack",
    "pack-sequence",
    "exr-header",
    "pointer-census",
    "skill",
];
const USAGE: &str = "oxblend COMMAND [OPTIONS]\nFrame range + FPS for every saved Scene: oxblend summary FILE\nBundled offline workflows: oxblend skill\nUse oxblend --help to list commands, oxblend COMMAND --help for arguments; oxblend skill for workflows and contracts.\nLocal diagnostics: set FLATTRACE_DIR; see oxblend skill.";

fn argument(v: &OsStr) -> Result<&str> {
    v.to_str().ok_or_else(|| {
        Error::new(
            "usage",
            "argument must be UTF-8 (file path may be arbitrary native bytes)",
        )
    })
}
fn integer(v: &OsStr) -> Result<usize> {
    argument(v)?
        .parse()
        .map_err(|_| Error::new("usage", "nonnegative integer required"))
}
fn path(v: &OsStr) -> Result<Vec<PathStep>> {
    let value: Json =
        serde_json::from_str(argument(v)?).map_err(|e| Error::new("path", e.to_string()))?;
    let items = value
        .as_array()
        .ok_or_else(|| Error::new("path", "JSON array required"))?;
    items
        .iter()
        .map(|v| match v {
            Json::String(s) => Ok(PathStep::Field(s.as_bytes().to_vec())),
            Json::Number(n) => n
                .as_u64()
                .and_then(|i| usize::try_from(i).ok())
                .map(PathStep::Index)
                .ok_or_else(|| Error::new("path", "index must be nonnegative and fit usize")),
            _ => Err(Error::new(
                "path",
                "only field strings and nonnegative integer indexes",
            )),
        })
        .collect()
}
fn unhex(value: &Json) -> Result<Vec<u8>> {
    let text = value
        .as_str()
        .ok_or_else(|| Error::new("edit_spec", "hex string required"))?;
    if !text.len().is_multiple_of(2) || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::new("edit_spec", "even-length ASCII hex required"));
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let one = |v: u8| {
                if v.is_ascii_digit() {
                    v - b'0'
                } else {
                    v.to_ascii_lowercase() - b'a' + 10
                }
            };
            Ok((one(pair[0]) << 4) | one(pair[1]))
        })
        .collect()
}
fn spec_index(value: &Json) -> Result<usize> {
    value
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| Error::new("edit_spec", "index must be a nonnegative usize"))
}
fn spec_keys(value: &Json, keys: &[&str]) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| Error::new("edit_spec", "object required"))?;
    if object.keys().any(|key| !keys.contains(&key.as_str())) {
        return Err(Error::new("edit_spec", "unknown specification field"));
    }
    Ok(())
}
fn rewrite_spec(value: &Json) -> Result<Vec<oxblend::edit::PathRewrite>> {
    spec_keys(value, &["schema", "operation", "edits"])?;
    if value["schema"] != 1 || value.get("operation").is_some_and(|v| v != "rewrite_paths") {
        return Err(Error::new(
            "edit_spec",
            "schema1 rewrite_paths specification required",
        ));
    }
    value["edits"]
        .as_array()
        .ok_or_else(|| Error::new("edit_spec", "edits array required"))?
        .iter()
        .map(|edit| {
            spec_keys(
                edit,
                &[
                    "block_index",
                    "element",
                    "path",
                    "expected_path_hex",
                    "replacement_path_hex",
                ],
            )?;
            let steps = edit["path"]
                .as_array()
                .ok_or_else(|| Error::new("edit_spec", "path component array required"))?
                .iter()
                .map(|step| {
                    spec_keys(step, &["field_hex", "index"])?;
                    if step.as_object().is_none_or(|v| v.len() != 1) {
                        return Err(Error::new("edit_spec", "one field_hex or index per step"));
                    }
                    if let Some(v) = step.get("field_hex") {
                        Ok(PathStep::Field(unhex(v)?))
                    } else {
                        Ok(PathStep::Index(spec_index(&step["index"])?))
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(oxblend::edit::PathRewrite {
                block_index: spec_index(&edit["block_index"])?,
                element: spec_index(&edit["element"])?,
                steps,
                expected_path: unhex(&edit["expected_path_hex"])?,
                replacement_path: unhex(&edit["replacement_path_hex"])?,
            })
        })
        .collect()
}
fn signature(path: &std::path::Path) -> Result<Json> {
    let metadata = path.metadata().map_err(output_error)?;
    let modified = metadata
        .modified()
        .map_err(|e| Error::new("io", e.to_string()))?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Error::new("io", e.to_string()))?;
    Ok(
        json!({"path_hex":hex(path.as_os_str().as_encoded_bytes()), "path":path.to_str(), "size_decimal":metadata.len().to_string(),
        "mtime_ns_decimal":modified.as_nanos().to_string()}),
    )
}
fn durability(value: &OsString) -> Result<oxblend::edit::Durability> {
    argument(value)?
        .parse()
        .map_err(|e: Error| Error::new("usage", e.message))
}
fn run(args: &[OsString]) -> Result<()> {
    let command = args.get(1).ok_or_else(|| Error::new("usage", USAGE))?;
    let command = argument(command)?;
    if command == "--help" || command == "-h" {
        println!("{USAGE}\nCommands: {}", COMMANDS.join(" "));
        return Ok(());
    }
    if args.len() == 3 && matches!(args[2].to_str(), Some("--help" | "-h")) {
        let arguments = match command {
            "skill" => "[print]",
            "summary" | "inspect" | "source-facts" | "library-facts" | "exr-header" => "FILE",
            "get" => "FILE BLOCK PATH_JSON [ELEMENT] [--raw]",
            "pointer" => "FILE BLOCK PATH_JSON [ELEMENT]",
            "payload" => "FILE BLOCK",
            "trace" => "FILE [--per-file|--recursive] [--strict|--non-strict] [--low-memory]",
            "list" => "FILE [--json] [--sha256] [--timing] [--full-info] [--low-memory]",
            "blocks" => "FILE [--limit N] [--address ADDRESS] [--dump]",
            "overrides" => "FILE [--root-only|--recursive] [--strict|--non-strict] [--json]",
            "pointer-census" => "FILE [--full]",
            "diff" => "A.blend B.blend",
            "edit" => {
                "INPUT --output NEW --spec JSON --mode format-only|comp [--durability none|end|each-file]"
            }
            "rewrite" => "INPUT --output NEW --edits SPEC [--durability none|end|each-file]",
            "pack" | "pack-sequence" => {
                let inputs = if command == "pack" {
                    "INPUT NEW_TARGET"
                } else {
                    "INPUT... -t NEW_TARGET"
                };
                println!(
                    "oxblend {command} {inputs} [-p PROJECT] [--exclude GLOB...] [--noop|--dry-run|-n] [--relative-only] [--compress] [--keep-hierarchy] [--durability none|end|each-file]\nINPUT: read-only root blend(s). NEW_TARGET: fresh directory or .zip; never merged/overwritten.\n-t, --target NEW_TARGET: required for pack-sequence; pack requires positional target.\n-p, --project PROJECT: existing directory containing roots; defaults to input parent (common parent for sequence).\n-e, --exclude GLOB...: repeatable resolved-path exclusions, component-wise/right-anchored.\n--noop, --dry-run, -n: plan only; write nothing.\n-r, --relative-only: leave absolute references uncopied/unmodified.\n-c, --compress: seekable zstd blend output; incompatible with ZIP. Short -nrc switches can be combined.\n--keep-hierarchy: preserve hierarchy; default for pack-sequence.\n--durability none|end|each-file: no sync (default), filesystem syncfs, or per-file/parent fsync.\n-- ends option parsing. Missing members can publish available files but exit 2; inspect JSON and pack-info.txt.\n-h, --help: show this help.\nRun oxblend skill for bundled workflows and contracts."
                );
                return Ok(());
            }
            _ => return Err(Error::new("usage", USAGE)),
        };
        let details = match command {
            "skill" => {
                "Print the version-matched, bundled Markdown guide. No checkout, network or installation of guidance files required."
            }
            "summary" => {
                "FILE: read-only blend. Print header, block counts, every saved Scene (name, camera, frame_start/end/step, fps numerator/base/effective) and object/material/mesh/image/collection names. Missing fields become typed warnings/null; inspect warnings before judging an empty scene."
            }
            "inspect" => {
                "FILE: read-only blend. Print every block in physical order and the complete source SDNA. Indices apply only to this exact file/save; byte names are hex."
            }
            "get" => {
                "FILE: read-only blend. BLOCK: nonnegative physical block index from inspect. PATH_JSON: JSON array of field strings/flattened array indices. ELEMENT: nonnegative struct element, default 0. --raw: retain full char-array bytes instead of trimming at NUL. Print a typed value; byte values are hex. Pointer traversal is explicit via pointer."
            }
            "pointer" => {
                "FILE: read-only blend. BLOCK: nonnegative physical block index from inspect. PATH_JSON: JSON array of field strings/flattened array indices. ELEMENT: nonnegative struct element, default 0. Print checked target block index or null; query that target explicitly."
            }
            "payload" => {
                "FILE: read-only blend. BLOCK: nonnegative physical block index from inspect. Print its entire decoded payload as hex (potentially large)."
            }
            "source-facts" | "library-facts" => {
                "FILE: read-only blend. Print narrow per-file collection/object closure (source-facts also follows current Scene and libraries). Unsupported linked layouts can refuse without a report; use trace for library questions and cross-file dependencies."
            }
            "trace" => {
                "FILE: read-only root blend. --per-file: only this file; --recursive: follow libraries (default). --strict: refuse unsupported pointer interpretation (default); --non-strict: record supported partial traversal. --low-memory: use checked spill/seek reads, trading IO/temp disk for memory, not a memory ceiling. Print usages/errors/warnings; complete is traversal, not file availability. Inspect exists and missing_path_count too."
            }
            "list" => {
                "FILE: read-only root blend. List existing expanded dependencies grouped by owner, text by default. --json: sorted JSON member sets. --full-info: include trace/usages. --sha256: hash members in text mode. --timing: timing diagnostics in text mode. SHA/timing with JSON warns and still emits JSON. --low-memory: trade IO/temp disk for lower memory. Missing dependencies refuse with exit 2 and no listing; use trace for details."
            }
            "blocks" => {
                "FILE: read-only blend. Print block statistics/largest blocks/pointer referrers and fields. --limit N: default 10, supports negative slicing. --address ADDRESS: inspect referrers to decimal or 0x hexadecimal address. --dump: print payload bytes in 32-byte lines."
            }
            "overrides" => {
                "FILE: read-only root blend. Print serialized override records/reference chains, not evaluated state. --root-only: only this file; --recursive: follow libraries (default). --strict: strict pointers (default); --non-strict: partial interpretation. --json: accepted; output is always JSON. Incomplete reports exit 2."
            }
            "pointer-census" => {
                "FILE: read-only blend. Print whole-file pointer sites/states, diagnostic only (semantic_verified:false). --full: include every pointer-site detail rather than the summary."
            }
            "diff" => {
                "A.blend B.blend: read-only inputs. Validate containers/layouts, then compare bytes or diagnostic allocation/content identities. Read equal/complete first: equal:null means inconclusive, even with difference_count:0. Returned comparison errors emit an incomplete v1 report and exit 2. Differences have unassessed impact, not render equivalence."
            }
            "edit" => {
                "INPUT: read-only blend, little-endian header >=502. --output NEW: fresh no-replace output. --spec JSON: schema-2 file with expected-old fixed-width writes/bits. --mode format-only|comp: required; masked bits require format-only. --durability none|end|each-file: no sync (default), filesystem syncfs, or per-file/parent fsync. Not an unrestricted setter; inspect complete, errors, publication and durability in JSON."
            }
            "rewrite" => {
                "INPUT: read-only blend. --output NEW: fresh no-replace output. --edits SPEC: schema-1 JSON file of explicit whitelisted filepath edits, with expected/replacement path hex and field_hex/index components. Replacement plus NUL must fit; no ID renames or pointer writes. --durability none|end|each-file: no sync (default), filesystem syncfs, or per-file/parent fsync. Inspect complete, errors, publication and durability; verify output bytes/field independently."
            }
            "exr-header" => {
                "FILE: read-only OpenEXR. Print header/parts/channels and decoded attribute values or typed refusal/raw hex. No pixel decoding: header_complete does not imply pixel_integrity_verified (always false)."
            }
            _ => unreachable!(),
        };
        println!(
            "oxblend {command} {arguments}\n{details}\n-h, --help: show this help.\nRun oxblend skill for bundled workflows and contracts."
        );
        return Ok(());
    }
    if command == "--version" || command == "-V" {
        println!("oxblend {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if command == "skill" {
        if args.len() != 2 && !(args.len() == 3 && args[2] == "print") {
            return Err(Error::new("usage", "skill [print]"));
        }
        let mut out = std::io::stdout().lock();
        out.write_all(SKILL.as_bytes())
            .and_then(|_| out.flush())
            .map_err(output_error)?;
        return Ok(());
    }
    if command == "diff" {
        if args.len() != 4 {
            return Err(Error::new("usage", "diff A.blend B.blend"));
        }
        let report = match oxblend::diff::compare(
            std::path::Path::new(&args[2]),
            std::path::Path::new(&args[3]),
        ) {
            Ok(report) => report,
            Err(error) => {
                // Preserve the Rust API's returned-error contract, but give CLI
                // consumers an honest verdict even before comparison can finish.
                let report = json!({"schema":"oxblend.diff/v1", "read_only":true,
                    "left":args[2].to_str(), "right":args[3].to_str(),
                    "left_path_hex":hex(args[2].as_encoded_bytes()),
                    "right_path_hex":hex(args[3].as_encoded_bytes()),
                    "equal":null, "complete":false, "difference_count":0, "differences":[],
                    "errors":[{"kind":error.kind,"message":error.message}]});
                println!("{report}");
                return Err(error);
            }
        };
        println!("{report}");
        if report["complete"] != true {
            return Err(Error::new(
                "incomplete_diff",
                "all unanchored/persistent unresolved allocations are listed; no equality claim",
            ));
        }
        return Ok(());
    }
    if command == "exr-header" {
        if args.len() != 3 {
            return Err(Error::new("usage", "exr-header INPUT"));
        }
        let header = oxblend::exr_header::read(std::path::Path::new(&args[2]))?;
        println!("{}", header.json());
        return Ok(());
    }
    if command == "pointer-census" {
        let input = args
            .get(2)
            .ok_or_else(|| Error::new("usage", "pointer-census INPUT [--full]"))?;
        if args.len() > 4 || args.get(3).is_some_and(|v| v != "--full") {
            return Err(Error::new("usage", "pointer-census INPUT [--full]"));
        }
        let file = BlendFile::open(std::path::Path::new(input))?;
        let report = oxblend::file_ops::pointer_census(&file)?;
        println!("{}", report.json(args.len() == 4));
        return Ok(());
    }
    if matches!(command, "list" | "blocks") {
        return oxblend::bat_cli::run(command, &args[2..]);
    }
    if command == "pack" || command == "pack-sequence" {
        let (inputs, target, options) = oxblend::bat_cli::pack_arguments(command, &args[2..])?;
        let report = oxblend::pack::pack(&inputs, &target, &options)?;
        println!("{}", report.json());
        if !report.complete {
            return Err(Error::new(
                "incomplete_pack",
                "inspect strict pack report; partial files may remain, never merge/resume",
            ));
        }
        return Ok(());
    }
    if command == "edit" {
        let input = args.get(2).ok_or_else(|| {
            Error::new(
                "usage",
                "edit INPUT --output NEW --spec JSON --mode format-only|comp",
            )
        })?;
        let (mut output, mut spec, mut mode, mut policy) = (None, None, None, None);
        let mut options = args[3..].iter();
        while let Some(option) = options.next() {
            let slot = match argument(option)? {
                "--output" if output.is_none() => &mut output,
                "--spec" if spec.is_none() => &mut spec,
                "--mode" if mode.is_none() => &mut mode,
                "--durability" if policy.is_none() => &mut policy,
                _ => return Err(Error::new("usage", "unknown or duplicate edit option")),
            };
            *slot = Some(
                options
                    .next()
                    .ok_or_else(|| Error::new("usage", "edit option requires a value"))?,
            );
        }
        let output = output.ok_or_else(|| Error::new("usage", "--output required"))?;
        let spec = spec.ok_or_else(|| Error::new("usage", "--spec required"))?;
        let mode =
            match argument(mode.ok_or_else(|| Error::new("usage", "explicit --mode required"))?)? {
                "format-only" => oxblend::typed_edit::EditMode::FormatOnly,
                "comp" => oxblend::typed_edit::EditMode::Comp,
                _ => return Err(Error::new("usage", "--mode format-only|comp required")),
            };
        let value: Json = serde_json::from_slice(
            &std::fs::read(spec).map_err(|e| Error::new("io", e.to_string()))?,
        )
        .map_err(|e| Error::new("edit_spec", e.to_string()))?;
        let snapshot = oxblend::typed_edit::snapshot(std::path::Path::new(input))?;
        let plan = oxblend::typed_spec::plan(&snapshot, &value, mode)?;
        let report = oxblend::typed_edit::write_plan(
            &snapshot,
            &plan,
            std::path::Path::new(output),
            &oxblend::edit::EditOptions::new(
                policy.map(durability).transpose()?.unwrap_or_default(),
            ),
        )?;
        let encoded = serde_json::to_vec(&report.json())
            .map_err(|e| Error::new("serialization", e.to_string()))?;
        let mut out = std::io::stdout().lock();
        out.write_all(&encoded)
            .and_then(|_| out.write_all(b"\n"))
            .and_then(|_| out.flush())
            .map_err(output_error)?;
        if !report.complete {
            return Err(Error::new(
                "incomplete_edit",
                "inspect typed report publication/durability errors",
            ));
        }
        return Ok(());
    }
    if command == "rewrite" {
        let input = args
            .get(2)
            .ok_or_else(|| Error::new("usage", "rewrite INPUT --output NEW --edits SPEC"))?;
        let (mut output, mut specification, mut selected_durability) = (None, None, None);
        let mut options = args[3..].iter();
        while let Some(option) = options.next() {
            let slot = match argument(option)? {
                "--output" if output.is_none() => &mut output,
                "--edits" if specification.is_none() => &mut specification,
                "--durability" if selected_durability.is_none() => &mut selected_durability,
                _ => return Err(Error::new("usage", "unknown or duplicate rewrite option")),
            };
            *slot = Some(
                options
                    .next()
                    .ok_or_else(|| Error::new("usage", "rewrite option needs a value"))?,
            );
        }
        let output = output.ok_or_else(|| Error::new("usage", "--output required"))?;
        let specification = specification.ok_or_else(|| Error::new("usage", "--edits required"))?;
        let data = std::fs::read(specification).map_err(output_error)?;
        let value: Json =
            serde_json::from_slice(&data).map_err(|e| Error::new("edit_spec", e.to_string()))?;
        let edits = rewrite_spec(&value)?;
        let report = oxblend::edit::rewrite_paths(
            std::path::Path::new(input),
            std::path::Path::new(output),
            &edits,
            &oxblend::edit::EditOptions::new(
                selected_durability
                    .map(durability)
                    .transpose()?
                    .unwrap_or_default(),
            ),
        )?;
        let encoded = serde_json::to_vec(&report.json())
            .map_err(|e| Error::new("serialization", e.to_string()))?;
        let mut out = std::io::stdout().lock();
        out.write_all(&encoded)
            .and_then(|_| out.write_all(b"\n"))
            .and_then(|_| out.flush())
            .map_err(output_error)?;
        if !report.complete {
            return Err(Error::new(
                "incomplete_edit",
                "inspect report.errors; no complete output published",
            ));
        }
        return Ok(());
    }
    if command == "overrides" {
        use oxblend::overrides::{OverrideOptions, OverrideScope, trace_overrides};
        use oxblend::trace::PointerMode;
        let input = args.get(2).ok_or_else(|| {
            Error::new(
                "usage",
                "overrides FILE [--root-only|--recursive] [--strict|--non-strict] [--json]",
            )
        })?;
        let mut options = OverrideOptions::default();
        let (mut scope, mut pointers, mut json_option) = (false, false, false);
        for option in &args[3..] {
            match argument(option)? {
                "--root-only" | "--recursive" if !scope => {
                    options.scope = if option == "--root-only" {
                        OverrideScope::RootFile
                    } else {
                        OverrideScope::Recursive
                    };
                    scope = true;
                }
                "--strict" | "--non-strict" if !pointers => {
                    options.pointers = if option == "--strict" {
                        PointerMode::Strict
                    } else {
                        PointerMode::NonStrict
                    };
                    pointers = true;
                }
                "--json" if !json_option => json_option = true,
                _ => return Err(Error::new("usage", "unknown or duplicate overrides option")),
            }
        }
        let start = std::time::Instant::now();
        let report = trace_overrides(std::path::Path::new(input), options)?;
        oxblend::diagnostics::point(
            "query.stats",
            json!({"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"complete":report.complete}),
        );
        let serialization = oxblend::diagnostics::Span::new("serialize", json!({}));
        let encoded = serde_json::to_vec(&report.json())
            .map_err(|e| Error::new("serialization", e.to_string()))?;
        let mut out = std::io::stdout().lock();
        out.write_all(&encoded)
            .and_then(|_| out.write_all(b"\n"))
            .and_then(|_| out.flush())
            .map_err(output_error)?;
        serialization.end("ok");
        if !report.complete {
            return Err(Error::new(
                "incomplete_overrides",
                "inspect report.errors; partial output is not success",
            ));
        }
        return Ok(());
    }
    if command == "trace" {
        use oxblend::trace::{PointerMode, TraceMode, TraceOptions, trace};
        let input = args.get(2).ok_or_else(|| {
            Error::new(
                "usage",
                "trace FILE [--per-file|--recursive] [--strict|--non-strict] [--low-memory]",
            )
        })?;
        let mut mode = TraceMode::Recursive;
        let mut pointers = PointerMode::Strict;
        let mut seen_mode = false;
        let mut seen_pointers = false;
        let mut low_memory = false;
        for option in &args[3..] {
            match argument(option)? {
                "--per-file" | "--recursive" if !seen_mode => {
                    mode = if option == "--per-file" {
                        TraceMode::PerFile
                    } else {
                        TraceMode::Recursive
                    };
                    seen_mode = true;
                }
                "--strict" | "--non-strict" if !seen_pointers => {
                    pointers = if option == "--strict" {
                        PointerMode::Strict
                    } else {
                        PointerMode::NonStrict
                    };
                    seen_pointers = true;
                }
                "--low-memory" if !low_memory => low_memory = true,
                _ => return Err(Error::new("usage", "unknown or duplicate trace option")),
            }
        }
        let start = std::time::Instant::now();
        let mut options = TraceOptions::new(mode, pointers);
        options.low_memory = low_memory;
        let report = trace(std::path::Path::new(input), options)?;
        oxblend::diagnostics::point(
            "query.stats",
            json!({"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"complete":report.complete}),
        );
        let serialization = oxblend::diagnostics::Span::new("serialize", json!({}));
        let encoded = serde_json::to_vec(&report.json())
            .map_err(|e| Error::new("serialization", e.to_string()))?;
        let mut out = std::io::stdout().lock();
        out.write_all(&encoded)
            .and_then(|_| out.write_all(b"\n"))
            .and_then(|_| out.flush())
            .map_err(output_error)?;
        serialization.end("ok");
        if !report.complete {
            return Err(Error::new(
                "incomplete_trace",
                "inspect report.errors; partial output is not success",
            ));
        }
        return Ok(());
    }
    let (element, raw, field_path, block) = match command {
        "summary" | "inspect" | "library-facts" | "source-facts" if args.len() == 3 => {
            (0, false, Vec::new(), 0)
        }
        "payload" if args.len() == 4 => (0, false, Vec::new(), integer(&args[3])?),
        "get" | "pointer" if args.len() >= 5 && args.len() <= 7 => {
            let mut element = 0;
            let mut raw = false;
            let mut seen_element = false;
            for arg in &args[5..] {
                if arg == "--raw" && command == "get" && !raw {
                    raw = true;
                } else if !seen_element {
                    element = integer(arg)?;
                    seen_element = true;
                } else {
                    return Err(Error::new("usage", "duplicate/unknown option"));
                }
            }
            (element, raw, path(&args[4])?, integer(&args[3])?)
        }
        _ => return Err(Error::new("usage", "unknown command or argument count")),
    };
    let start = std::time::Instant::now();
    let facts_query = matches!(command, "library-facts" | "source-facts");
    let owned_path = if facts_query {
        Some(std::fs::canonicalize(&args[2]).map_err(|e| Error::new("io", e.to_string()))?)
    } else {
        None
    };
    let input_signature = owned_path.as_ref().map(|p| signature(p)).transpose()?;
    let file = BlendFile::open(
        owned_path
            .as_ref()
            .map(|p| p.as_os_str())
            .unwrap_or(&args[2]),
    )?;
    let opened = start.elapsed().as_secs_f64();
    let output = match command {
        "summary" => oxblend::summary::summarize(&file)?,
        "library-facts" | "source-facts" => {
            let facts = if command == "source-facts" {
                oxblend::facts::source_facts(&file)
            } else {
                oxblend::facts::library_facts(&file)
            }
            .map_err(|mut error| {
                error.message = format!(
                    "{command} in file '{}': {}; use trace for cross-file dependencies",
                    std::path::Path::new(&args[2]).display(),
                    error.message
                );
                error
            })?;
            json!({"schema":1, "query":command, "semantic":if command == "source-facts" {"oxblend-source-1"} else {"oxblend-library-1"},
                "reader":{"name":"oxblend", "version":env!("CARGO_PKG_VERSION"), "python_reference":"BAT-1.23"},
                "input":input_signature, "stats":{"decompressed_bytes":file.decompressed_len(),
                    "indexed_blocks":file.blocks().len(), "fact_blocks":facts["blocks"].as_object().map_or(0, |b| b.len())}, "facts":facts})
        }
        "get" => json!({"schema":1, "value":file.get(block, element, &field_path, !raw)?.json()}),
        "payload" => {
            json!({"schema":1, "value":Value::Bytes(file.payload(block)?.to_vec()).json()})
        }
        "pointer" => {
            json!({"schema":1, "block_index":file.get_pointer(block, element, &field_path)?})
        }
        "inspect" => {
            let header = file.header();
            json!({"schema":1, "header":{"version":header.version, "pointer_size":header.pointer_size,
                "little_endian":header.little_endian, "file_format":header.file_format},
                "blocks":file.blocks().iter().map(|b| json!({"code_hex":hex(&b.code), "address_decimal":b.old_address.to_string(),
                    "sdna_index":b.sdna_index, "count":b.count, "size":b.size, "data_offset":b.data_offset})).collect::<Vec<_>>(),
                "sdna":{"names_hex":file.schema().names.iter().map(|v| hex(v)).collect::<Vec<_>>(),
                    "types_hex":file.schema().types.iter().map(|v| hex(v)).collect::<Vec<_>>(), "lengths":file.schema().lengths,
                    "structs":file.schema().structs.iter().map(|s| json!({"type_index":s.type_index,
                        "fields":s.fields.iter().map(|f| json!({"name_hex":hex(&f.name), "full_name_hex":hex(&f.full_name),
                            "type_index":f.type_index, "offset":f.offset, "size":f.size, "array_len":f.array_len,
                            "element_size":f.element_size, "pointer":f.pointer})).collect::<Vec<_>>() })).collect::<Vec<_>>()}})
        }
        _ => return Err(Error::new("usage", "command")),
    };
    if let Some(path) = owned_path.as_ref()
        && Some(signature(path)?) != input_signature
    {
        return Err(Error::new(
            "input_changed",
            "source changed during per-file fact extraction",
        ));
    }
    oxblend::diagnostics::point(
        "query.stats",
        json!({"open_index_ms":opened*1000.0,"query_ms":(start.elapsed().as_secs_f64()-opened)*1000.0}),
    );
    let serialization = oxblend::diagnostics::Span::new("serialize", json!({}));
    let encoded =
        serde_json::to_vec(&output).map_err(|e| Error::new("serialization", e.to_string()))?;
    let mut out = std::io::stdout().lock();
    out.write_all(&encoded)
        .and_then(|_| out.write_all(b"\n"))
        .and_then(|_| out.flush())
        .map_err(output_error)?;
    serialization.end("ok");
    Ok(())
}
// Structured argv are data, never an interpolated shell command. Native-byte
// paths remain exact in argv_hex even when their text representation is null.
fn next_command(error: &Error, args: &[OsString]) -> Option<Json> {
    let command = args.get(1).and_then(|v| v.to_str()).unwrap_or("");
    let mut next: Vec<OsString> = vec!["oxblend".into()];
    match error.kind {
        "usage" | "path" => {
            if COMMANDS.contains(&command) {
                next.push(command.into());
            }
            next.push("--help".into());
        }
        "missing_field" | "unsupported_type" | "type" | "bounds" | "pointer_target_type" => {
            let file = args.get(2)?;
            next.push(
                if matches!(command, "source-facts" | "library-facts") {
                    "trace"
                } else {
                    "inspect"
                }
                .into(),
            );
            next.push(file.clone());
        }
        "pointer_path" => next.extend(["pointer".into(), "--help".into()]),
        _ => return None,
    }
    Some(
        json!({"argv":next.iter().map(|v| v.to_str()).collect::<Vec<_>>(),
        "argv_hex":next.iter().map(|v| hex(v.as_encoded_bytes())).collect::<Vec<_>>()}),
    )
}
fn main() {
    let args = std::env::args_os().collect::<Vec<_>>();
    let requested = args.get(1).and_then(|v| v.to_str()).unwrap_or("invalid");
    let command = if matches!(requested, "--help" | "-h")
        || args.get(2).is_some_and(|a| a == "--help" || a == "-h")
    {
        "help"
    } else if matches!(requested, "--version" | "-V") {
        "version"
    } else if COMMANDS.contains(&requested) {
        requested
    } else {
        "invalid"
    };
    oxblend::diagnostics::init(&args, command);
    let span = oxblend::diagnostics::Span::new(command, json!({}));
    let result = span.result(run(&args));
    oxblend::diagnostics::finish(
        result
            .as_ref()
            .err()
            .map_or("ok", oxblend::diagnostics::outcome),
    );
    if let Err(error) = result {
        let _ = writeln!(
            std::io::stderr().lock(),
            "{}",
            json!({"schema":1,"error":{"kind":error.kind,"message":error.message,"next_command":next_command(&error, &args)}})
        );
        std::process::exit(if error.kind == "pack_project" {
            5
        } else if error.kind == "pack_input" || error.kind == "missing_input" {
            3
        } else {
            2
        });
    }
}
