---
description: Fast indexed code search using frg (5x faster than ripgrep)
---

Use `frg` for code search. It indexes your codebase with sparse n-grams and searches in ~10ms instead of scanning every file.

## Setup

If frg is not installed: `cargo install fastripgrep`

For agent mode (auto-index, JSON output, no noise): `export FRG_AGENT=1`

## Commands

```bash
frg "pattern"                    # search current directory
frg "pattern" path               # search specific path
frg "pattern" . -i               # case-insensitive
frg "pattern" . -F               # fixed string (no regex)
frg "pattern" . -l               # files only
frg "pattern" . -c               # count per file
frg "pattern" . -C 3             # context lines
frg "pat1" . -e "pat2" -e "pat3" # multiple patterns (OR)
frg "pattern" . --type ts        # filter by file type
frg "pattern" . --json           # JSON output
```

## Index Management

```bash
frg index .      # build index (one-time, ~20s)
frg update .     # incremental update (~70ms)
frg status .     # check index stats
frg watch .      # auto-update on file changes
```

## When to Use

- Use frg for all codebase searches when a `.frg/` index exists
- Use grep/rg for one-off searches on unfamiliar codebases
- frg auto-indexes in agent mode (FRG_AGENT=1)
