---
description: Search and replace across files with diff preview
---

Use `frg replace` for codebase-wide search and replace. Shows a diff preview by default, applies changes with `--write`. Works on unindexed trees — no `frg index` required.

## Commands

```bash
frg replace "old" "new" .             # preview (dry-run)
frg replace "old" "new" . --write     # apply changes
frg replace -i "old" "new" .          # case-insensitive
frg replace -F "old.val" "new.val" .  # fixed string
frg replace "fn (\w+)" "fn ${1}_v2" . # capture groups
frg replace --type ts "old" "new" .   # filter by file type
frg replace '(?s)start\n.*?\nend' 'replaced' . # multiline
```

## Important

- Always preview first (default) before using `--write`
- Supports regex capture groups ($1, $2)
- Supports multiline patterns with (?s) flag
- Works without an index — walks files directly
