# Python Patterns — Polytracker

## Repository / Data Access Pattern
```python
class Database:
    """Central data access layer. All DB operations go through here."""

    async def get_wallet(self, address: str) -> dict[str, Any] | None:
        async with self._connect() as db:
            row = await db.execute_fetchone(
                "SELECT * FROM wallets WHERE address = ?", (address,)
            )
            return dict(row) if row else None

    async def upsert_wallet(self, address: str, **fields: Any) -> None:
        ...
```

## Service / Engine Pattern
```python
class Engine:
    """Orchestrator that composes services and runs the main loop."""

    def __init__(
        self,
        settings: Settings,
        database: Database,
        client: httpx.AsyncClient,
    ) -> None:
        self.settings = settings
        self.db = database
        self.client = client
        self._shutdown = asyncio.Event()

    async def run(self) -> None:
        while not self._shutdown.is_set():
            await self._scan_cycle()
            await asyncio.sleep(self.settings.scan_interval)

    async def stop(self) -> None:
        self._shutdown.set()
```

## Configuration Pattern
```python
from pydantic_settings import BaseSettings, SettingsConfigDict

class Settings(BaseSettings):
    scan_interval: int = 60
    max_wallets: int = 500
    telegram_token: str = ""

    model_config = SettingsConfigDict(
        env_file=".env",
        env_prefix="POLY_",
    )
```

## Structured Logging Pattern
```python
import logging

logger = logging.getLogger(__name__)

# ✅ Use lazy formatting with %s, never f-strings in log calls
logger.info("Scanning %d wallets in market %s", count, market_id)
logger.error("Failed to fetch market %s: %s", market_id, exc)
```

## Result / Either Pattern (for recoverable errors)
```python
from dataclasses import dataclass
from typing import Generic, TypeVar

T = TypeVar("T")

@dataclass(frozen=True, slots=True)
class Ok(Generic[T]):
    value: T

@dataclass(frozen=True, slots=True)
class Err:
    message: str
    code: str = "UNKNOWN"

type Result[T] = Ok[T] | Err
```
