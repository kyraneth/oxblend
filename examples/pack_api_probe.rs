//! direct synchronous local pack API, no CLI dispatcher.
fn main() {
    let args = std::env::args_os().collect::<Vec<_>>();
    assert!(
        args.len() == 4 || args.len() == 5,
        "INPUT NEW_DIRECTORY PROJECT_ROOT [none|end|each-file]"
    );
    let mut options = oxblend::pack::PackOptions::default();
    options.project_root = std::path::PathBuf::from(&args[3]);
    options.durability = args
        .get(4)
        .map(|v| {
            v.to_str()
                .expect("durability ASCII")
                .parse()
                .expect("durability level")
        })
        .unwrap_or_default();
    match oxblend::pack::pack(
        &[std::path::PathBuf::from(&args[1])],
        std::path::Path::new(&args[2]),
        &options,
    ) {
        Ok(report) => {
            println!("{}", report.json());
            if !report.complete {
                std::process::exit(2);
            }
        }
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::json!({"error":{"kind":error.kind,"message":error.message}})
            );
            std::process::exit(2);
        }
    }
}
