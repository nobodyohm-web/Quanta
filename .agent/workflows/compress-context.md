# Context Compression & Token Saving Workflow
# Invoked via: /compress-context
# Can also be AUTO-TRIGGERED when session counter reaches 25

This workflow saves tokens by creating a dense state snapshot, allowing a fresh session to resume instantly.

## Instructions

### Step 1: Audit Current State
- List all modified files (check `git diff --name-only` or recent edits)
- Read `.claude/.session_counter` for session stats
- Identify the current active task and its completion percentage
- Note any open bugs, failing tests, or pending decisions

### Step 2: Generate CONTEXT_SUMMARY.md
Create/overwrite `CONTEXT_SUMMARY.md` in the project root with this exact structure:

```markdown
# Polytracker — Context Summary
> Generated: <timestamp>
> Session messages: <count from counter>

## Last Session
<1-2 sentence summary of what was accomplished>

## Current Task
<What is being worked on right now>
- Status: <in-progress | blocked | done>
- Files modified: <list>
- Key decisions made: <list>

## Next Steps (Priority Order)
1. <Most important next action>
2. <Second priority>
3. <Third priority>

## Open Issues
- <Bug or issue description> → <file:line if known>

## Architecture Notes
<Any architectural decisions or patterns that MUST be preserved>

## Recent Code Changes (Compact Diff)
<For each modified file, show ONLY the function signatures that changed — not full code>
```

### Step 3: Reset Counter
Delete or reset `.claude/.session_counter` to `count=0`

### Step 4: Validate
- Ensure the summary is under 80 lines (token-efficient)
- Verify all critical information is captured
- Remove any redundant or obvious information
- Do NOT include full code blocks — only function signatures and descriptions

### Step 5: Instruct the User
Output this EXACT message:

> ⛽ **Contexte compressé → `CONTEXT_SUMMARY.md`**
>
> **Pour relancer proprement :**
> 1. Ferme cette conversation
> 2. Ouvre une nouvelle
> 3. Dis : **"Reprends depuis CONTEXT_SUMMARY.md"**
>
> L'agent lira le fichier et reprendra exactement là où on s'est arrêté.
> Tokens économisés : ~95% de l'historique actuel.
