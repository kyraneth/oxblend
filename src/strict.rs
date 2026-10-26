//! Single engine owner for every strict/incomplete delivery refusal.
//!
//! The CLI delivery checks and the Python facade (`_admit`) both select the
//! identical canonical [`crate::Error`] here, so no surface synthesizes its
//! own prose, derives a second variant policy, or falls back to a formatted
//! string. Kinds, messages and selection rules below are byte-exact copies of
//! the historical CLI literals; only the location changed. Report-embedded
//! per-site errors (trace failures, extract refusal causes, pack planning
//! errors) stay with their owning report builders and are not strict prose.

use crate::{Error, Result};
use serde_json::Value as Json;

/// Strict trace refusal: the retained report is incomplete, never a guess.
pub fn trace_refusal() -> Error {
    Error::new(
        "incomplete_trace",
        "Dependency report is INCOMPLETE: independent provable facts and missing/unknown sites are retained. Inspect errors, warnings and problem_counts; stop at each unknown subtree, not the whole report. Diagnose does not mean repair: do not guess or byte-patch pointers without a separately authorized repair plan.",
    )
}

/// Strict overrides refusal.
pub fn overrides_refusal() -> Error {
    Error::new(
        "incomplete_overrides",
        "inspect report.errors; partial output is not success",
    )
}

/// Strict query refusal.
pub fn query_refusal() -> Error {
    Error::new(
        "incomplete_query",
        "requested fields unavailable; all independent rows retained, inspect errors and each next_action",
    )
}

/// Strict diff refusal.
pub fn diff_refusal() -> Error {
    Error::new(
        "incomplete_diff",
        "all unanchored/persistent unresolved allocations are listed; no equality claim",
    )
}

/// Strict extract refusal.
pub fn extract_refusal() -> Error {
    Error::new(
        "extract_refusal",
        "inspect extract refusals and publication errors",
    )
}

/// Strict typed-edit refusal (format-only / comp).
pub fn edit_typed_refusal() -> Error {
    Error::new(
        "incomplete_edit",
        "inspect typed report publication/durability errors",
    )
}

/// Strict node-tree edit refusal.
pub fn edit_node_tree_refusal() -> Error {
    Error::new(
        "incomplete_edit",
        "inspect node-tree publication/durability errors",
    )
}

/// Strict rewrite refusal.
pub fn rewrite_refusal() -> Error {
    Error::new(
        "incomplete_edit",
        "inspect report.errors; no complete output published",
    )
}

/// Strict read-batch aggregate refusal (no Python batch API; CLI only).
pub fn read_batch_refusal() -> Error {
    Error::new(
        "incomplete_read_batch",
        "read-batch incomplete: independent item errors retained; inspect each error and its next_command",
    )
}

/// Multi-root dependency refusal.
pub fn multi_root_refusal() -> Error {
    Error::new(
        "incomplete_trace",
        "multi-root dependency report INCOMPLETE; inspect every root's errors and missing files",
    )
}

/// Listing incomplete-trace refusal (missing/unknown sites, no guess).
pub fn listing_incomplete_trace() -> Error {
    Error::new(
        "incomplete_trace",
        "incomplete dependency report; inspect the reported missing/unknown sites",
    )
}

/// Listing missing-asset refusal.
pub fn listing_missing_asset() -> Error {
    Error::new(
        "missing_asset",
        "saved pattern member missing; use trace for per-usage availability",
    )
}

/// Listing pattern-members-unknown refusal; the reason is the saved
/// usage's own unknown-member explanation, never synthesized here.
pub fn listing_pattern_unknown(reason: String) -> Error {
    Error::new("pattern_members_unknown", reason)
}

