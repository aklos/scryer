//! Per-test coverage of a Rust test run: `scryer-mcp coverage run -- <command>`
//! runs the agent's nextest command instrumented, one profile per test, and
//! writes the per-test LCOV report `ingest_test_report` reads.
//!
//! Each test needs its own profile, and `LLVM_PROFILE_FILE` cannot name a
//! file after the test: it expands a pid, never an environment variable. So
//! this binary sets itself as cargo's target runner (`coverage runner`):
//! nextest runs every test in its own process with `NEXTEST_BINARY_ID` and
//! `NEXTEST_TEST_NAME` set, and the runner points the profile at a file named
//! for them before it execs the test. `cargo test` runs a whole binary in one
//! process, so it cannot be split per test; only nextest is supported.
//!
//! The instrumented build goes to its own target directory, so it never
//! evicts the developer's normal build. The LLVM tools must match rustc's
//! LLVM; they are found through `LLVM_PROFDATA` / `LLVM_COV`, then `PATH`,
//! then rustup's `llvm-tools` component, and their absence is said, with how
//! to install them, never worked around.

use scryer_extract::test_evidence::{parse_lcov, write_lcov, ParsedCoverage, TestLines};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

const USAGE: &str = "scryer-mcp coverage run [--out file] -- <test command, e.g. cargo nextest run -p crate>";

/// `coverage run …` and the internal `coverage runner …`.
pub(crate) fn run_coverage(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    match args.split_first() {
        Some((verb, rest)) if verb == "run" => run(rest),
        Some((verb, rest)) if verb == "runner" => runner(rest),
        _ => fail(&format!("usage: {USAGE}")),
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(2);
}

fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let split = args.iter().position(|a| a == "--").unwrap_or_else(|| fail(&format!("usage: {USAGE}")));
    let (opts, command) = (&args[..split], &args[split + 1..]);
    if command.is_empty() {
        fail(&format!("usage: {USAGE}"));
    }
    let mut out: Option<PathBuf> = None;
    let mut it = opts.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => out = it.next().map(PathBuf::from),
            other => fail(&format!("unknown argument '{other}'\nusage: {USAGE}")),
        }
    }
    let cwd = std::env::current_dir()?;
    let project = crate::cli::find_project(&cwd).unwrap_or(cwd);
    let tools = LlvmTools::find(&|var| std::env::var_os(var), &host_sysroot_bin()).unwrap_or_else(|e| fail(&e));
    let host = rustc_host().unwrap_or_else(|| fail("scryer coverage: cannot read the host target from `rustc -vV`"));

    let profiles = project.join(".scryer").join("reports").join(format!("coverage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&profiles);
    std::fs::create_dir_all(&profiles)?;
    let exe = std::env::current_exe()?;
    let rustflags = format!("{} -C instrument-coverage", std::env::var("RUSTFLAGS").unwrap_or_default()).trim().to_string();
    let status = Command::new(&command[0])
        .args(&command[1..])
        .current_dir(&project)
        .env("RUSTFLAGS", rustflags)
        .env("CARGO_TARGET_DIR", project.join("target").join("scryer-coverage"))
        .env(runner_var(&host), format!("{} coverage runner", exe.display()))
        .env("SCRYER_COV_DIR", &profiles)
        // Instrumented build scripts and proc macros write profiles too:
        // into the directory that is cleared, never the working directory.
        .env("LLVM_PROFILE_FILE", profiles.join("build-%p-%m.profraw"))
        .status()?;

    let parsed = collect(&tools, &profiles, &project);
    let _ = std::fs::remove_dir_all(&profiles);
    if parsed.tests.is_empty() {
        fail("scryer coverage: no test process wrote a profile. Run the tests through `cargo nextest run`, which runs each test in its own process; `cargo test` cannot be split per test.");
    }
    let out = out.unwrap_or_else(|| project.join(".scryer").join("reports").join("coverage.lcov"));
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&out, write_lcov(&parsed))?;
    let shown = out.strip_prefix(&project).unwrap_or(&out).display().to_string();
    println!("Wrote per-test coverage of {} test(s) to {shown}. Record it: ingest_test_report {{path: \"{shown}\"}}", parsed.tests.len());
    std::process::exit(status.code().unwrap_or(1));
}

/// Cargo's per-target runner variable: `CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER`.
fn runner_var(host: &str) -> String {
    format!("CARGO_TARGET_{}_RUNNER", host.to_uppercase().replace(['-', '.'], "_"))
}

