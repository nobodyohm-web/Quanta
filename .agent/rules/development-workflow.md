# Development Workflow — Polytracker

## Task Routing (Decide in < 5 seconds)

| Task Type | Workflow | Thinking? |
|-----------|----------|-----------|
| Typo / one-liner | `/quick-fix` | OFF |
| Simple bug (< 20 lines) | Direct fix | OFF |
| New feature (< 50 lines) | TDD: test → implement | ON |
| New feature (> 50 lines) | `/plan` → confirm → implement | ON |
| Refactor | `/code-review` → prioritize → fix | ON |
| Performance issue | `/optimize-perf` → profile → fix | ON |
| Complex/unclear bug | `/deep-dive` | ON |
| Session getting long | `/compress-context` | OFF |
| Visual/UI improvement | `/ui-ux-upgrade` | ON |

## Research Before Code (Mandatory)

1. **Grep the codebase first** — the pattern you need may already exist
2. **Check PyPI** — prefer battle-tested libraries over hand-rolled code
3. **Read the API docs** — don't guess endpoint schemas
4. **Check git history** — was this tried before? Did it break?

## Implementation Checklist

- [ ] Type hints on all new functions
- [ ] Async for all I/O operations
- [ ] `logging` instead of `print`
- [ ] Error handling with specific exceptions
- [ ] No hardcoded secrets or magic numbers
- [ ] File under 800 lines after edits
- [ ] Code runs without errors

## Post-Implementation

1. Run the code: `python -m polytracker.cli run` or target module
2. Review: `/python-review` for quality check
3. Commit: `git add -A && git commit -m "<type>: <description>"`
