fn main() {
    let manifest = std::fs::read_to_string("../../Cargo.toml").expect("engine manifest");
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    let package = manifest.split("[package]").nth(1).expect("engine package");
    let version = package
        .lines()
        .take_while(|line| !line.starts_with('['))
        .find_map(|line| line.strip_prefix("version = ").map(|v| v.trim_matches('"')))
        .expect("engine version");
    println!("cargo:rustc-env=OXBLEND_ENGINE_VERSION={version}");
    println!("cargo:rerun-if-env-changed=OXBLEND_BINDING_BUILD_COMMIT");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=../../src");
    for args in [
        vec!["rev-parse", "--git-path", "HEAD"],
        vec!["symbolic-ref", "-q", "HEAD"],
    ] {
        if let Ok(output) = std::process::Command::new("git").args(&args).output()
            && output.status.success()
        {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if args[0] == "symbolic-ref" {
                if let Ok(path) = std::process::Command::new("git")
                    .args(["rev-parse", "--git-path", &value])
                    .output()
                    && path.status.success()
                {
                    println!(
                        "cargo:rerun-if-changed={}",
                        String::from_utf8_lossy(&path.stdout).trim()
                    );
                }
            } else {
                println!("cargo:rerun-if-changed={value}");
            }
        }
    }
    let commit = std::env::var("OXBLEND_BINDING_BUILD_COMMIT")
        .ok()
        .or_else(|| {
            std::process::Command::new("git")
                .args(["describe", "--always", "--dirty", "--abbrev=40"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        })
        .unwrap_or_else(|| "unknown: source archive without injected revision".into());
    println!("cargo:rustc-env=OXBLEND_BINDING_BUILD_COMMIT={commit}");
}
