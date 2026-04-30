# API Integration Specialist Skill

You are an expert at integrating with external REST/WebSocket APIs, specifically Polymarket's Gamma API and CLOB API.

## Methodology

### Step 1: Research the API
- Read the official API documentation first
- Check for rate limits, pagination, and authentication requirements
- Identify the exact endpoint URL, HTTP method, and response schema
- Check for geo-restrictions (Polymarket blocks certain regions)

### Step 2: Design the Client
```python
# Standard pattern for Polytracker API clients
import httpx
import logging
from typing import Any

logger = logging.getLogger(__name__)

class PolymarketClient:
    BASE_URL = "https://gamma-api.polymarket.com"

    def __init__(self, client: httpx.AsyncClient) -> None:
        self._client = client

    async def fetch(
        self,
        endpoint: str,
        *,
        params: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        url = f"{self.BASE_URL}/{endpoint}"
        response = await self._client.get(url, params=params)
        response.raise_for_status()
        return response.json()
```

### Step 3: Handle Pagination
```python
async def fetch_all_pages(
    self,
    endpoint: str,
    *,
    page_size: int = 100,
) -> list[dict[str, Any]]:
    results: list[dict[str, Any]] = []
    offset = 0
    while True:
        page = await self.fetch(
            endpoint,
            params={"limit": page_size, "offset": offset},
        )
        if not page:
            break
        results.extend(page)
        if len(page) < page_size:
            break
        offset += page_size
    return results
```

### Step 4: Error Handling & Resilience
- Retry on 429 (rate limit) with exponential backoff
- Retry on 5xx with max 3 attempts
- Fail fast on 4xx (client error) — fix the request
- Always set timeouts
- Log response status and timing for debugging

### Step 5: Response Validation
- Parse API responses into Pydantic models
- Handle missing/optional fields gracefully
- Log and skip malformed records, don't crash the whole pipeline
