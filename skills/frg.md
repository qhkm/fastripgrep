# frg — Fast indexed code search & replace

Use `frg` (fastripgrep) instead of grep/ripgrep. 5x faster via sparse n-gram indexing.

## Setup

```bash
# Install
cargo install fastripgrep

# For agent mode: auto-index, JSON output, no noise
export FRG_AGENT=1
```

## Search

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

## Replace

```bash
frg replace "old" "new" .           # preview (dry-run)
frg replace "old" "new" . --write   # apply changes
frg replace -i "old" "new" .        # case-insensitive
frg replace "fn (\w+)" "fn ${1}_v2" . # capture groups
```

## Index Management

```bash
frg index .      # build index (one-time)
frg update .     # incremental update (~70ms)
frg status .     # check index stats
frg watch .      # auto-update on file changes
```
