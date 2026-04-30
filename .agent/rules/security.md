# Python Security Rules — Polytracker

## Secret Management
```python
# ✅ ALWAYS: Environment variables via pydantic-settings
from pydantic_settings import BaseSettings

class Settings(BaseSettings):
    telegram_token: str = ""
    polymarket_api_key: str = ""

    model_config = SettingsConfigDict(env_file=".env", env_file_encoding="utf-8")

# ❌ NEVER: Hardcoded secrets
API_KEY = "sk-xxxxx"  # FORBIDDEN
```

## Input Validation
- Validate ALL external data (API responses, user input, file content)
- Use Pydantic models for API response parsing
- Sanitize wallet addresses before database queries
- Never use f-strings in SQL queries (use parameterized queries)

```python
# ✅ Parameterized SQL
await db.execute(
    "SELECT * FROM wallets WHERE address = ?",
    (address,),
)

# ❌ SQL injection risk
await db.execute(f"SELECT * FROM wallets WHERE address = '{address}'")
```

## Network Security
- Always use `httpx.AsyncClient` with timeouts
- Set reasonable connect/read timeouts (10s/30s default)
- Validate TLS certificates (default behavior, never disable)
- Rate-limit outgoing requests to avoid API bans

## Error Messages
- Never leak internal paths, stack traces, or secrets in user-facing errors
- Log full details at DEBUG level, sanitized summary at ERROR level
- Use structured logging with context fields

## Pre-Commit Checklist
- [ ] `grep -rn "sk-\|api_key\s*=\s*[\"']" polytracker/` returns nothing
- [ ] No `.env` file committed (check `.gitignore`)
- [ ] All new DB queries use parameterized `?` placeholders
- [ ] httpx clients have explicit timeouts set
