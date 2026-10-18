// receiving-surface qualification harness only, no product implementation.
use oxblend::overrides::{OverrideOptions, OverrideScope, trace_overrides};
use oxblend::trace::PointerMode;
use std::path::Path;
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    let mut options = OverrideOptions::default();
    assert_eq!(args[1], "--workspace");
    let workspace = std::path::PathBuf::from(&args[2])
        .canonicalize()
        .expect("existing workspace");
    assert!(workspace.is_dir());
    let input = std::path::PathBuf::from(&args[3]);
    let input = if input.is_absolute() {
        input
    } else {
        workspace.join(input)
    };
    let admitted = if input.exists() {
        input.canonicalize().expect("input path")
    } else {
        input
            .parent()
            .unwrap()
            .canonicalize()
            .expect("input parent")
            .join(input.file_name().unwrap())
    };
    assert!(
        admitted.starts_with(&workspace),
        "API harness input outside admitted workspace"
    );
    // Python owner applies require_owned/local_metadata before passing workspace.
    for arg in &args[4..] {
        match arg.to_str().unwrap() {
            "--root-only" => options.scope = OverrideScope::RootFile,
            "--recursive" => options.scope = OverrideScope::Recursive,
            "--strict" => options.pointers = PointerMode::Strict,
            "--non-strict" => options.pointers = PointerMode::NonStrict,
            _ => panic!("unknown harness option"),
        }
    }
    match trace_overrides(Path::new(&admitted), options) {
        Ok(report) => {
            println!("{}", report.json());
            if !report.complete {
                std::process::exit(2);
            }
        }
        Err(error) => {
            eprintln!("{}", error);
            std::process::exit(2);
        }
    }
}
