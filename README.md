# git2-submodule-race

Reproducer for a race condition in `Repository::submodules()` where
`git_submodule_lookup` can return `GIT_ENOTFOUND` (-3) when HEAD and
the index are temporarily out of sync during concurrent git operations.

The current git2 code has `assert_eq!(rc, 0)` in the
`git_submodule_foreach` callback, which aborts the process on any
non-zero return. The proposed fix propagates the error through the
callback return value instead, allowing `submodules()` to return `Err`.

## How it works

One thread repeatedly calls `repo.submodules()` while the main thread
cycles through `git rm -f` / `git commit` / `git submodule add` /
`git commit` in a loop. The brief window between the index rename and
the branch ref update during `git commit` is enough to trigger the
inconsistency.

## Usage

```
# With the fix (should print errors but not abort)
cargo run

# More iterations
cargo run -- 200
```

On my machine, I typically observe ~15-20 errors for every ~32k-33k successes.

To observe the abort with git2's current implementation, switch to the
commented out dependency in `Cargo.toml`.
