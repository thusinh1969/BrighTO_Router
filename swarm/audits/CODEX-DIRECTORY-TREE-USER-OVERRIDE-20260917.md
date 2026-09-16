# Directory tree user override - 2026-09-17

User corrected the cleanup policy:

```text
target/ stays locally because it contains build outputs/cache.
swarm/ stays because it is used for build/collaboration.
```

## Updated verdict

### `target/`

Keep it on the machine. Do not delete it during active coding or benchmarking because it contains Rust build cache and release/debug binaries.

Still keep it out of GitHub. It remains ignored by `.gitignore`, which is correct for a Rust repository.

### `swarm/`

Keep `swarm/` in the repo while Codex/DeepSeek collaboration is active. It is the live auditor/coder channel and build coordination area.

Do not remove `swarm/` unless the user explicitly asks again after the build/release process is finished.

### Public cleanliness target after this override

Clean means:

- no obsolete or contradictory files;
- no generated runtime output tracked by Git;
- no broken scripts;
- no stale instructions that point to old binary names or old paths;
- `target/` may exist locally but must stay untracked;
- `swarm/` may stay tracked because the user wants it for build collaboration.

This replaces the earlier recommendation to remove most of `swarm/` before public release.
