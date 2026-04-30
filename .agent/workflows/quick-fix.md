# Quick Fix Workflow
# Invoked via: /quick-fix

Ultra-fast, minimal-token workflow for simple bugs and one-liner fixes.

## Rules
1. **No planning phase** — this is for obvious fixes only
2. **Max 1 file change** — if it needs more, use `/plan` instead
3. **Read → Fix → Run → Confirm** — 4 steps, no more
4. **Response under 10 lines** — be surgical

## Steps

1. Read the relevant file section (targeted line range only, not full file)
2. Apply the fix with minimal diff
3. Run the code to verify
4. Report in exactly this format:

```
✅ Fixed: <what was wrong>
📁 File: <filename>:<line>
🔧 Change: <one-line description of the change>
```
