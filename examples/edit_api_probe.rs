//! actual synchronous editing API entry, independent of CLI dispatch.
use oxblend::edit::{EditOptions, PathRewrite, rewrite_paths};
use oxblend::value::PathStep;
use oxblend::{Error, Result};
use std::path::Path;
fn hex(text: &str) -> Result<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.bytes().all(|v| v.is_ascii_hexdigit()) {
        return Err(Error::new("probe_usage", "hex bytes required"));
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| {
            let digit = |v: u8| {
                if v.is_ascii_digit() {
                    v - b'0'
                } else {
                    v.to_ascii_lowercase() - b'a' + 10
                }
            };
            Ok((digit(p[0]) << 4) | digit(p[1]))
        })
        .collect()
}
fn run() -> Result<()> {
    let args = std::env::args_os().collect::<Vec<_>>();
    if args.len() != 7 && args.len() != 8 {
        return Err(Error::new(
            "probe_usage",
            "INPUT OUTPUT BLOCK FIELD_HEX OLD_HEX NEW_HEX [none|end|each-file]",
        ));
    }
    let text = |i: usize| {
        args[i]
            .to_str()
            .ok_or_else(|| Error::new("probe_usage", "nonpath argument must be ASCII"))
    };
    let request = PathRewrite {
        block_index: text(3)?
            .parse()
            .map_err(|_| Error::new("probe_usage", "block index"))?,
        element: 0,
        steps: vec![PathStep::Field(hex(text(4)?)?)],
        expected_path: hex(text(5)?)?,
        replacement_path: hex(text(6)?)?,
    };
    let options = EditOptions::new(
        args.get(7)
            .map(|_| text(7)?.parse())
            .transpose()?
            .unwrap_or_default(),
    );
    let report = rewrite_paths(
        Path::new(&args[1]),
        Path::new(&args[2]),
        &[request],
        &options,
    )?;
    println!("{}", report.json());
    if !report.complete {
        return Err(Error::new(
            "incomplete_edit",
            "API returned an incomplete report",
        ));
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!(
            "{}",
            serde_json::json!({"error":{"kind":e.kind,"message":e.message}})
        );
        std::process::exit(2);
    }
}
