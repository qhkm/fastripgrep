# frg Replace — Search and replace across files

Use `frg replace` for codebase-wide search and replace. Shows a diff preview by default, applies changes with `--write`.

## Usage

```bash
# Preview (dry-run, default)
frg replace "old_name" "new_name" .

# Apply changes
frg replace "old_name" "new_name" . --write

# Case-insensitive
frg replace -i "oldName" "newName" .

# Fixed string (no regex)
frg replace -F "old.value" "new.value" .

# With capture groups
frg replace "fn (\w+)\(\)" "fn ${1}_v2()" .

# Filter by file type
frg replace --type ts "oldImport" "newImport" .

# Multiline replacement
frg replace '(?s)start\n.*?\nend' 'replaced' .
```

## Important

- Always preview first (default behavior) before using `--write`
- Works on unindexed trees — no `frg index` required
- Supports multiline patterns with `(?s)` flag
- Supports regex capture groups (`$1`, `$2`)