/// Strict per-file facts refusal. The message embeds the caller's command
/// spelling, the input display path and the report's first error message,
/// exactly as the CLI delivery check formats it. `first_error` is the
/// report's `errors[0].message` or the historical `"unknown fact edge"`.
pub fn facts_refusal(command: &str, input_display: &str, first_error: Option<&str>) -> Error {
    Error::new(
        "incomplete_facts",
        format!(
            "{} in file '{}': {}. Per-file fact report is INCOMPLETE: unknown pointer sites and independent facts are retained; inspect errors and facts.blocks. Unknown ancestry is not a proved root; use trace for cross-file dependencies.",
            command,
            input_display,
            first_error.unwrap_or("unknown fact edge")
        ),
    )
}

/// Facts refusal over the serialized CLI-shaped facts report. Fails loudly
/// as usage when the report is not the engine's facts shape, never selecting
/// a wrong first-error silently.
pub fn facts_refusal_json(command: &str, input_display: &str, report: &Json) -> Result<Error> {
    let first = report
        .get("errors")
        .and_then(Json::as_array)
        .and_then(|errors| errors.first())
        .and_then(|error| error.get("message"))
        .and_then(Json::as_str);
    // The CLI delivery check reads `output["errors"][0]["message"]` with the
    // same fallback; an absent errors array is a caller-shape bug, not a
    // silent unknown edge.
    if report.get("errors").is_none() {
        return Err(Error::new(
            "usage",
            "facts refusal needs the serialized facts report",
        ));
    }
    Ok(facts_refusal(command, input_display, first))
}

fn incomplete_pack_message(noop: bool, published: bool) -> &'static str {
    if noop {
        "Dry-run only; nothing was written. The plan is INCOMPLETE: inspect errors, missing, missing_links, skipped_items and every root_traces entry. Strict production refuses these requirements; explicit --permissive may publish provable members only, INCOMPLETE, exit 2, not farm-ready."
    } else if published {
        "INCOMPLETE package: output was published but the command did not complete. Retain this report and pack-info.txt; inspect missing/unknown sites, publication and durability errors. Do not deliver as complete or farm-ready. Never merge/resume this destination; resolve source requirements and use a fresh target."
    } else {
        "Package not published: inspect errors, skipped_items and every root_traces entry for the missing/unknown requirement or planning failure. Strict mode (default) publishes nothing on unprovable requirements. Use --permissive only for an INCOMPLETE diagnostic partial package, never farm-ready; it does not bypass unsafe planning. Diagnose does not mean repair: do not guess or byte-patch pointers without a separately authorized repair plan."
    }
}

/// Strict pack refusal from typed publication facts.
pub fn pack_refusal(noop: bool, published: bool) -> Error {
    Error::new("incomplete_pack", incomplete_pack_message(noop, published))
}

/// Strict pack refusal over the serialized pack report. The JSON shape is
/// engine-owned (see [`crate::pack::PackReport::json`]); absent flags fail
/// loudly as usage rather than selecting a wrong variant silently.
pub fn pack_refusal_json(report: &Json) -> Result<Error> {
    let object = report
        .as_object()
        .ok_or_else(|| Error::new("usage", "pack refusal needs the serialized pack report"))?;
    let flag = |key: &str| -> Result<bool> {
        match object.get(key) {
            Some(Json::Bool(value)) => Ok(*value),
            _ => Err(Error::new(
                "usage",
                "pack refusal needs boolean report flags",
            )),
        }
    };
    let published = flag("directory_created")?
        || flag("archive_written")?
        || object
            .get("pack_info")
            .and_then(Json::as_object)
            .and_then(|info| info.get("output_written"))
            .and_then(Json::as_bool)
            .unwrap_or(false)
        || object
            .get("files")
            .and_then(Json::as_array)
            .is_some_and(|files| {
                files.iter().any(|file| {
                    file.get("output_written")
                        .and_then(Json::as_bool)
                        .unwrap_or(false)
                })
            });
    Ok(pack_refusal(flag("noop")?, published))
}

