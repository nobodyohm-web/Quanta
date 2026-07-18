<script lang="ts">
  // Live-event toasts — the app breathes: every mining reward, sealed block
  // and incoming transfer surfaces instantly (Tauri events, no polling),
  // with a subtle generated chime. Light cards, teal accent, auto-dismiss.
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { untrack } from "svelte";
  import { t } from "./i18n.svelte";
  import { forge, seal, receive } from "./sound";
  import { shortAddr } from "./quanta";

  let { myAddress = "" } = $props<{ myAddress?: string }>();

  interface Toast {
    id: number;
    kind: "mined" | "sealed" | "received";
    title: string;
    sub: string;
  }

  let toasts = $state<Toast[]>([]);
  let seq = 0;

  function push(kind: Toast["kind"], title: string, sub: string) {
    const id = ++seq;
    toasts = [...toasts.slice(-3), { id, kind, title, sub }];
    setTimeout(() => dismiss(id), 4600);
  }

  function dismiss(id: number) {
    toasts = toasts.filter((x) => x.id !== id);
  }

  // The address is read inside the async handlers via this box so the
  // listeners (created once) always see the latest value.
  let addr = untrack(() => myAddress);
  $effect(() => { addr = myAddress; });

  $effect(() => {
    const unsubs: UnlistenFn[] = [];
    let alive = true;
    (async () => {
      const u1 = await listen<{ amount: number; kwh: number }>("quanta://mined", (e) => {
        const a = e.payload?.amount ?? 0;
        if (a <= 0) return;
        forge();
        push("mined", `+${a.toFixed(4)} QUANTA`, t("toast.mined"));
      });
      const u2 = await listen<{ index: number; txs: number; mine: boolean }>(
        "quanta://block-sealed",
        (e) => {
          if (!e.payload?.mine) return; // remote blocks stay quiet (sync floods)
          seal();
          push(
            "sealed",
            t("toast.sealedTpl")
              .replace("{n}", String(e.payload.index))
              .replace("{t}", String(e.payload.txs)),
            t("toast.sealedSub"),
          );
        },
      );
      const u3 = await listen<{ from: string; to: string; amount: number; tx_type: string }>(
        "quanta://tx-applied",
        (e) => {
          const p = e.payload;
          if (!p || p.tx_type !== "Transfer" || !addr || p.to !== addr) return;
          receive();
          push(
            "received",
            `+${p.amount.toFixed(2)} QUANTA`,
            `${t("toast.receivedFrom")} ${shortAddr(p.from)}`,
          );
        },
      );
      if (!alive) { u1(); u2(); u3(); return; }
      unsubs.push(u1, u2, u3);
    })();
    return () => {
      alive = false;
      unsubs.forEach((u) => u());
    };
  });
</script>

{#if toasts.length > 0}
  <div class="toast-stack" role="status" aria-live="polite">
    {#each toasts as toast (toast.id)}
      <button class="toast t-{toast.kind}" onclick={() => dismiss(toast.id)}>
        <span class="toast-ic" aria-hidden="true">
          {#if toast.kind === "mined"}⚡{:else if toast.kind === "sealed"}◈{:else}↓{/if}
        </span>
        <span class="toast-body">
          <span class="toast-title mono">{toast.title}</span>
          <span class="toast-sub">{toast.sub}</span>
        </span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .toast-stack {
    position: fixed; top: 16px; right: 16px; z-index: 200;
    display: flex; flex-direction: column; gap: 8px;
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    display: flex; align-items: center; gap: 11px;
    min-width: 230px; max-width: 320px;
    padding: 11px 14px;
    background: var(--surface);
    border: 1px solid var(--color-border);
    border-left: 3px solid var(--color-accent);
    border-radius: var(--radius);
    box-shadow: var(--shadow-lg);
    cursor: pointer; text-align: left;
    font-family: inherit;
    animation: toast-in var(--dur-med) var(--ease-spring);
  }
  .toast.t-received { border-left-color: var(--color-green); }
  .toast.t-sealed { border-left-color: var(--color-sealed-stone); }
  @keyframes toast-in {
    from { opacity: 0; transform: translateX(18px) scale(0.97); }
    to   { opacity: 1; transform: none; }
  }
  @media (prefers-reduced-motion: reduce) { .toast { animation: none; } }
  .toast-ic {
    width: 28px; height: 28px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
    border-radius: 8px; font-size: 13px;
    background: var(--cyan-dim); color: var(--color-accent);
  }
  .t-received .toast-ic { background: rgba(22,163,74,0.1); color: var(--color-green); }
  .t-sealed .toast-ic { background: var(--teal-100); color: var(--color-sealed-stone); }
  .toast-body { display: flex; flex-direction: column; gap: 1px; }
  .toast-title { font-size: 13.5px; font-weight: 700; color: var(--color-text-0); }
  .toast-sub { font-size: 11.5px; color: var(--color-text-2); }
</style>
