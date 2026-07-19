<script lang="ts">
  import { t, type TKey } from "./i18n.svelte";
  import { getReceiveAddress } from "./api";
  import { myUsername } from "./stores.svelte";
  import { FEEDBACK_COPY_MS } from "./quanta";

  let { isOpen, onClose, onCommand }: {
    isOpen: boolean;
    onClose: () => void;
    onCommand: (id: string) => void;
  } = $props();

  let query = $state("");
  let activeIndex = $state(0);

  type Kind = "nav" | "action" | "copy";
  interface Entry {
    id: string;
    labelKey: TKey;
    shortcut?: string;
    groupKey: TKey;
    kind: Kind;
    run: () => void;
  }

  function goTo(view: string) {
    onCommand(view);
    onClose();
  }

  // Copy feedback — one-shot success flag per action, same convention as
  // Contacts.svelte's `copied` state (writeText + timed reset, silent catch).
  let addrCopied = $state(false);
  let unameCopied = $state(false);
  let addrTO: ReturnType<typeof setTimeout> | undefined;
  let unameTO: ReturnType<typeof setTimeout> | undefined;

  async function copyAddress() {
    try {
      const addr = await getReceiveAddress();
      await navigator.clipboard.writeText(addr);
      addrCopied = true;
      clearTimeout(addrTO);
      addrTO = setTimeout(() => (addrCopied = false), FEEDBACK_COPY_MS);
    } catch {
      // clipboard or backend unavailable — silent, no invented error command
    }
  }

  async function copyUsername() {
    const u = myUsername.value;
    if (!u) return;
    try {
      await navigator.clipboard.writeText("@" + u);
      unameCopied = true;
      clearTimeout(unameTO);
      unameTO = setTimeout(() => (unameCopied = false), FEEDBACK_COPY_MS);
    } catch {
      // silent
    }
  }

  // @pseudo needed to offer "copy my @username" — one more subscriber on the
  // app-wide refcounted store (already polled by Sidebar/Wallet/Profile).
  $effect(() => myUsername.subscribe());

  const GROUP_GO: TKey = "cmd.group.goTo";
  const GROUP_ACTIONS: TKey = "cmd.group.actions";

  // Every action here calls only functions that already exist elsewhere in
  // the app (api.ts / stores.svelte.ts) — no backend command is invented.
  // "Send"/"Receive" open Wallet (its panels aren't reachable from here
  // without touching Wallet.svelte, out of scope) — a lock/verrouiller entry
  // was considered but omitted: no lock function is exposed to this
  // component (only an internal auto-lock timer in +page.svelte).
  let commands = $derived.by((): Entry[] => {
    const nav: Entry[] = [
      { id: "wallet", labelKey: "cmd.wallet", shortcut: "W", groupKey: GROUP_GO, kind: "nav", run: () => goTo("wallet") },
      { id: "contacts", labelKey: "nav.contacts", shortcut: "C", groupKey: GROUP_GO, kind: "nav", run: () => goTo("contacts") },
      { id: "dashboard", labelKey: "cmd.dashboard", shortcut: "D", groupKey: GROUP_GO, kind: "nav", run: () => goTo("dashboard") },
      { id: "network", labelKey: "cmd.network", shortcut: "N", groupKey: GROUP_GO, kind: "nav", run: () => goTo("network") },
      { id: "profile", labelKey: "cmd.profile", shortcut: "P", groupKey: GROUP_GO, kind: "nav", run: () => goTo("profile") },
      { id: "whitepaper", labelKey: "nav.whitepaper", groupKey: GROUP_GO, kind: "nav", run: () => goTo("whitepaper") },
      { id: "settings", labelKey: "cmd.settings", shortcut: ",", groupKey: GROUP_GO, kind: "nav", run: () => goTo("settings") },
    ];
    const actions: Entry[] = [
      { id: "send", labelKey: "wallet.send", shortcut: "S", groupKey: GROUP_ACTIONS, kind: "action", run: () => goTo("wallet") },
      { id: "receive", labelKey: "wallet.receive", shortcut: "R", groupKey: GROUP_ACTIONS, kind: "action", run: () => goTo("wallet") },
      { id: "copyAddress", labelKey: "cmd.copyAddress", groupKey: GROUP_ACTIONS, kind: "copy", run: () => void copyAddress() },
    ];
    // Honest empty-state: no username yet → nothing to copy, so no entry
    // (same convention as ct.reserveFirst in Contacts.svelte).
    if (myUsername.value) {
      actions.push({ id: "copyUsername", labelKey: "cmd.copyUsername", groupKey: GROUP_ACTIONS, kind: "copy", run: () => void copyUsername() });
    }
    return [...nav, ...actions];
  });

  let filtered = $derived(
    query
      ? commands.filter(c => t(c.labelKey).toLowerCase().includes(query.toLowerCase()))
      : commands
  );

  $effect(() => {
    if (!isOpen) { query = ""; activeIndex = 0; }
  });

  $effect(() => {
    if (!isOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") { onClose(); return; }
      if (e.key === "ArrowDown") { e.preventDefault(); activeIndex = Math.min(activeIndex + 1, filtered.length - 1); }
      if (e.key === "ArrowUp") { e.preventDefault(); activeIndex = Math.max(activeIndex - 1, 0); }
      if (e.key === "Enter" && filtered[activeIndex]) {
        filtered[activeIndex].run();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  });
</script>

{#if isOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="cmd-overlay" onclick={onClose}>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="cmd-box" onclick={(e) => e.stopPropagation()}>
      <input
        class="cmd-input"
        type="text"
        placeholder={t('cmd.placeholder')}
        bind:value={query}
        id="cmd-palette-input"
      />
      <div class="cmd-list">
        {#each filtered as cmd, i (cmd.id)}
          {#if i === 0 || cmd.groupKey !== filtered[i - 1].groupKey}
            <div class="cmd-group-label">{t(cmd.groupKey)}</div>
          {/if}
          <button
            class="cmd-item"
            class:active={i === activeIndex}
            onclick={() => cmd.run()}
            id="cmd-{cmd.id}"
          >
            <span class="cmd-icon">
              {#if cmd.kind === 'nav'}
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
                  <path d="M3 8h8m0 0L7.5 4.5M11 8l-3.5 3.5" stroke-linecap="round" stroke-linejoin="round" />
                </svg>
              {:else if cmd.kind === 'action'}
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
                  <path d="M8.6 1.6L3.4 8.4h3.1L6 12.4l5.2-6.8H8.1z" stroke-linecap="round" stroke-linejoin="round" />
                </svg>
              {:else}
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
                  <rect x="2.5" y="4.6" width="7" height="8.4" rx="1.3" />
                  <path d="M5.2 4.6V3.4a1.3 1.3 0 011.3-1.3h3.7a1.3 1.3 0 011.3 1.3v6.5a1.3 1.3 0 01-1.3 1.3h-1" stroke-linecap="round" />
                </svg>
              {/if}
            </span>
            <span class="cmd-label">{t(cmd.labelKey)}</span>
            {#if cmd.id === 'copyAddress' && addrCopied}
              <span class="cmd-feedback">{t('ct.copied')}</span>
            {:else if cmd.id === 'copyUsername' && unameCopied}
              <span class="cmd-feedback">{t('ct.copied')}</span>
            {:else if cmd.shortcut}
              <span class="cmd-shortcut mono">⌘{cmd.shortcut}</span>
            {/if}
          </button>
        {/each}
        {#if filtered.length === 0}
          <div class="cmd-empty">{t('cmd.empty')} "{query}"</div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .cmd-overlay {
    position: fixed; inset: 0; z-index: 100;
    display: flex; align-items: flex-start; justify-content: center;
    padding-top: 20vh;
    background: rgba(0, 0, 0, 0.32);
    backdrop-filter: blur(4px);
    animation: fadeIn 0.1s ease-out;
  }
  .cmd-box {
    width: 520px; background: var(--surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    overflow: hidden;
    box-shadow: var(--shadow-lg);
  }
  .cmd-input {
    width: 100%; padding: 14px 18px;
    background: transparent; border: none;
    border-bottom: 1px solid var(--color-border);
    color: var(--color-text-0); font-family: inherit;
    font-size: 15px; outline: none;
  }
  .cmd-input::placeholder { color: var(--color-text-2); }
  .cmd-list { max-height: 340px; overflow-y: auto; padding: 4px 0; }
  .cmd-group-label {
    padding: 10px 18px 4px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--color-text-3);
  }
  .cmd-item {
    display: flex; align-items: center; gap: 10px;
    width: 100%; padding: 10px 18px;
    border: none; background: transparent;
    color: var(--color-text-1); cursor: pointer;
    font-family: inherit; font-size: 13px; text-align: left;
    transition: background-color 0.08s ease, color 0.08s ease, border-color 0.08s ease;
  }
  .cmd-item:hover, .cmd-item.active {
    background: var(--color-accent-dim);
    color: var(--color-text-0);
  }
  .cmd-item.active { border-left: 2px solid var(--color-accent); padding-left: 16px; }
  .cmd-icon { display: flex; align-items: center; flex-shrink: 0; color: var(--color-text-3); }
  .cmd-icon svg { display: block; }
  .cmd-item:hover .cmd-icon, .cmd-item.active .cmd-icon { color: var(--color-accent); }
  .cmd-label { flex: 1; }
  .cmd-shortcut { font-size: 11px; color: var(--color-text-3); }
  .cmd-feedback { font-size: 11px; color: var(--color-accent); font-weight: 600; }
  .cmd-empty { padding: 16px 18px; text-align: center; color: var(--color-text-3); font-size: 12px; }
</style>
