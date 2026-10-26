//! Offline version-matched error vocabulary. Generated prose, no tracker fetches.
use crate::{Error, Result};
use serde_json::{Value, json};

pub fn next_command(error: &Error, args: &[std::ffi::OsString]) -> Option<Value> {
    suggestion(error.kind, args)
}

/// Shared CLI/binding next-action suggestion. It is data, never executed.
pub fn suggestion(kind: &str, args: &[std::ffi::OsString]) -> Option<Value> {
    let command = args.get(1).and_then(|v| v.to_str()).unwrap_or("");
    let mut next: Vec<std::ffi::OsString> = vec!["oxblend".into()];
    match kind {
        "usage" | "path" => {
            if crate::bat_cli::COMMANDS.contains(&command) {
                next.push(command.into());
            }
            next.push("--help".into());
        }
        "incomplete_facts"
        | "missing_field"
        | "unsupported_type"
        | "type"
        | "bounds"
        | "pointer_target_type" => {
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
        "argv_hex":next.iter().map(|v| crate::value::hex(v.as_encoded_bytes())).collect::<Vec<_>>()}),
    )
}

/// Single owner for the public CLI error object: kind, message,
/// next_command plus catalog prose, with NO diagnostic reference inside.
/// Diagnostic evidence lives only in the outer failure-receipt (`diagnostic`)
/// for single commands and as the sibling `diagnostic_ref` on batch rows;
/// never serializes raw source snapshots and never prints native error
/// context; refs stay invocation-local and are validated separately,
/// never fabricated.
pub fn public_error(error: &Error, args: &[std::ffi::OsString]) -> Value {
    let mut object = json!({"kind":error.kind,"message":error.message,"next_command":suggestion(error.kind, args)});
    if let Some(entry) = entry(error.kind) {
        for field in ["explanation", "likely_causes", "next_steps", "known_issue"] {
            object[field] = entry[field].clone();
        }
    }
    object
}

fn stored_catalog() -> &'static Value {
    static CATALOG: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../docs/error-catalog.json"))
            .expect("compiled catalog is validated by generation gate")
    })
}
pub fn catalog() -> Value {
    stored_catalog().clone()
}
pub fn entry(kind: &str) -> Option<Value> {
    stored_catalog()["entries"]
        .as_array()?
        .iter()
        .find(|e| e["kind"] == kind)
        .cloned()
}
pub fn help(kind: &str, machine: bool) -> Result<String> {
    let Some(entry) = entry(kind) else {
        return Err(Error::new(
            "usage",
            format!(
                "unknown error kind {kind:?}; preserve that exact kind, retain stdout/stderr/exit, inspect nested errors, do not infer a repair or retry writes. This binary's offline catalog may predate the producer."
            ),
        ));
    };
    if machine {
        Ok(serde_json::to_string_pretty(&json!({"schema":"oxblend.error-help/v1","version":env!("CARGO_PKG_VERSION"),"error":entry})).expect("JSON value"))
    } else {
        Ok(format!(
            "{} — {}\nLikely causes:\n{}\nSafe next steps:\n{}\nKnown issue: no verified public mapping.\nA refusal is not proof of corruption or a product bug.",
            kind,
            entry["explanation"].as_str().unwrap(),
            entry["likely_causes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| format!("- {}", v.as_str().unwrap()))
                .collect::<Vec<_>>()
                .join("\n"),
            entry["next_steps"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| format!("- {}", v.as_str().unwrap()))
                .collect::<Vec<_>>()
                .join("\n")
        ))
    }
}
