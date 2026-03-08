//! Stress test for the git2 `submodules()` race condition.
//!
//! Repeatedly adds and removes a submodule while calling `repo.submodules()`
//! from another thread. With the old `assert_eq!(rc, 0)` in the
//! `git_submodule_foreach` callback, this aborts the process when
//! `git_submodule_lookup` returns GIT_ENOTFOUND (-3) due to HEAD and
//! the index being temporarily out of sync.
//!
//! With the fix (propagating the error instead of asserting),
//! `submodules()` returns `Err` and the process stays alive.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use git2::Repository;
use tempfile::TempDir;

fn git(args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@test.com"])
        .args(args)
        .output()
        .expect("Failed to run git");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn create_source_repo(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    let path_str = path.display().to_string();
    git(&["-C", &path_str, "init"]);
    std::fs::write(path.join("README.md"), "# Source\n").unwrap();
    git(&["-C", &path_str, "add", "README.md"]);
    git(&["-C", &path_str, "commit", "-m", "Initial commit"]);
}

fn main() {
    let iterations: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);

    let td = TempDir::new().unwrap();
    let root = td.path().join("repo");
    let source = td.path().join("source");

    create_source_repo(&source);
    std::fs::create_dir_all(&root).unwrap();
    let root_str = root.display().to_string();
    let source_str = source.display().to_string();
    git(&["-C", &root_str, "init"]);
    git(&["-C", &root_str, "commit", "--allow-empty", "-m", "init"]);
    git(&[
        "-C",
        &root_str,
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        &source_str,
        "child",
    ]);
    git(&["-C", &root_str, "commit", "-m", "add submodule"]);

    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = Arc::clone(&stop);
    let root2 = root.clone();

    let poller = std::thread::spawn(move || {
        let mut errors = 0u64;
        let mut successes = 0u64;
        while !stop2.load(Ordering::Relaxed) {
            let repo = match Repository::open(&root2) {
                Ok(r) => r,
                Err(_) => {
                    eprintln!("Failed to open repository");
                    continue;
                }
            };
            match repo.submodules() {
                Ok(_) => successes += 1,
                Err(e) => {
                    eprintln!("git2 error: {e}");
                    errors += 1;
                }
            };
        }
        (successes, errors)
    });

    // Main thread: repeatedly remove and re-add the submodule
    for i in 0..iterations {
        git(&["-C", &root_str, "rm", "-f", "child"]);
        git(&["-C", &root_str, "commit", "-m", "remove"]);

        let modules_dir = root.join(".git").join("modules").join("child");
        if modules_dir.exists() {
            std::fs::remove_dir_all(&modules_dir).unwrap();
        }

        git(&[
            "-C",
            &root_str,
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "--force",
            &source_str,
            "child",
        ]);
        git(&["-C", &root_str, "commit", "-m", "re-add"]);

        if (i + 1) % 10 == 0 {
            println!("  iteration {}/{iterations}", i + 1);
        }
    }

    stop.store(true, Ordering::Relaxed);
    let (successes, errors) = poller.join().unwrap();
    println!("\nsubmodules() calls: {successes} ok, {errors} err");
}
