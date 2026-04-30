# Performance Optimization Workflow

This workflow focuses on deep technical improvements, algorithmic efficiency, and resource optimization of the application.

## When to use
Use this when the user requests technical improvements, speedups, refactoring for efficiency, or reduction in memory/CPU usage.

## Instructions for Claude

1.  **Profiling & Analysis**:
    *   Analyze the target code for bottlenecks (e.g., nested loops, redundant database queries, inefficient DOM manipulation, synchronous blocking code).

2.  **Algorithmic Improvements**:
    *   Identify data structures that can be optimized (e.g., using sets instead of lists for lookups, proper indexing).
    *   Propose caching mechanisms (memoization, LRU cache) for expensive operations.

3.  **Concurrency & Async**:
    *   Convert synchronous blocking operations to asynchronous where appropriate.
    *   Look for opportunities to parallelize independent tasks (e.g., Promise.all in JS, asyncio.gather in Python).

4.  **Implementation**:
    *   Present the optimization plan with estimated benefits.
    *   WAIT for user confirmation.
    *   Apply the changes, ensuring exact behavioral equivalence (unless explicitly changing a feature).
