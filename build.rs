use std::process::Command;
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
fn main() {
    println!("cargo:rerun-if-env-changed=OXBLEND_BUILD_COMMIT");
    for name in [
        "HEAD".to_owned(),
        git(&["symbolic-ref", "-q", "HEAD"]).unwrap_or_default(),
    ] {
        if !name.is_empty()
            && let Some(path) = git(&["rev-parse", "--git-path", &name])
        {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    let commit = std::env::var("OXBLEND_BUILD_COMMIT")
        .ok()
        .or_else(|| git(&["rev-parse", "HEAD"]))
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=OXBLEND_BUILD_COMMIT={commit}");
}
