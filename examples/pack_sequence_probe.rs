//! direct list-based pack receiving API, including empty-list refusal.
fn main() {
    let args = std::env::args_os().collect::<Vec<_>>();
    let project = std::path::PathBuf::from(&args[1]);
    let output = std::path::Path::new(&args[2]);
    let inputs = args[3..]
        .iter()
        .map(std::path::PathBuf::from)
        .collect::<Vec<_>>();
    let mut options = oxblend::pack::PackOptions::default();
    options.project_root = project;
    options.keep_hierarchy = true;
    match oxblend::pack::pack(&inputs, output, &options) {
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
