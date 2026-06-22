<script lang="ts">
  // État vide « vivant » — un champ quantique discret dérive en fond, le message
  // flotte au-dessus. Transforme un vide en moment calme et soigné.
  import QuantumField from "./QuantumField.svelte";
  import type { Snippet } from "svelte";
  let { children, minHeight = 190 } = $props<{ children?: Snippet; minHeight?: number }>();
</script>

<div class="empty" style="min-height:{minHeight}px;">
  <QuantumField density={1.3} maxOpacity={0.75} />
  <div class="empty-inner">{@render children?.()}</div>
</div>

<style>
  .empty {
    position: relative; overflow: hidden;
    display: flex; align-items: center; justify-content: center;
    text-align: center;
    border-radius: var(--radius-lg);
    background:
      radial-gradient(120% 90% at 50% 35%, var(--color-bg-1), transparent 70%);
  }
  .empty-inner {
    position: relative; z-index: 1;
    max-width: 420px; padding: 20px 24px;
    font-size: 13px; line-height: 1.65; color: var(--color-text-2);
  }
</style>
