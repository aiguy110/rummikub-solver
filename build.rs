use std::process::Command;

fn main() {
    // Capture the first 8 characters of the current git commit hash
    let output = Command::new("git")
        .args(&["rev-parse", "--short=8", "HEAD"])
        .output();

    let commit_hash = match output {
        Ok(output) if output.status.success() => {
            String::from_utf8(output.stdout)
                .unwrap_or_else(|_| "unknown".to_string())
                .trim()
                .to_string()
        }
        _ => "unknown".to_string(),
    };

    // Set the BUILD_COMMIT environment variable for use in the Rust code
    println!("cargo:rustc-env=BUILD_COMMIT={}", commit_hash);

    // Re-run if .git/HEAD changes (when switching branches)
    println!("cargo:rerun-if-changed=.git/HEAD");
}
