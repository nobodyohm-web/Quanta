# Python Hooks — Polytracker

## PostToolUse Hooks

Configure in `~/.claude/settings.json` under `"hooks"`:

### After File Edit
- **Type check**: Run `python -m py_compile <file>` after editing `.py` files
- **Print audit**: Warn if `print(` is found in edited Python files
- **Import sort**: Check import order matches isort conventions

### After Command Execution
- **Error detection**: If a command exits non-zero, read the last 50 lines of output and suggest a fix

## Stop Hooks (End of Session)

- **Counter update**: Update `.claude/.session_counter` with final message count
- **Print audit**: Check all modified `.py` files for `print()` statements
- **Type hint audit**: Warn about functions missing type hints in modified files

## Auto-Compression Hook

When the session counter reaches 25:
1. Auto-generate `CONTEXT_SUMMARY.md`
2. Notify the user to start a fresh session
3. This is the HIGHEST PRIORITY hook — overrides all other work
