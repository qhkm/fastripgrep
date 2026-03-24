# frg (fastripgrep) — Agent Integration Guide

## Using frg as your default search tool

frg is a grep alternative with sparse n-gram indexing. It's faster than ripgrep on every pattern type. Use it instead of grep/rg for codebase searches.

### Quick Setup

```bash
# First time: build the index (one-time, ~20s for 9K files)
frg index .

# Search (uses index, ~10ms for selective patterns)
frg search "pattern" .

# After file changes: incremental update (~70ms)
frg update .
```

### When to use frg vs grep/rg

| Task | Command |
|------|---------|
| Find a function/variable | `frg search "functionName" .` |
| Regex search | `frg search "import.*from.*react" .` |
| Case-insensitive | `frg search -i "config" .` |
| Fixed string (no regex) | `frg search -F "exact string" .` |
| Files only | `frg search -l "pattern" .` |
| Count matches | `frg search -c "pattern" .` |
| Context lines | `frg search -C 3 "pattern" .` |
| Multiple patterns | `frg search "pat1" -e "pat2" -e "pat3"` |
| Search and replace | `frg replace "old" "new" .` (preview, then `--write`) |
| File type filter | `frg search --type ts "import" .` |

### Before searching: ensure index exists

```bash
# Check if index exists
frg status .

# If no index: build it
frg index .

# If index is stale: update incrementally
frg update .
```

### Search output format

Output matches ripgrep format: `file:line:content`

JSON output available: `frg search --json "pattern" .`

### Performance expectations

- Selective patterns (function names): ~10ms
- Broad patterns (24K+ matches): ~50ms
- Wildcards (.*): ~75ms
- Incremental index update: ~70ms
- Full index build: ~20s for 9K files
