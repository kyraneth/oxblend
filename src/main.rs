//! Thin serialized reader/editor CLI. Complete output or typed partial report.
use oxblend::value::{PathStep, hex};
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
fn deliver(write: impl FnOnce(&mut dyn Write) -> std::io::Result<()>) -> Result<()> {
    deliver_with_span(
        write,
        oxblend::diagnostics::Span::new("serialize", json!({"role":"stdout"})),
    )
}
fn deliver_with_span(
    write: impl FnOnce(&mut dyn Write) -> std::io::Result<()>,
    mut span: oxblend::diagnostics::Span,
) -> Result<()> {
    let mut bytes = 0;
    let result = {
        let mut out = oxblend::failure_receipt::stdout_counted(&mut bytes);
        write(&mut out)
            .and_then(|_| out.flush())
            .map_err(output_error)
    };
    oxblend::diagnostics::stdout_delivery(bytes, result.is_ok());
    if span.active() {
        span.data(json!({"bytes":bytes,"delivery_complete":result.is_ok(),"counter_source":"actual OS accepted writes"}));
    }
    span.result(result)
}
fn deliver_json(value: &Json) -> Result<Vec<String>> {
    let causes = oxblend::failure_receipt::record_report(value);
    let span = oxblend::diagnostics::Span::new("serialize", json!({"role":"stdout"}));
    let encoded = match serde_json::to_vec(value) {
        Ok(encoded) => encoded,
        Err(error) => return span.result(Err(Error::new("serialization", error.to_string()))),
    };
    deliver_with_span(
        |out| {
            out.write_all(&encoded)?;
            out.write_all(b"\n")
        },
        span,
    )?;
    Ok(causes)
}
fn deliver_line(arguments: std::fmt::Arguments<'_>) -> Result<()> {
    deliver(|out| writeln!(out, "{arguments}"))
}
macro_rules! println {
    () => {{ deliver_line(format_args!(""))?; }};
    // Evaluate product expressions (including ?) in the caller's Error scope,
    // not inside the stdout writer's io::Result closure.
    ($($arg:tt)*) => {{ deliver_line(format_args!($($arg)*))?; }};
}
mod guidance;
const SKILL: &str = include_str!("../skills/oxblend/SKILL.md");
use oxblend::bat_cli::COMMANDS;
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
    oxblend::read_reports::field_path(argument(v)?)
}
use oxblend::rewrite_spec::parse as rewrite_spec;
fn exr_root_summary(started: std::time::Instant, outputs: Json, complete: bool) {
    oxblend::diagnostics::root_point(
        "root.summary",
        serde_json::json!({
            "outputs":outputs,"diagnostics_complete":complete,
            "resource":{"wall":{"value":started.elapsed().as_secs_f64(),"unit":"seconds","source":"std::time::Instant","scope":"EXR operation, excluding argument and path-list ingestion"},"cpu":null,"peak_rss":null,"unavailable_reason":"process resource counters are retained by receiving GNU time, not sampled here"}
        }),
    );
}
fn signature(path: &std::path::Path) -> Result<Json> {
    oxblend::read_reports::signature(path)
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
    if command == "help" && args.get(2).and_then(|v| v.to_str()) == Some("error") {
        if !(args.len() == 4 || (args.len() == 5 && args[4] == "--json")) {
            return Err(Error::new("usage", "oxblend help error KIND [--json]"));
        }
        let span = oxblend::diagnostics::Span::new(
            "error.help",
            json!({"catalog_version":env!("CARGO_PKG_VERSION")}),
        );
        println!(
            "{}",
            span.result(oxblend::error_help::help(
                argument(&args[3])?,
                args.len() == 5
            ))?
        );
        return Ok(());
    }
    if command == "report" && !matches!(args.get(2).and_then(|v| v.to_str()), Some("--help" | "-h"))
    {
        let span = oxblend::diagnostics::Span::new(
            "report.snapshot",
            json!({"privacy_profile":"structural","network":false}),
        );
        let report = span.result(oxblend::report::run(&args[2..]))?;
        oxblend::failure_receipt::record_report(&report);
        let inventory=report["members"].as_array().map(|members|members.iter().map(|m|json!({"path":m["path"],"bytes":m["bytes"],"sha256":m["sha256"],"media_type":m["media_type"]})).collect::<Vec<_>>());
        // L5 owns root outputs/resource measurements. Keep report-member
        // inventory separate instead of overwriting actual publication facts.
        oxblend::tracing::root_summary(
            json!({"report":{"member_inventory":inventory,"output_written":report.get("output_written"),"source_context_complete":false,"control_snapshot":{"path":report.get("receipt_path"),"path_hex":report.get("receipt_path_hex"),"sha256":report.get("receipt_sha256"),"bytes":report.get("receipt_bytes")},"archive":{"path":report.get("output_path"),"path_hex":report.get("output_path_hex"),"sha256":report.get("archive_sha256")}}}),
        );
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|e| Error::new("serialization", e.to_string()))?
        );
        return Ok(());
    }
    if args.len() == 3 && matches!(args[2].to_str(), Some("--help" | "-h")) {
        let arguments = match command {
            "help" => "error KIND [--json]",
            "report" => {
                println!("{}\n{}", oxblend::report::HELP, guidance::REPORT);
                return Ok(());
            }
            "skill" => "[print]",
            "query" => "FILE --type TYPE --fields FIELD[,FIELD...]",
            "catalog" => "[--json]",
            "explain" => "CONCEPT [--file FILE] [--json]",
            "summary" | "inspect" | "source-facts" | "library-facts" | "exr-header"
            | "node-tree" => "FILE",
            "exr-scan" => "[DIR|FILES...] [--files-from PATH|-] [-0] [--jobs N]",
            "exr-pixels" => "FILE --part N --pass NAME --output NEW.f32",
            "get" => {
                "FILE BLOCK PATH_JSON [ELEMENT] [--raw | --byte-array COUNT | --pointer-table COUNT]"
            }
            "pointer" => "FILE BLOCK PATH_JSON [ELEMENT]",
            "payload" => "FILE BLOCK",
            "read-batch" => "FILE --requests SPEC",
            "trace" => {
                "FILE... [--files-from PATH] [-0] [--summary] [--per-file|--recursive] [--strict|--permissive|--non-strict] [--low-memory]"
            }
            "list" => {
                "FILE... [--files-from PATH] [-0] [--json] [--sha256] [--timing] [--full-info] [--low-memory] [--strict|--permissive|--non-strict]"
            }
            "blocks" => "FILE [--limit N] [--address ADDRESS] [--dump]",
            "overrides" => {
                "FILE [--root-only|--recursive] [--strict|--permissive|--non-strict] [--json]"
            }
            "pointer-census" => "FILE [--full]",
            "diff" => "A.blend B.blend",
            "edit" => {
                "INPUT --output NEW --spec JSON --mode format-only|comp|node-tree [--durability none|end|each-file]"
            }
            "extract" => {
                "SRC (--id NAME|--block INDEX)... [--clear STRUCT.FIELD...] --output NEW [--json] [--durability none|end|each-file]"
            }
            "rewrite" => "INPUT --output NEW --edits SPEC [--durability none|end|each-file]",
            "pack" | "pack-sequence" => {
                let inputs = if command == "pack" {
                    "INPUT NEW_TARGET"
                } else {
                    "INPUT... [--files-from PATH] [-0] -t NEW_TARGET"
                };
                let diagnosis = guidance::DIAGNOSIS;
                let validation = guidance::VALIDATION;
                println!(
                    "oxblend {command} {inputs} [-p PROJECT] [--exclude GLOB...] [--noop|--dry-run|-n] [--relative-only] [--compress] [--keep-hierarchy] [--strict|--permissive|--non-strict] [--durability none|end|each-file] [--memory fast|bounded]\n{diagnosis}\nINPUT: read-only root blend(s). NEW_TARGET: fresh local directory or .zip; never merged/overwritten. All URI scheme:// targets (including shaman:// and s3://) are refused as usage, even with --noop. Upload transports are excluded; use a filesystem target.\nSaved Image tile lists and associated ImageUser spans require every declared tile/frame; missing or UNKNOWN pattern members make the pack incomplete.\n-t, --target NEW_TARGET: required for pack-sequence; pack requires positional target.\n-p, --project PROJECT: existing directory containing roots; defaults to input parent (common parent for sequence).\n-e, --exclude GLOB...: repeatable resolved-path exclusions, component-wise/right-anchored; narrows what is packed, reports not self-contained.\n--noop, --dry-run, -n: plan only; write nothing.\n-r, --relative-only: narrows what is packed to relative declarations; reports not self-contained.\n-c, --compress: seekable zstd blend output; incompatible with ZIP. Short -nrc switches can be combined.\n--keep-hierarchy: preserve hierarchy; default for pack-sequence.\n--durability none|end|each-file: no sync (default), filesystem syncfs, or per-file/parent fsync.\n--memory fast|bounded: fast (default) uses signature-bound selective raw windows for raw blends, reading only needed raw ranges and retaining no whole decoded sources on that path. Compressed blends fall back to full decoded capture. Python Sessions retain captured sources as before. Bounded lowers peak memory to roughly the largest single source plus indexes (and active ranges; not a hard RAM ceiling), retaining digests/indexes and re-reading sources: more IO, slower. Selected workload measurements showed fast heap 27-53% lower; this is not a universal memory or timing guarantee. Same output bytes and refusals. Use for large sequences on shared workstations. Report and CLI failure receipt record \"memory\":\"bounded\" only; absence means fast.\n--strict (default): any unprovable dependency refuses before any publication.\n--permissive (--non-strict alias): publish provable members only, INCOMPLETE with every skipped reason, exit 2. Never farm-ready.\nReports always tell everything; packages refuse unless --permissive.\n{validation}\n-- ends option parsing. Inspect JSON mode, skipped_items and pack-info.txt.\n-h, --help: show this help.\nRun oxblend skill for bundled workflows and contracts."
                );
                if command == "pack-sequence" {
                    println!(
                        "--files-from PATH (or --files-from=PATH): expand literal native paths in place; - reads stdin. -0 uses NUL delimiters for all lists; otherwise LF, rejecting NUL. Empty entries ignored, CR/spaces/# preserved, relative paths use cwd. Inspect cross_reference for all source requirements; lexical pack layout is unchanged."
                    );
                }
                return Ok(());
            }
            _ => return Err(Error::new("usage", USAGE)),
        };
        let details = match command {
            "query" => {
                "One reader, every matching saved record/element in order. Exact raw SDNA fields or Scene render,view_layers,compositor,id_properties. Partial outcomes preserve independent fields and rows, exit 2. No evaluation, implicit linked-file reads or runtime defaults."
            }
            "catalog" => {
                "Authored saved-fact concepts and exact aliases. JSON output; no file required. Use explain CONCEPT --file FILE --json to inspect actual SDNA."
            }
            "explain" => {
                "Explain an exact catalog concept or file SDNA type, actual fields and next steps. No fuzzy aliases or inferred runtime behavior. Output is JSON."
            }
            "help" => {
                "Offline explanation, likely causes and safe next actions for every built-in error kind. Unknown kinds are preserved in a usage error, not aliased."
            }
            "skill" => {
                "Print the version-matched, bundled Markdown guide. No checkout, network or installation of guidance files required."
            }
            "summary" => {
                "FILE: read-only blend. Print header, block counts, every saved Scene (name, camera, frame_start/end/step, fps numerator/base/effective) and object/material/mesh/image/collection names. Missing fields become typed warnings/null; inspect warnings before judging an empty scene."
            }
            "inspect" => {
                "FILE: read-only blend. Print every block in physical order and the complete source SDNA. inspect.blocks is an ordered array: its index is BLOCK. code_hex includes four bytes with NUL padding (Scene: 53430000). address_decimal is a decimal string; parse as an integer before numeric formatting. Struct names come from sdna.structs[sdna_index].type_index into sdna.types_hex. Indices apply only to this exact file/save; byte names are hex. Do not assume index/code/type aliases."
            }
            "get" => {
                "FILE: read-only blend. BLOCK: nonnegative physical block index from inspect. PATH_JSON: JSON array of field strings/flattened array indices. ELEMENT: nonnegative struct element, default 0. --raw: retain full char-array bytes instead of trimming at NUL. --byte-array COUNT: read exactly COUNT bytes through one declared byte pointer. --pointer-table COUNT: read COUNT nullable targets through one declared struct** in its source-role namespace; return value.block_indices. These three modes are mutually exclusive. Counts come from actual saved count fields, never guessed defaults; wrong depth/type, overflow, crossing an allocation or bad targets refuse the operation. Print a typed value; bytes use value.hex. inspect.blocks is ordered: its array index is BLOCK, code_hex includes NUL padding (Scene: 53430000), address_decimal is a decimal string. Read actual fields from SDNA, not guessed aliases. Pointer traversal is explicit via pointer."
            }
            "pointer" => {
                "FILE: read-only blend. BLOCK: nonnegative physical block index from inspect. PATH_JSON: JSON array of field strings/flattened array indices. ELEMENT: nonnegative struct element, default 0. Print checked target block index or null; query that target explicitly."
            }
            "payload" => {
                "FILE: read-only blend. BLOCK: nonnegative physical block index from inspect. Print its entire decoded payload as hex (potentially large)."
            }
            "read-batch" => {
                "FILE: read-only blend. SPEC: JSON file {\"schema\":1,\"requests\":[...]} with ordered inspect/payload/get/pointer items; empty array is valid and still checks input. BLOCK/ELEMENT/path/raw/byte_array/pointer_table use the exact single-command meanings; unknown ops/keys are per-item usage errors. One open, ordered JSONL items plus a terminal summary; independent errors use incomplete_read_batch, source-change guards stop remaining as unobserved. Same report/error bytes as the single commands; failed rows add the invocation-local sibling diagnostic_ref (null or run_id/failure_id), never inside error."
            }
            "source-facts" | "library-facts" => {
                "FILE: read-only blend. Print narrow per-file collection/object closure (source-facts also follows current Scene and libraries). Failed pointer edges remain unknown at their owner/field site; independent facts are retained, complete:false and exit 2. Unknown ancestry has no proved root. Unsupported root layouts can still refuse; use trace for cross-file dependencies."
            }
            "trace" => {
                "Diagnose does not mean repair. Explain invalid/unknown pointer sites; never guess, null or byte-patch a pointer to bypass refusal without an explicitly authorized separate repair plan. Keep originals untouched. A clean trace proves only serialized dependency closure, not Blender open/render validity. FILE: read-only root blend. --per-file: only this file; --recursive: follow libraries (default). Reports always tell everything; packages refuse unless --permissive. --strict/--permissive/--non-strict are accepted compatibility no-ops here; unknown pointer subtrees are reported, never guessed. --low-memory: use checked spill/seek reads, trading IO/temp disk for memory, not a memory ceiling. Print usages/errors/warnings; missing files/IDs or unknown subtrees set complete:false and exit 2. Inspect exists, missing_path_count and problem_counts too."
            }
            "list" => {
                "FILE: read-only root blend. List expanded dependencies and missing physical declarations grouped by owner, text by default. --json: sorted JSON member sets. --full-info: include trace/usages. --sha256: hash members in text mode. --timing: timing diagnostics in text mode. SHA/timing with JSON warns and still emits JSON. --low-memory: trade IO/temp disk for lower memory. Missing physical dependencies are listed with [missing] and exit 2; incomplete JSON has complete:false and files with path/missing member records. Missing IDs add missing_links/counts and [missing linked ID] lines. Invalid pointers/unreadable dependency libraries add unknown sites/errors and problem_counts, even without --full-info. Reports always tell everything; packages refuse unless --permissive. --strict/--permissive/--non-strict are compatibility no-ops for list. True root input/usage/I/O errors still refuse."
            }
            "blocks" => {
                "FILE: read-only blend. Print block statistics/largest blocks/pointer referrers and fields. --limit N: default 10, supports negative slicing. --address ADDRESS: inspect referrers to decimal or 0x hexadecimal address. --dump: print payload bytes in 32-byte lines."
            }
            "overrides" => {
                "FILE: read-only root blend. Print serialized override records/reference chains, not evaluated state. --root-only: only this file; --recursive: follow libraries (default). Reports always tell everything; packages refuse unless --permissive. --strict/--permissive/--non-strict are compatibility no-ops here. Unknown sites retain pointer context; no guessed targets. --json: accepted; output is always JSON. Incomplete reports exit 2."
            }
            "pointer-census" => {
                "FILE: read-only blend. Print whole-file pointer sites/states, diagnostic only (semantic_verified:false). --full: include every pointer-site detail rather than the summary."
            }
            "diff" => {
                "A.blend B.blend: read-only inputs. Validate containers/layouts, then compare bytes or diagnostic allocation/content identities. Read equal/complete first: equal:null means inconclusive, even with difference_count:0. Returned comparison errors emit an incomplete v1 report and exit 2. Differences have unassessed impact, not render equivalence."
            }
            "extract" => {
                "SRC: read-only blend. Select exact prefixed saved ID names with --id or physical indices from this save with --block. Copy owned closure using source header/DNA/addresses. Other Main IDs need explicit selection or --clear Struct.field; CompositorNodeRLayers.scene aliases that node type's bNode.id. Unknown pointers and embedded root promotion refuse. --output NEW is atomic no-replace, never in place. JSON is always emitted; --json is accepted. --durability none|end|each-file. Inspect complete, refusals, applied_policies, runtime_sanitation, structural_fixups, output_written and durability_completed. Native open/append/render is not certified."
            }
            "node-tree" => {
                "FILE: read exact compositor graphs for expected-old structural guards. Native Blender 5.2.1 build 9e2066aef7ef only; no evaluation or native verification."
            }
            "edit" => {
                "INPUT: read-only blend, little-endian header >=502. --output NEW: fresh no-replace output. --spec JSON: schema-2 file with expected-old fixed-width writes/bits and optional idprop_strings (idprop-string: same-allocation UTF-8 values on 502/503, len updated, pointer/totallen unchanged). --mode format-only|comp|node-tree: required; masked bits require format-only. node-tree uses its separate schema-1 expected-graph specification and native-template constructors. --durability none|end|each-file: no sync (default), filesystem syncfs, or per-file/parent fsync. Not an unrestricted setter; inspect complete, errors, publication and durability in JSON."
            }
            "rewrite" => {
                "INPUT: read-only blend. --output NEW: fresh no-replace output. --edits SPEC: schema-1 JSON file of explicit whitelisted filepath edits, with expected/replacement path hex and field_hex/index components. Replacement plus NUL must fit; no ID renames or pointer writes. --durability none|end|each-file: no sync (default), filesystem syncfs, or per-file/parent fsync. Inspect complete, errors, publication and durability; verify output bytes/field independently."
            }
            "exr-pixels" => {
                "Decode an explicit zero-based part and channel pass prefix, never the first pass by default. Empty --pass selects unprefixed channels. New output only: raw f32 little-endian, interleaved channels in header order, increasing x then y from dataWindow minimum. No color conversion, padding or alpha synthesis. Full-sampled half/float base-resolution pixels only; deep/subsampled/selected uint refuse. JSON describes exact layout. Codec failures are typed, exit 2, no output. Header/structure scan alone never verifies pixels."
            }
            "exr-header" => {
                "FILE: read-only OpenEXR. Print decoded header/part/channel facts and chunk-table integrity. header_end (deprecated alias extent) is not file_size. No pixel decoding: pixel_integrity_verified is always false; deep files are unsupported."
            }
            "exr-scan" => {
                "Recursively inventory all regular files, including non-EXRs. Stream one JSONL facts record per file and a final summary by status/output tree. --jobs N: positive worker count, default available cores. --files-from PATH|-: read paths from a file or stdin, newline-delimited (NUL with -0); relative to cwd. Do not follow directory symlinks. Malformed files are classified data; filesystem/changed-input errors or incomplete stdout delivery exit 2."
            }
            _ => unreachable!(),
        };
        let shared = match command {
            "summary" => guidance::METADATA.to_owned(),
            "inspect" | "get" => guidance::INSPECT.to_owned(),
            "trace" | "pointer" | "payload" => guidance::DIAGNOSIS.to_owned(),
            "read-batch" => format!("{}\n{}", guidance::DIAGNOSIS, guidance::INSPECT),
            "help" => guidance::REPORT.to_owned(),
            "skill" => format!("{}\n{}", guidance::RELEASE, guidance::RUN_HELPER),
            _ => String::new(),
        };
        println!(
            "oxblend {command} {arguments}\n{details}\n{shared}\n-h, --help: show this help.\nRun oxblend skill for bundled workflows and contracts."
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
        deliver(|out| out.write_all(SKILL.as_bytes()))?;
        return Ok(());
    }
    if command == "catalog" {
        if args.len() != 2 && !(args.len() == 3 && args[2] == "--json") {
            return Err(Error::new("usage", "catalog [--json]"));
        }
        println!("{}", oxblend::query::catalog());
        return Ok(());
    }
    if command == "explain" {
        let concept = argument(
            args.get(2)
                .ok_or_else(|| Error::new("usage", "explain CONCEPT [--file FILE] [--json]"))?,
        )?;
        let mut source = None;
        let mut json_flag = false;
        let mut options = args[3..].iter();
        while let Some(option) = options.next() {
            match argument(option)? {
                "--file" if source.is_none() => {
                    source = Some(
                        options
                            .next()
                            .ok_or_else(|| Error::new("usage", "--file requires FILE"))?,
                    )
                }
                "--json" if !json_flag => json_flag = true,
                _ => return Err(Error::new("usage", "unknown or duplicate explain option")),
            }
        }
        let file = source
            .map(|p| BlendFile::open(std::path::Path::new(p)))
            .transpose()?;
        println!("{}", oxblend::query::explain(file.as_ref(), concept)?);
        return Ok(());
    }
    if command == "query" {
        let input = args.get(2).ok_or_else(|| {
            Error::new("usage", "query FILE --type TYPE --fields FIELD[,FIELD...]")
        })?;
        let mut type_name = None;
        let mut fields = None;
        let mut options = args[3..].iter();
        while let Some(option) = options.next() {
            let slot = match argument(option)? {
                "--type" if type_name.is_none() => &mut type_name,
                "--fields" if fields.is_none() => &mut fields,
                _ => return Err(Error::new("usage", "unknown or duplicate query option")),
            };
            *slot = Some(argument(options.next().ok_or_else(|| {
                Error::new("usage", "query option requires value")
            })?)?);
        }
        let type_name = type_name.ok_or_else(|| Error::new("usage", "--type required"))?;
        let fields = fields
            .ok_or_else(|| Error::new("usage", "--fields required"))?
            .split(',')
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let file = BlendFile::open(std::path::Path::new(input))?;
        let mut report = oxblend::query::query_with_source(
            &file,
            std::path::Path::new(input),
            type_name,
            &fields,
        )?;
        report["source_observation"]["path"] = json!(input.to_str());
        report["source_observation"]["path_hex"] = json!(hex(input.as_encoded_bytes()));
        let causes = deliver_json(&report)?;
        if report["requested_coverage"]["complete"] != true {
            let error = oxblend::strict::query_refusal();
            oxblend::failure_receipt::record_terminal(&error, &causes);
            return Err(error);
        }
        return Ok(());
    }
    if command == "extract" {
        if args.len() < 3 {
            return Err(Error::new("usage", "extract SRC --id NAME --output NEW"));
        }
        let mut options = oxblend::extract::ExtractOptions::default();
        let mut output = None;
        let mut i = 3;
        while i < args.len() {
            let flag = argument(&args[i])?;
            if flag == "--json" {
                i += 1;
                continue;
            }
            i += 1;
            let value = args
                .get(i)
                .ok_or_else(|| Error::new("usage", "extract option needs value"))?;
            match flag {
                "--id" => options.ids.push(argument(value)?.to_owned()),
                "--block" => options.blocks.push(integer(value)?),
                "--clear" => {
                    options.clear.insert(argument(value)?.to_owned());
                }
                "--output" if output.is_none() => output = Some(std::path::PathBuf::from(value)),
                "--durability" => options.edit.durability = argument(value)?.parse()?,
                _ => return Err(Error::new("usage", "unknown or duplicate extract option")),
            }
            i += 1;
        }
        let output = output.ok_or_else(|| Error::new("usage", "extract requires --output"))?;
        let report = oxblend::extract::extract(std::path::Path::new(&args[2]), &output, &options)?;
        deliver_json(&report)?;
        if report["complete"] != true {
            return Err(oxblend::strict::extract_refusal());
        }
        return Ok(());
    }
    if command == "node-tree" {
        if args.len() != 3 {
            return Err(Error::new("usage", "node-tree FILE"));
        }
        let report = oxblend::node_tree_edit::inspect(std::path::Path::new(&args[2]))?;
        deliver_json(&report)?;
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
                let report = oxblend::diff::failure_report(
                    std::path::Path::new(&args[2]),
                    std::path::Path::new(&args[3]),
                    &error,
                );
                deliver_json(&report)?;
                return Err(error);
            }
        };
        let causes = deliver_json(&report)?;
        if report["complete"] != true {
            let error = oxblend::strict::diff_refusal();
            oxblend::failure_receipt::record_terminal(&error, &causes);
            return Err(error);
        }
        return Ok(());
    }
    if command == "exr-pixels" {
        if args.len() != 9 || args[3] != "--part" || args[5] != "--pass" || args[7] != "--output" {
            return Err(Error::new(
                "usage",
                "exr-pixels FILE --part N --pass NAME --output NEW.f32",
            ));
        }
        let part = args[4]
            .to_str()
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or_else(|| Error::new("usage", "--part requires a zero-based unsigned index"))?;
        let pass = args[6]
            .to_str()
            .ok_or_else(|| Error::new("usage", "--pass must be UTF-8"))?;
        let started = std::time::Instant::now();
        let result = oxblend::exr_pixels::extract(
            std::path::Path::new(&args[2]),
            part,
            pass,
            std::path::Path::new(&args[8]),
        );
        oxblend::diagnostics::point(
            "exr.output",
            serde_json::json!({"output":args[8].to_string_lossy(),"output_written":if result.is_ok(){Some(true)}else{None}}),
        );
        let outputs = result.as_ref().ok().map(|report| serde_json::json!([{"path":args[8].to_string_lossy(),"bytes":report["bytes"],"format":"f32le-interleaved"}])).unwrap_or_else(||serde_json::json!([{"path":args[8].to_string_lossy(),"publication":"unknown-or-refused; consult typed error and publish span"}]));
        exr_root_summary(started, outputs, result.is_ok());
        let report = result?;
        deliver_json(&report)?;
        return Ok(());
    }
    if command == "exr-scan" {
        let mut jobs = std::thread::available_parallelism()
            .map_err(output_error)?
            .get();
        let mut inputs = Vec::new();
        let mut lists = Vec::new();
        let mut nul = false;
        let mut args = args[2..].iter();
        let mut options = true;
        while let Some(arg) = args.next() {
            if options && arg == "--" {
                options = false;
            } else if options && arg == "--jobs" {
                jobs = integer(args.next().ok_or_else(|| {
                    Error::new("usage", "--jobs requires a positive worker count")
                })?)?;
            } else if options && arg.to_str().is_some_and(|a| a.starts_with("--jobs=")) {
                jobs = integer(OsStr::new(
                    arg.to_str().unwrap().strip_prefix("--jobs=").unwrap(),
                ))?;
            } else if options && arg == "-0" {
                nul = true;
            } else if options && arg == "--files-from" {
                lists.push(
                    args.next()
                        .ok_or_else(|| Error::new("usage", "--files-from requires PATH or -"))?
                        .clone(),
                );
            } else if options && oxblend::path_list::inline_path(arg).is_some() {
                lists.push(oxblend::path_list::inline_path(arg).unwrap().to_owned());
            } else if options && arg.to_str().is_some_and(|a| a.starts_with('-')) {
                return Err(Error::new("usage", "unknown exr-scan option"));
            } else {
                inputs.push(std::path::PathBuf::from(arg));
            }
        }
        for source in lists {
            inputs.extend(oxblend::path_list::read(
                std::path::Path::new(&source),
                nul,
            )?);
        }
        let started = std::time::Instant::now();
        let result =
            oxblend::exr_scan::scan(&inputs, jobs, &mut oxblend::failure_receipt::stdout());
        exr_root_summary(
            started,
            serde_json::json!([{"kind":"stdout","format":"jsonl"}]),
            result.is_ok(),
        );
        return result;
    }
    if command == "exr-header" {
        if args.len() != 3 {
            return Err(Error::new("usage", "exr-header INPUT"));
        }
        let header = oxblend::exr_header::read(std::path::Path::new(&args[2]))?;
        deliver_json(&header.json())?;
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
        deliver_json(&report.json(args.len() == 4))?;
        return Ok(());
    }
    if command == "read-batch" {
        if args.len() != 5 || args[3] != "--requests" {
            return Err(Error::new("usage", "read-batch FILE --requests SPEC"));
        }
        let started = std::time::Instant::now();
        let mut out = oxblend::failure_receipt::stdout();
        let result = oxblend::read_batch::run(
            std::path::Path::new(&args[2]),
            args[2].as_os_str(),
            std::path::Path::new(&args[4]),
            &mut out,
        );
        // Drop the writer before the timing point so byte accounting is final.
        drop(out);
        oxblend::diagnostics::point(
            "query.stats",
            json!({"elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"complete":result.is_ok()}),
        );
        result.map(|_| ())?;
        return Ok(());
    }
    // Cardinality is decided after lexical root deduplication. A single root
    // returns to the unchanged legacy renderer, including its original spelling.
    let selected_args;
    let args = if matches!(command, "trace" | "list") {
        let selection = oxblend::multi_root::arguments(&args[2..])?;
        if selection.roots.len() > 1 || selection.summary {
            return oxblend::multi_root::run(command, &selection);
        }
        selected_args = if command == "list" {
            args[..2]
                .iter()
                .cloned()
                .chain(selection.options)
                .chain([OsString::from("--"), selection.first_spelling])
                .collect::<Vec<_>>()
        } else {
            args[..2]
                .iter()
                .cloned()
                .chain(std::iter::once(selection.first_spelling))
                .chain(selection.options)
                .collect::<Vec<_>>()
        };
        selected_args.as_slice()
    } else {
        args
    };
    if matches!(command, "list" | "blocks") {
        return oxblend::bat_cli::run(command, &args[2..]);
    }
    if command == "pack" || command == "pack-sequence" {
        let (inputs, target, options) = oxblend::bat_cli::pack_arguments(command, &args[2..])?;
        let report = oxblend::bat_cli::pack_cli(&inputs, &target, &options)?;
        let causes = deliver_json(&report.json())?;
        if let Some(error) = oxblend::pack::missing_root_error(&report) {
            return Err(error);
        }
        if !report.complete {
            let error = oxblend::pack::strict_pack_refusal(&report);
            oxblend::failure_receipt::record_terminal(&error, &causes);
            return Err(error);
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
        let selected_mode =
            argument(mode.ok_or_else(|| Error::new("usage", "explicit --mode required"))?)?;
        if selected_mode == "node-tree" {
            let value: Json = serde_json::from_slice(
                &std::fs::read(spec).map_err(|e| Error::new("io", e.to_string()))?,
            )
            .map_err(|e| Error::new("edit_spec", e.to_string()))?;
            let report = oxblend::node_tree_edit::edit(
                std::path::Path::new(input),
                std::path::Path::new(output),
                &value,
                &oxblend::edit::EditOptions::new(
                    policy.map(durability).transpose()?.unwrap_or_default(),
                ),
            )?;
            deliver_json(&report)?;
            if report["complete"] != true {
                return Err(oxblend::strict::edit_node_tree_refusal());
            }
            return Ok(());
        }
        let mode = match selected_mode {
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
        let causes = deliver_json(&report.json())?;
        if !report.complete {
            let error = oxblend::strict::edit_typed_refusal();
            oxblend::failure_receipt::record_terminal(&error, &causes);
            return Err(error);
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
        let causes = deliver_json(&report.json())?;
        if !report.complete {
            let error = oxblend::edit::strict_rewrite_refusal();
            oxblend::failure_receipt::record_terminal(&error, &causes);
            return Err(error);
        }
        return Ok(());
    }
    if command == "overrides" {
        use oxblend::overrides::{OverrideOptions, OverrideScope, trace_overrides};
        use oxblend::trace::PointerMode;
        let input = args.get(2).ok_or_else(|| {
            Error::new(
                "usage",
                "overrides FILE [--root-only|--recursive] [--strict|--permissive|--non-strict] [--json]",
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
                "--strict" | "--permissive" | "--non-strict" if !pointers => {
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
        let causes = deliver_json(&report.json())?;
        if !report.complete {
            let error = oxblend::strict::overrides_refusal();
            oxblend::failure_receipt::record_terminal(&error, &causes);
            return Err(error);
        }
        return Ok(());
    }
    if command == "trace" {
        use oxblend::trace::{PointerMode, TraceMode, TraceOptions};
        let input = args.get(2).ok_or_else(|| {
            Error::new(
                "usage",
                "trace FILE [--per-file|--recursive] [--strict|--permissive|--non-strict] [--low-memory]",
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
                "--strict" | "--permissive" | "--non-strict" if !seen_pointers => {
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
        let report = oxblend::bat_cli::trace_cli(std::path::Path::new(input), options)?;
        oxblend::diagnostics::point(
            "query.stats",
            json!({"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"complete":report.complete}),
        );
        let causes = deliver_json(&report.json())?;
        if !report.complete {
            let error = oxblend::strict::trace_refusal();
            oxblend::failure_receipt::record_terminal(&error, &causes);
            return Err(error);
        }
        return Ok(());
    }
    enum ArrayRead {
        Bytes(usize),
        Pointers(usize),
    }
    let mut array_read = None;
    let (element, raw, field_path, block) = match command {
        "summary" | "inspect" | "library-facts" | "source-facts" if args.len() == 3 => {
            (0, false, Vec::new(), 0)
        }
        "payload" if args.len() == 4 => (0, false, Vec::new(), integer(&args[3])?),
        "get" | "pointer" if args.len() >= 5 => {
            let mut element = 0;
            let mut raw = false;
            let mut seen_element = false;
            let mut options = args[5..].iter();
            while let Some(arg) = options.next() {
                match argument(arg)? {
                    "--raw" if command == "get" && !raw && array_read.is_none() => raw = true,
                    mode @ ("--byte-array" | "--pointer-table")
                        if command == "get" && !raw && array_read.is_none() =>
                    {
                        let count = integer(options.next().ok_or_else(|| {
                            Error::new("usage", "array read mode requires a count")
                        })?)?;
                        array_read = Some(if mode == "--byte-array" {
                            ArrayRead::Bytes(count)
                        } else {
                            ArrayRead::Pointers(count)
                        });
                    }
                    name if !seen_element && !name.starts_with("--") => {
                        element = integer(arg)?;
                        seen_element = true;
                    }
                    _ => {
                        return Err(Error::new(
                            "usage",
                            "duplicate, incompatible or unknown get/pointer option",
                        ));
                    }
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
    let resolving = oxblend::diagnostics::Span::new_lazy("query.resolve", || {
        if oxblend::diagnostics::enabled() {
            json!({"command":command,"block":block,"element":element,"raw":raw})
        } else {
            Json::Null
        }
    });
    let output = resolving.result((|| {
        Ok(match command {
            "summary" => oxblend::summary::summarize(&file)?,
            "library-facts" | "source-facts" => oxblend::read_reports::facts(
                &file,
                command,
                input_signature.clone().unwrap_or(Json::Null),
                std::path::Path::new(&args[2]),
            )?,
            "get" => {
                let (bytes, pointers) = match array_read {
                    None => (None, None),
                    Some(ArrayRead::Bytes(count)) => (Some(count), None),
                    Some(ArrayRead::Pointers(count)) => (None, Some(count)),
                };
                oxblend::read_reports::get(
                    &file,
                    block,
                    element,
                    &field_path,
                    raw,
                    bytes,
                    pointers,
                )?
            }
            "payload" => oxblend::read_reports::payload(&file, block)?,
            "pointer" => oxblend::read_reports::pointer(&file, block, element, &field_path)?,
            "inspect" => oxblend::read_reports::inspect(&file),
            _ => return Err(Error::new("usage", "command")),
        })
    })())?;
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
    let causes = deliver_json(&output)?;
    if facts_query && output["complete"] == false {
        let first = output["errors"][0]["message"].as_str();
        let error = oxblend::strict::facts_refusal(
            command,
            &std::path::Path::new(&args[2]).display().to_string(),
            first,
        );
        oxblend::failure_receipt::record_terminal(&error, &causes);
        return Err(error);
    }
    Ok(())
}
// Structured argv are data, never an interpolated shell command. Native-byte
// paths remain exact in argv_hex even when their text representation is null.
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
    oxblend::failure_receipt::begin(command);
    let span = oxblend::diagnostics::Span::new(command, json!({}));
    let result = span.result(run(&args));
    if let Err(error) = result {
        let base = json!({"schema":1,"error":oxblend::error_help::public_error(&error, &args)});
        let exit_code = if error.kind == "pack_project" {
            5
        } else if error.kind == "pack_input" || error.kind == "missing_input" {
            3
        } else {
            2
        };
        let diagnostic = oxblend::failure_receipt::finish(&error, &base, exit_code);
        let mut envelope = base;
        if let Some(diagnostic) = diagnostic {
            envelope["diagnostic"] = diagnostic;
        }
        let delivered = writeln!(std::io::stderr().lock(), "{envelope}").is_ok();
        oxblend::tracing::root_summary(json!({"diagnostic_stderr_delivered":delivered}));
        oxblend::diagnostics::finish(oxblend::diagnostics::outcome(&error));
        std::process::exit(if error.kind == "pack_project" {
            5
        } else if error.kind == "pack_input" || error.kind == "missing_input" {
            3
        } else {
            2
        });
    }
    oxblend::failure_receipt::summarize_stdout();
    oxblend::diagnostics::finish("ok");
}