/// Whether the serialized report is the engine-owned multi-root shape.
/// The CLI routes every multi-root incomplete (trace and list) through the
/// same multi-root refusal; the Python facade must select it from the
/// report itself, never from a caller guess. Single-root reports never
/// carry this schema.
fn is_multi_root(report: &Json) -> bool {
    report
        .get("schema")
        .and_then(Json::as_str)
        .is_some_and(|schema| schema == "oxblend.multi-root/v1")
}

/// One coherent strict/refusal seam for the CLI delivery checks and the
/// Python facade. `operation` is the `_admit` operation (`trace`, `facts`,
/// `query`, `overrides`, `diff`, `edit`, `extract`, `pack`, `list`);
/// `command` disambiguates `edit` (`edit` typed, `node-tree`, `rewrite`) and
/// carries the facts command spelling (`source-facts`/`library-facts`);
/// `report` is the serialized incomplete report; `input_display` is the
/// caller-spelling display path for facts. Multi-root reports select the
/// shared multi-root refusal from their engine-owned schema, not from the
/// caller operation. Unknown operations fail loudly as usage; no caller
/// may synthesize prose on failure.
pub fn strict_refusal(
    operation: &str,
    command: Option<&str>,
    report: &Json,
    input_display: Option<&str>,
) -> Result<Error> {
    // Multi-root incomplete reports (trace and list) share one CLI
    // canonical refusal, selected here from the engine-owned schema.
    if is_multi_root(report) {
        return Ok(multi_root_refusal());
    }
    match operation {
        "trace" => Ok(trace_refusal()),
        "list" => Ok(listing_incomplete_trace()),
        "query" => Ok(query_refusal()),
        "overrides" => Ok(overrides_refusal()),
        "diff" => Ok(diff_refusal()),
        "extract" => Ok(extract_refusal()),
        "edit" => match command {
            Some("rewrite") => Ok(rewrite_refusal()),
            Some("node-tree") => Ok(edit_node_tree_refusal()),
            _ => Ok(edit_typed_refusal()),
        },
        "pack" => pack_refusal_json(report),
        "facts" => {
            let command = command.ok_or_else(|| {
                Error::new(
                    "usage",
                    "facts refusal needs the source-facts/library-facts command",
                )
            })?;
            let input_display = input_display
                .ok_or_else(|| Error::new("usage", "facts refusal needs the input display path"))?;
            facts_refusal_json(command, input_display, report)
        }
        _ => Err(Error::new(
            "usage",
            format!("unknown strict operation {operation:?}; no synthesized refusal"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_matches_cli_canonical() {
        let error = trace_refusal();
        assert_eq!(error.kind, "incomplete_trace");
        assert_eq!(
            error.message,
            "Dependency report is INCOMPLETE: independent provable facts and missing/unknown sites are retained. Inspect errors, warnings and problem_counts; stop at each unknown subtree, not the whole report. Diagnose does not mean repair: do not guess or byte-patch pointers without a separately authorized repair plan."
        );
    }

    #[test]
    fn all_static_variants_match_cli_canonical() {
        assert_eq!(overrides_refusal().kind, "incomplete_overrides");
        assert_eq!(
            overrides_refusal().message,
            "inspect report.errors; partial output is not success"
        );
        assert_eq!(query_refusal().kind, "incomplete_query");
        assert_eq!(
            query_refusal().message,
            "requested fields unavailable; all independent rows retained, inspect errors and each next_action"
        );
        assert_eq!(diff_refusal().kind, "incomplete_diff");
        assert_eq!(
            diff_refusal().message,
            "all unanchored/persistent unresolved allocations are listed; no equality claim"
        );
        assert_eq!(extract_refusal().kind, "extract_refusal");
        assert_eq!(
            extract_refusal().message,
            "inspect extract refusals and publication errors"
        );
        assert_eq!(edit_typed_refusal().kind, "incomplete_edit");
        assert_eq!(
            edit_typed_refusal().message,
            "inspect typed report publication/durability errors"
        );
        assert_eq!(edit_node_tree_refusal().kind, "incomplete_edit");
        assert_eq!(
            edit_node_tree_refusal().message,
            "inspect node-tree publication/durability errors"
        );
        assert_eq!(rewrite_refusal().kind, "incomplete_edit");
        assert_eq!(
            rewrite_refusal().message,
            "inspect report.errors; no complete output published"
        );
        assert_eq!(multi_root_refusal().kind, "incomplete_trace");
        assert_eq!(
            multi_root_refusal().message,
            "multi-root dependency report INCOMPLETE; inspect every root's errors and missing files"
        );
        assert_eq!(listing_incomplete_trace().kind, "incomplete_trace");
        assert_eq!(
            listing_incomplete_trace().message,
            "incomplete dependency report; inspect the reported missing/unknown sites"
        );
        assert_eq!(listing_missing_asset().kind, "missing_asset");
        assert_eq!(
            listing_missing_asset().message,
            "saved pattern member missing; use trace for per-usage availability"
        );
        assert_eq!(read_batch_refusal().kind, "incomplete_read_batch");
    }

    #[test]
    fn pack_selects_cli_variant() {
        let unpublished = pack_refusal(false, false);
        assert_eq!(unpublished.kind, "incomplete_pack");
        assert!(unpublished.message.starts_with("Package not published:"));
        let dry = pack_refusal(true, false);
        assert!(dry.message.starts_with("Dry-run only;"));
        let published = pack_refusal(false, true);
        assert!(published.message.starts_with("INCOMPLETE package:"));
    }

    #[test]
    fn dispatch_covers_every_admit_operation() {
        let empty = Json::Null;
        assert_eq!(
            strict_refusal("trace", None, &empty, None).unwrap().kind,
            "incomplete_trace"
        );
        assert_eq!(
            strict_refusal("query", None, &empty, None).unwrap().kind,
            "incomplete_query"
        );
        assert_eq!(
            strict_refusal("overrides", None, &empty, None)
                .unwrap()
                .kind,
            "incomplete_overrides"
        );
        assert_eq!(
            strict_refusal("diff", None, &empty, None).unwrap().kind,
            "incomplete_diff"
        );
        assert_eq!(
            strict_refusal("extract", None, &empty, None).unwrap().kind,
            "extract_refusal"
        );
        assert_eq!(
            strict_refusal("edit", None, &empty, None).unwrap().message,
            "inspect typed report publication/durability errors"
        );
        assert_eq!(
            strict_refusal("edit", Some("node-tree"), &empty, None)
                .unwrap()
                .message,
            "inspect node-tree publication/durability errors"
        );
        assert_eq!(
            strict_refusal("edit", Some("rewrite"), &empty, None)
                .unwrap()
                .message,
            "inspect report.errors; no complete output published"
        );
        assert!(strict_refusal("nope", None, &empty, None).is_err());
    }

    // Permanent multi-root pin: the engine-owned schema selects the shared
    // multi-root refusal for every trace/list caller, so single-root-only
    // coverage (and its two skips) can never hide a multi-root prose drift.
    #[test]
    fn multi_root_schema_overrides_single_trace_and_list() {
        let multi = serde_json::json!({
            "schema": "oxblend.multi-root/v1",
            "operation": "trace",
            "complete": false,
        });
        for operation in ["trace", "list"] {
            let error = strict_refusal(operation, None, &multi, None).unwrap();
            assert_eq!(error.kind, "incomplete_trace");
            assert_eq!(
                error.message,
                "multi-root dependency report INCOMPLETE; inspect every root's errors and missing files"
            );
        }
        // Single-root reports keep their own variants, never the multi-root one.
        let single_trace = serde_json::json!({"schema": 1, "complete": false});
        assert_eq!(
            strict_refusal("trace", None, &single_trace, None)
                .unwrap()
                .message,
            "Dependency report is INCOMPLETE: independent provable facts and missing/unknown sites are retained. Inspect errors, warnings and problem_counts; stop at each unknown subtree, not the whole report. Diagnose does not mean repair: do not guess or byte-patch pointers without a separately authorized repair plan."
        );
    }
}
