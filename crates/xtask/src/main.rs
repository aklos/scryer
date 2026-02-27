use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();
    let task = args.get(1).map(|s| s.as_str()).unwrap_or("");

    match task {
        "build-sidecar" => build_sidecar(),
        _ => {
            eprintln!("Usage: cargo run -p xtask -- build-sidecar");
            std::process::exit(1);
        }
    }
}

fn build_sidecar() {
    let triple = get_target_triple();
    let root = workspace_root();
    let out_dir = root.join("src-tauri").join("binaries");

    std::fs::create_dir_all(&out_dir).expect("failed to create binaries dir");

    println!("Building scryer-mcp for {triple}...");

    let status = Command::new("cargo")
        .args(["build", "--release", "-p", "scryer-mcp"])
        .status()
        .expect("failed to run cargo build");

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    let (src_name, dst_name) = if cfg!(windows) {
        (
            "scryer-mcp.exe".to_string(),
            format!("scryer-mcp-{triple}.exe"),
        )
    } else {
        ("scryer-mcp".to_string(), format!("scryer-mcp-{triple}"))
    };

    let src = root.join("target").join("release").join(&src_name);
    let dst = out_dir.join(&dst_name);

    std::fs::copy(&src, &dst).unwrap_or_else(|e| {
        panic!(
            "failed to copy {} -> {}: {e}",
            src.display(),
            dst.display()
        );
    });

    println!("Sidecar copied to {}", dst.display());
}

fn get_target_triple() -> String {
    // Try `rustc --print host-tuple` (stable since 1.84)
    let output = Command::new("rustc")
        .args(["--print", "host-tuple"])
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            let triple = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !triple.is_empty() {
                return triple;
            }
        }
    }

    // Fallback: parse `rustc -vV`
    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .expect("failed to run rustc");

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .expect("could not determine host triple from rustc -vV")
        .to_string()
}

fn workspace_root() -> PathBuf {
    // Walk up from current dir to find Cargo.toml with [workspace]
    let mut dir = env::current_dir().expect("no current dir");
    loop {
        let manifest = dir.join("Cargo.toml");
        if manifest.exists() {
            if let Ok(contents) = std::fs::read_to_string(&manifest) {
                if contents.contains("[workspace]") {
                    return dir;
                }
            }
        }
        if !dir.pop() {
            panic!("could not find workspace root");
        }
    }
}
