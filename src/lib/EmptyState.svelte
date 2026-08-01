<script lang="ts">
  // Bank-grade empty state — calm and static: a quiet monochrome mark, no motion,
  // no decor. Message content is passed in by the caller (already i18n'd).
  import type { Snippet } from "svelte";
  let { children, minHeight = 190 } = $props<{ children?: Snippet; minHeight?: number }>();
</script>

<div class="empty" style="min-height:{minHeight}px;">
  <svg class="empty-mark" width="26" height="26" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" aria-hidden="true">
    <circle cx="8" cy="8" r="6" />
    <path d="M8 5.6v3.4" stroke-linecap="round" />
    <circle cx="8" cy="11.1" r="0.6" fill="currentColor" stroke="none" />
  </svg>
  <div class="empty-inner">{@render children?.()}</div>
</div>

<style>
  .empty {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 12px;
    text-align: center;
    border-radius: var(--radius-lg);
    background: var(--color-bg-1);
  }
  .empty-mark { color: var(--color-text-3); }
  .empty-inner {
    max-width: 420px; padding: 0 24px;
    font-size: 13px; line-height: 1.65; color: var(--color-text-2);
  }
</style>
