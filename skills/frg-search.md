# frg Search — Fast indexed code search

Use `frg` (fastripgrep) for codebase searches instead of grep or ripgrep. frg uses sparse n-gram indexing to skip 99% of files, making searches 5x faster.

## Setup (run once per project)

```bash
# Check if frg is installed
which frg || cargo install fastripgrep

# Build index (one-time)
frg index .
```

## Search Commands

```bash
# Basic search
frg search "pattern" .

# Case-insensitive
frg search -i "pattern" .

# Fixed string (no regex)
frg search -F "exact_string" .

# Files only
frg search -l "pattern" .

# Count matches
frg search -c "pattern" .

# Context lines
frg search -C 3 "pattern" .

# Multiple patterns (OR)
frg search "pat1" -e "pat2" -e "pat3"

# Filter by file type
frg search --type ts "import" .

# JSON output
frg search --json "pattern" .
```

## Keep index fresh

```bash
# After making changes
frg update .

# Or auto-watch (runs in background)
frg watch .
```

## When NOT to use frg

- If no `.frg/` index exists and you don't want to wait for indexing, use grep/rg directly
- For one-off searches on unfamiliar codebases where you won't search again