/// What nextest runs as `<runner> <test binary> <args…>`: point this test's
/// profile at a file named for it, note which binary and test it is, exec.
fn runner(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (bin, rest) = args.split_first().unwrap_or_else(|| fail("scryer coverage runner: no test binary"));
    let mut cmd = Command::new(bin);
    cmd.args(rest);
    if let Some(dir) = std::env::var_os("SCRYER_COV_DIR").map(PathBuf::from) {
        match (std::env::var("NEXTEST_BINARY_ID"), std::env::var("NEXTEST_TEST_NAME")) {
            (Ok(binary), Ok(test)) => {
                let key = profile_key(&binary, &test);
                std::fs::write(dir.join(format!("{key}.meta")), format!("{bin}\n{binary}\n{test}\n"))?;
                cmd.env("LLVM_PROFILE_FILE", dir.join(format!("{key}.profraw")));
            }
            // Listing the tests, not running one: its profile is nobody's.
            _ => {
                cmd.env("LLVM_PROFILE_FILE", dir.join("unnamed-%p.profraw"));
            }
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(Box::new(cmd.exec()))
    }
    #[cfg(not(unix))]
    {
        std::process::exit(cmd.status()?.code().unwrap_or(1));
    }
}

/// A file-name-safe key for one test's profile, named for its binary and test.
fn profile_key(binary: &str, test: &str) -> String {
    format!("{binary}${test}")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || "-_.$".contains(c) { c } else { '_' })
        .collect()
}

struct LlvmTools {
    profdata: PathBuf,
    cov: PathBuf,
}

impl LlvmTools {
    /// `LLVM_PROFDATA` / `LLVM_COV`, else `PATH`, else `sysroot_bin` (rustup's
    /// `llvm-tools`), reading variables through `env`. The error says how to
    /// install them.
    fn find(env: &dyn Fn(&str) -> Option<std::ffi::OsString>, sysroot_bin: &Option<PathBuf>) -> Result<Self, String> {
        let pick = |var: &str, name: &str| -> Option<PathBuf> {
            if let Some(p) = env(var).map(PathBuf::from).filter(|p| p.is_file()) {
                return Some(p);
            }
            let in_path = env("PATH")
                .into_iter()
                .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
                .map(|d| d.join(name))
                .find(|p| p.is_file());
            in_path.or_else(|| sysroot_bin.as_ref().map(|d| d.join(name)).filter(|p| p.is_file()))
        };
        match (pick("LLVM_PROFDATA", "llvm-profdata"), pick("LLVM_COV", "llvm-cov")) {
            (Some(profdata), Some(cov)) => Ok(Self { profdata, cov }),
            _ => Err(format!(
                "scryer coverage: llvm-profdata and llvm-cov were not found. They must match rustc's LLVM \
                 ({}): `rustup component add llvm-tools`, or on Nix add `rustc.llvmPackages.llvm` to the \
                 shell, or set LLVM_PROFDATA and LLVM_COV.",
                rustc_llvm().unwrap_or_else(|| "see `rustc -vV`".into())
            )),
        }
    }
}

fn rustc_vv() -> Option<String> {
    let out = Command::new("rustc").arg("-vV").output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn rustc_host() -> Option<String> {
    rustc_vv()?.lines().find_map(|l| l.strip_prefix("host: ").map(|h| h.trim().to_string()))
}

fn rustc_llvm() -> Option<String> {
    rustc_vv()?.lines().find_map(|l| l.strip_prefix("LLVM version: ").map(|v| format!("LLVM {}", v.trim())))
}

/// Where rustup's `llvm-tools` component puts its binaries.
fn host_sysroot_bin() -> Option<PathBuf> {
    let out = Command::new("rustc").args(["--print", "sysroot"]).output().ok()?;
    let sysroot = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    Some(sysroot.join("lib").join("rustlib").join(rustc_host()?).join("bin"))
}

/// Every profile a test wrote, as one report: each test's run lines and all
/// executable lines, the project's own sources only. Profiles convert in
/// parallel; one that fails to convert is skipped and said.
fn collect(tools: &LlvmTools, dir: &Path, project: &Path) -> ParsedCoverage {
    let metas: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "meta"))
        .collect();
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16);
    let chunks: Vec<Vec<PathBuf>> = (0..workers).map(|w| metas.iter().skip(w).step_by(workers).cloned().collect()).collect();
    let results: Vec<(String, String, Result<String, String>)> = std::thread::scope(|s| {
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| s.spawn(move || chunk.iter().filter_map(|m| export(tools, m)).collect::<Vec<_>>()))
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });
    let mut out = ParsedCoverage::default();
    for (binary, test, lcov) in results {
        let lcov = match lcov {
            Ok(l) => l,
            Err(e) => {
                eprintln!("scryer coverage: skipped {binary}${test}: {e}");
                continue;
            }
        };
        let Ok(parsed) = parse_lcov(&lcov) else { continue };
        for (file, lines) in parsed.executable {
            if let Some(rel) = own_source(project, &file) {
                out.executable.entry(rel).or_default().extend(lines);
            }
        }
        let mut files = BTreeMap::new();
        for t in parsed.tests {
            for (file, lines) in t.files {
                if let Some(rel) = own_source(project, &file) {
                    files.entry(rel).or_insert_with(std::collections::BTreeSet::new).extend(lines);
                }
            }
        }
        out.tests.push(TestLines { classname: binary, name: test, files });
    }
    out.tests.sort_by(|a, b| (&a.classname, &a.name).cmp(&(&b.classname, &b.name)));
    out
}

