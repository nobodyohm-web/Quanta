# Deep Dive Debugging Workflow
# Invoked via: /deep-dive

This workflow performs an exhaustive investigation of a bug, performance issue, or unexpected behavior.

## When to Use
When a simple fix attempt has failed, or the root cause is unclear.

## Instructions

### Phase 1: Evidence Collection (DO NOT SKIP)
1. Read the FULL traceback or error output
2. Read the failing function AND its callers (trace up the call stack)
3. Check recent git changes: `git log --oneline -10` and `git diff HEAD~3`
4. Check the database schema if DB-related
5. Check the API response format if API-related

### Phase 2: Hypothesis Generation
- List 3-5 possible root causes, ranked by probability
- For each hypothesis, state what evidence would confirm or deny it
- Format:
  ```
  Hypothesis 1 (70%): <description>
  → Confirm: <what to check>
  → Deny: <what would disprove this>
  ```

### Phase 3: Systematic Testing
- Test each hypothesis starting from highest probability
- Use targeted `print`/`logger.debug` at critical junctions (remove after)
- Run the code after each test to observe behavior
- Eliminate hypotheses one by one

### Phase 4: Fix & Verify
1. Apply the minimal fix
2. Run the code — confirm the bug is gone
3. Check for the same pattern elsewhere in the codebase
4. Add a regression test if the bug was non-trivial

### Phase 5: Report
Provide a 5-line summary:
- **Bug**: What was broken
- **Root Cause**: Why it was broken
- **Fix**: What was changed
- **Impact**: What else could have been affected
- **Prevention**: How to prevent similar bugs
