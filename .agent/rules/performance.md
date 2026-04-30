# Performance Optimization — Polytracker

## Python Async Performance

### Concurrency
- Use `asyncio.gather()` for parallel independent I/O operations
- Use `asyncio.Semaphore` to cap concurrent requests (avoid API rate limits)
- Use `asyncio.TaskGroup` (Python 3.11+) for structured concurrency
- NEVER use `time.sleep()` — always `asyncio.sleep()`
- NEVER use `requests` — always `httpx.AsyncClient`

### HTTP Client
```python
# ✅ Reuse a single client with connection pooling
async with httpx.AsyncClient(
    http2=True,
    timeout=httpx.Timeout(connect=10, read=30, write=10, pool=5),
    limits=httpx.Limits(max_connections=50, max_keepalive_connections=20),
) as client:
    await engine.run(client)
```

### Database
- Use WAL mode for aiosqlite: `PRAGMA journal_mode=WAL`
- Create indexes on frequently queried columns (wallet address, market_id, timestamp)
- Use `executemany()` for batch inserts
- Keep transactions short — commit early

### Memory
- Use generators/`async for` for large datasets instead of loading everything into memory
- Use `__slots__` on hot-path dataclasses
- Profile with `tracemalloc` when memory usage grows unexpectedly

---

## Claude Code Token Optimization

### Context Window Discipline
- **When nearing context limits**: Use `/compress-context` workflow to create a `CONTEXT_SUMMARY.md` and start a fresh session
- **Large file edits**: Use targeted line ranges, never rewrite whole files
- **Exploration**: Use `grep` and `list_dir` before `view_file` to minimize tokens read
- **Responses**: Be concise. Bullet points > paragraphs. Skip boilerplate.

### Model Selection Strategy
| Task | Recommended Model | Why |
|------|-------------------|-----|
| Quick edits, simple bugs | Haiku 4.5 | Fast, cheap, 90% quality |
| Feature implementation | Sonnet 4.6 | Best coding balance |
| Architecture, deep debug | Opus 4.6 | Maximum reasoning depth |

### Extended Thinking
- **ON by default** for: multi-file refactors, architecture, debugging complex chains
- **OFF for**: single-line fixes, formatting, simple questions
- Toggle: Option+T (macOS)

### Smart Workflow Routing
1. **Simple bug fix** → Fix directly, no planner needed
2. **New feature (< 50 lines)** → TDD: write test → implement → verify
3. **New feature (> 50 lines)** → `/plan` first → confirm → implement in phases
4. **Refactor** → `/code-review` first → prioritize issues → fix systematically
5. **Performance issue** → `/optimize-perf` → profile → targeted fix
