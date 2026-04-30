# Python Coding Style — Polytracker

## Formatting
- **Line length**: 100 characters max (Black formatter compatible)
- **Imports**: stdlib → third-party → local, separated by blank lines. Use `isort` order.
- **Quotes**: Double quotes for strings, single quotes acceptable for dict keys
- **Trailing commas**: Always on multi-line collections

## Naming
| Element | Convention | Example |
|---------|-----------|---------|
| Functions/methods | `snake_case` | `fetch_market_data()` |
| Variables | `snake_case` | `wallet_address` |
| Classes | `PascalCase` | `ArbitrageScanner` |
| Constants | `UPPER_SNAKE_CASE` | `MAX_RETRY_COUNT` |
| Private | `_leading_underscore` | `_parse_response()` |
| Booleans | `is_/has_/should_` prefix | `is_active`, `has_position` |

## Type Hints (Mandatory)
```python
# Every function must have full type annotations
from typing import Any

async def scan_wallet(
    address: str,
    *,
    limit: int = 100,
    include_pending: bool = False,
) -> list[dict[str, Any]]:
    """Scan wallet transactions on Polymarket."""
    ...
```

## Docstrings (Google Style)
```python
def kelly_fraction(
    probability: float,
    odds: float,
    *,
    kelly_multiplier: float = 0.25,
) -> float:
    """Calculate fractional Kelly criterion bet size.

    Args:
        probability: Estimated true probability (0-1).
        odds: Decimal odds offered by the market.
        kelly_multiplier: Fraction of full Kelly to use (default quarter-Kelly).

    Returns:
        Optimal fraction of bankroll to bet.

    Raises:
        ValueError: If probability is not in [0, 1].
    """
    ...
```

## Async Patterns
```python
# ✅ Gather for parallel I/O
results = await asyncio.gather(
    fetch_market(client, market_id_1),
    fetch_market(client, market_id_2),
    fetch_market(client, market_id_3),
    return_exceptions=True,
)

# ✅ Semaphore for rate limiting
sem = asyncio.Semaphore(10)
async def rate_limited_fetch(url: str) -> dict:
    async with sem:
        return await client.get(url)

# ✅ Graceful shutdown
shutdown_event = asyncio.Event()
while not shutdown_event.is_set():
    await scan_cycle()
    await asyncio.sleep(interval)
```

## Error Handling
```python
# ✅ Specific exceptions, structured logging
try:
    data = await crawler.fetch_markets(category=category)
except httpx.TimeoutException:
    logger.warning("Market fetch timed out for category=%s", category)
    return []
except httpx.HTTPStatusError as exc:
    logger.error(
        "API error %d for category=%s: %s",
        exc.response.status_code,
        category,
        exc.response.text[:200],
    )
    raise

# ❌ NEVER
except:  # bare except
    pass   # silent swallow
```
