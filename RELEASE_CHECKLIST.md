# Release Checklist

Run before every release to verify everything works.

## Build

- [ ] `cargo test` — all tests pass
- [ ] `cargo clippy -- -D warnings` — zero warnings
- [ ] `cargo build --release` — compiles

## Index & Update (on openclaw, 9K files)

- [ ] `frg index .` — completes in ~20s
- [ ] `frg status .` — shows version, file count, n-grams, commit
- [ ] `frg update .` (no changes) — completes in <100ms, "No changes detected"
- [ ] `frg update .` (1 file changed) — completes in <100ms, shows overlay + tombstone count
- [ ] `frg status .` — shows "base + overlay, N tombstoned"

## Match Parity (vs ripgrep, case-sensitive)

All must show EXACT:

- [ ] `renderUsage` — selective
- [ ] `function` — broad (24K hits)
- [ ] `^\s*export\s` — anchored
- [ ] `TODO|FIXME|HACK|XXX|BUG` — multi-alternation
- [ ] `useState|useEffect` — alternation
- [ ] `.*` — wildcard (ScanAll)
- [ ] `x` — single byte (ScanAll)

## Speed (vs ripgrep, frg must win all)

- [ ] `renderUsage` — selective (~5x)
- [ ] `function` — broad (~1.5x)
- [ ] `.*` — wildcard (~2.7x)
- [ ] `x` — single byte (~1.7x)
- [ ] `TODO|FIXME|HACK|XXX|BUG` — multi-alt (~2.5x)

## Flags

- [ ] `-i` — case-insensitive
- [ ] `-S` — smart case
- [ ] `-F` — fixed string
- [ ] `-l` — files only
- [ ] `-c` — count per file
- [ ] `-C 2` — context lines
- [ ] `--json` — JSON output
- [ ] `-n` — brute force (no index)
- [ ] `-q` — quiet (exit code only)
- [ ] `-e` — multi-pattern
- [ ] `--follow` — follow symlinks
- [ ] `--type ts` — file type filter
- [ ] Shorthand: `frg "pattern"` = `frg search "pattern" .`
- [ ] Agent mode: `FRG_AGENT=1 frg "pattern"` — auto-index, JSON, no stderr

## Replace

- [ ] `frg replace "old" "new" .` — preview (dry-run)
- [ ] `frg replace "old" "new" . --write` — applies changes
- [ ] Unindexed tree — works without `frg index`
- [ ] Multiline: `frg replace '(?s)foo\nbar' 'replaced' .`

## Commands

- [ ] `frg init .` — detects project type, creates `.frgignore`
- [ ] `frg completions zsh` — outputs completion script
- [ ] `frg man` — outputs man page
- [ ] `frg upgrade` — checks for updates
- [ ] `frg watch .` — watches for file changes (Ctrl+C to stop)
- [ ] `frg --help` — shows all commands + shorthand + env vars

## Edge Cases

- [ ] Empty pattern `""` — matches every line
- [ ] Single char `z` — falls back to ScanAll
- [ ] No match `xyzzy_nothing_42` — exit code 1
- [ ] Anchored `^\s*export\s` — correct line-boundary matching
- [ ] Unicode `→` — byte-level matching
- [ ] Backreference `\b(\w{4,})\s+\1\b` — works

## Install Methods

- [ ] `brew install qhkm/tap/frg` — installs from Homebrew
- [ ] `cargo install fastripgrep` — installs from crates.io
- [ ] `curl -fsSL .../install.sh | sh` — installs from GitHub release

## Plugin

- [ ] `claude plugin list` — shows `frg@frg-marketplace` enabled
- [ ] `/frg-search` skill — loads in Claude Code

## Release Steps

```bash
# 1. Bump version
vim Cargo.toml  # update version

# 2. Commit and tag
git add -A
git commit -m "chore: bump version to X.Y.Z"
git tag vX.Y.Z
git push origin main vX.Y.Z

# 3. Publish to crates.io
cargo publish

# 4. Verify GitHub Actions
gh run list --repo qhkm/fastripgrep --limit 1
# Wait for release workflow to complete

# 5. Verify
gh release view vX.Y.Z --repo qhkm/fastripgrep
brew update && brew upgrade frg
cargo search fastripgrep
```
