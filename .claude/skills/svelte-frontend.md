---
description: Building or modifying Svelte components, frontend UI, CSS styles, or Tauri IPC calls
globs: ["src/**/*.svelte", "src/**/*.ts", "src/**/*.css"]
---

# Skill: Svelte 5 Frontend

## Runes — MANDATORY (Svelte 5)
```svelte
<!-- State -->
let count = $state(0);
let items = $state<string[]>([]);

<!-- Derived -->
let total = $derived(items.length);
let isValid = $derived(count > 0 && count < 100);

<!-- Effects -->
$effect(() => {
  // Runs when dependencies change
  console.log('count is', count);
});

<!-- Props -->
let { title, onClose } = $props();
```

## BANNED (Svelte 4)
```svelte
<!-- ❌ NEVER USE THESE -->
import { writable, derived } from 'svelte/store';
import { onMount, onDestroy } from 'svelte';
export let prop;  // use $props() instead
```

## Tauri IPC Pattern
```svelte
<script>
import { invoke } from "@tauri-apps/api/core";

let data = $state(null);
let error = $state('');
let loading = $state(false);

async function fetchData() {
  loading = true;
  try {
    data = await invoke<ReturnType>("command_name", { arg1: "value" });
  } catch (e) {
    error = String(e);
  } finally {
    loading = false;
  }
}

// Auto-fetch on mount
$effect(() => { fetchData(); });

// Polling pattern (e.g., every 5s)
$effect(() => {
  const interval = setInterval(fetchData, 5000);
  return () => clearInterval(interval);
});
</script>
```

## Design System
| Token | Value |
|-------|-------|
| bg-primary | #000000 |
| bg-surface | #0f0f0f |
| bg-elevated | #1a1a1a |
| bg-hover | #2a2a2a |
| accent | #00DC82 |
| text-primary | #ffffff |
| text-secondary | #888888 |
| border | #1a1a1a |
| font | Inter |
| spacing | 4, 8, 12, 16, 24, 32, 48px |

## Forbidden
- No gradients, glassmorphism, or glow effects
- No external HTTP requests (all data via Tauri IPC)
- No Tailwind CSS

## Components (src/lib/)
Wallet, Contacts, Dashboard, Network (+ Network3D globe), Explorer, Profile,
Settings, Sidebar, Welcome, CommandPalette, HelpModal — plus design helpers
(Aurora, QuantumField, EmptyState, LanguageSelect, Identicon, ChainHistory).
i18n via `t('clé')`, 6 langues (`i18n.svelte.ts` + `i18n.generated.ts`).