/// One test's profile as LCOV text, its section named for the test.
fn export(tools: &LlvmTools, meta: &Path) -> Option<(String, String, Result<String, String>)> {
    let text = std::fs::read_to_string(meta).ok()?;
    let mut lines = text.lines();
    let (bin, binary, test) = (lines.next()?, lines.next()?.to_string(), lines.next()?.to_string());
    let raw = meta.with_extension("profraw");
    let data = meta.with_extension("profdata");
    let run = |cmd: &mut Command| -> Result<String, String> {
        let out = cmd.output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let lcov = run(Command::new(&tools.profdata).arg("merge").arg("-sparse").arg(&raw).arg("-o").arg(&data)).and_then(|_| {
        run(Command::new(&tools.cov)
            .args(["export", "-format=lcov", "-skip-functions", "-instr-profile"])
            .arg(&data)
            .arg("-object")
            .arg(bin))
    });
    // Name the section for the test: llvm-cov leaves `TN:` empty or out.
    let tn = format!("TN:{binary}${test}\n");
    let named = lcov.map(|l| if l.contains("TN:\n") { l.replace("TN:\n", &tn) } else { format!("{tn}{l}") });
    let _ = std::fs::remove_file(&raw);
    let _ = std::fs::remove_file(&data);
    Some((binary, test, named))
}

/// A path the coverage names, as a project-relative source path: only the
/// project's own files, never the toolchain's, a dependency's, or anything
/// built under `target/`.
fn own_source(project: &Path, file: &str) -> Option<String> {
    let rel = Path::new(file).strip_prefix(project).ok()?;
    let rel = rel.to_string_lossy().replace('\\', "/");
    (!rel.starts_with("target/") && !rel.is_empty()).then_some(rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Names each test process's profile after its binary and test, safe as a file name.
    #[test]
    fn names_a_profile_after_its_test() {
        assert_eq!(profile_key("scryer-mcp::bin/scryer-mcp", "cli::tests::a b"), "scryer-mcp__bin_scryer-mcp$cli__tests__a_b");
        assert_eq!(runner_var("x86_64-unknown-linux-gnu"), "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER");
    }

    /// Keeps only the project's own source files from a test's coverage.
    #[test]
    fn keeps_only_the_projects_own_sources() {
        let p = Path::new("/home/me/repo");
        assert_eq!(own_source(p, "/home/me/repo/crates/a/src/lib.rs").as_deref(), Some("crates/a/src/lib.rs"));
        assert_eq!(own_source(p, "/home/me/repo/target/debug/build/x/out/gen.rs"), None);
        assert_eq!(own_source(p, "/rustc/abc/library/core/src/fmt.rs"), None);
        assert_eq!(own_source(p, "/home/me/.cargo/registry/src/serde/lib.rs"), None);
    }

    /// If the LLVM tools are missing, then says how to install them to match rustc's LLVM.
    #[test]
    fn missing_tools_say_how_to_install_them() {
        let empty = tempfile::tempdir().unwrap();
        // Nothing to find: PATH holds only an empty directory, no variables, an empty sysroot.
        let path = empty.path().as_os_str().to_owned();
        let env = move |var: &str| (var == "PATH").then(|| path.clone());
        let err = LlvmTools::find(&env, &Some(empty.path().to_path_buf())).err().expect("nothing to find");
        assert!(err.contains("rustup component add llvm-tools") && err.contains("LLVM_COV"), "{err}");
    }
}
