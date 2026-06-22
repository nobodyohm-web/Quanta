<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Identicon from "./Identicon.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { t } from "./i18n.svelte";

  type Contact = { username: string; pk: string; code: string; addedAt: number };
  const STORE = "quanta-contacts";

  let myUsername = $state<string | null>(null);
  let myCode = $state("");
  let copied = $state<"" | "pseudo" | "code">("");
  let contacts = $state<Contact[]>([]);

  // Add form
  let addPseudo = $state("");
  let addCode = $state("");
  let adding = $state(false);
  let addErr = $state("");
  let addOk = $state("");

  // Inline send
  let sendFor = $state<string | null>(null); // pk du contact en cours d'envoi
  let sendAmount = $state("");
  let sendBusy = $state(false);
  let sendMsg = $state<{ ok: boolean; text: string } | null>(null);

  function load() {
    try {
      const raw = localStorage.getItem(STORE);
      contacts = raw ? JSON.parse(raw) : [];
    } catch { contacts = []; }
  }
  function persist() {
    try { localStorage.setItem(STORE, JSON.stringify(contacts)); } catch {}
  }

  $effect(() => {
    load();
    refreshMe();
    const iv = setInterval(refreshMe, 8000);
    return () => clearInterval(iv);
  });

  async function refreshMe() {
    try { myUsername = await invoke<string | null>("get_my_username"); } catch {}
    try { myCode = await invoke<string>("get_my_connection_code"); } catch {}
  }

  async function copy(kind: "pseudo" | "code") {
    const text = kind === "pseudo" ? "@" + (myUsername ?? "") : myCode;
    try { await navigator.clipboard.writeText(text); copied = kind; setTimeout(() => (copied = ""), 1800); } catch {}
  }

  async function addContact() {
    addErr = ""; addOk = "";
    const u = addPseudo.trim().replace(/^@/, "");
    if (!u || !addCode.trim()) { addErr = t('ct.errFields'); return; }
    adding = true;
    try {
      const v = await invoke<{ username: string; pk: string; connection_code: string }>(
        "verify_connection", { username: u, code: addCode.trim() }
      );
      if (contacts.some(c => c.pk === v.pk)) {
        addErr = `${t('ct.alreadyPre')}@${v.username}${t('ct.alreadyPost')}`;
      } else {
        contacts = [{ username: v.username, pk: v.pk, code: v.connection_code, addedAt: Date.now() }, ...contacts];
        persist();
        addOk = `${t('ct.addedPre')}@${v.username}${t('ct.addedPost')}`;
        addPseudo = ""; addCode = "";
        setTimeout(() => (addOk = ""), 3000);
      }
    } catch (e) {
      addErr = String(e);
    } finally {
      adding = false;
    }
  }

  function removeContact(pk: string) {
    contacts = contacts.filter(c => c.pk !== pk);
    persist();
    if (sendFor === pk) sendFor = null;
  }

  function openSend(pk: string) {
    sendFor = sendFor === pk ? null : pk;
    sendAmount = ""; sendMsg = null;
  }

  async function sendTo(c: Contact) {
    const amt = parseFloat(sendAmount);
    if (!isFinite(amt) || amt <= 0) { sendMsg = { ok: false, text: t('ct.invalidAmount') }; return; }
    sendBusy = true; sendMsg = null;
    try {
      await invoke("ledger_transfer", { to: c.pk, amount: amt });
      sendMsg = { ok: true, text: `${amt.toFixed(2)}${t('ct.sentMid')}@${c.username}${t('ct.sentPost')}` };
      sendAmount = "";
      setTimeout(() => { sendMsg = null; sendFor = null; }, 2500);
    } catch (e) {
      sendMsg = { ok: false, text: e instanceof Error ? e.message : String(e) };
    } finally {
      sendBusy = false;
    }
  }
</script>

<div class="page">
  <div class="page-header">
    <div>
      <div class="page-title">{t('ct.title')}</div>
      <div class="page-sub">{t('ct.subtitle')}</div>
    </div>
  </div>

  <!-- Mon adresse à partager -->
  <div class="card" style="margin-bottom:12px;">
    <div class="card-title">{t('ct.cardTitle')}</div>
    <p style="font-size:13px;color:var(--color-text-2);margin-bottom:14px;">
      {@html t('ct.cardDesc')}
    </p>
    <div class="cc-grid">
      <div class="cc-item">
        <div class="cc-k">{t('ct.pseudo')}</div>
        <div class="cc-v mono">{myUsername ? "@" + myUsername : "—"}</div>
        <button class="copy-btn" onclick={() => copy("pseudo")} disabled={!myUsername}>
          {copied === "pseudo" ? t('ct.copied') : t('ct.copy')}
        </button>
      </div>
      <div class="cc-item">
        <div class="cc-k">{t('ct.connCode')}</div>
        <div class="cc-v mono" style="letter-spacing:0.08em;">{myCode || "—"}</div>
        <button class="copy-btn" onclick={() => copy("code")} disabled={!myCode}>
          {copied === "code" ? t('ct.copied') : t('ct.copy')}
        </button>
      </div>
    </div>
    {#if !myUsername}
      <div style="font-size:12px;color:var(--color-amber);margin-top:12px;">
        {t('ct.reserveFirst')}
      </div>
    {/if}
  </div>

  <!-- Ajouter un proche -->
  <div class="card" style="margin-bottom:12px;">
    <div class="card-title">{t('ct.addTitle')}</div>
    <div class="add-grid">
      <div class="form-group" style="margin:0;">
        <div class="form-label">{t('ct.theirPseudo')}</div>
        <input class="input" placeholder="@maman" bind:value={addPseudo} />
      </div>
      <div class="form-group" style="margin:0;">
        <div class="form-label">{t('ct.theirCode')}</div>
        <input class="input mono" placeholder="K7P2-QM9X" bind:value={addCode}
          onkeydown={(e) => e.key === 'Enter' && addContact()} />
      </div>
      <button class="btn btn-primary" style="align-self:end;" onclick={addContact} disabled={adding}>
        {adding ? t('ct.verifying') : t('ct.link')}
      </button>
    </div>
    {#if addErr}<div style="font-size:12px;color:var(--color-red);margin-top:10px;">{addErr}</div>{/if}
    {#if addOk}<div style="font-size:12px;color:var(--color-green);margin-top:10px;">{addOk}</div>{/if}
  </div>

  <!-- Mes proches -->
  <div class="card">
    <div class="card-title">{t('ct.myContacts')} · {contacts.length}</div>
    {#if contacts.length === 0}
      <EmptyState minHeight={150}>{t('ct.empty')}</EmptyState>
    {:else}
      {#each contacts as c (c.pk)}
        <div class="ct-row">
          <Identicon pubkey={c.pk} size={36} />
          <div style="flex:1;min-width:0;">
            <div style="font-weight:700;color:var(--color-accent);">@{c.username}</div>
            <div class="mono" style="font-size:11px;color:var(--color-text-3);">{c.code}</div>
          </div>
          <button class="btn btn-primary btn-sm" onclick={() => openSend(c.pk)}>{t('ct.send')}</button>
          <button class="btn btn-ghost btn-sm" onclick={() => removeContact(c.pk)} title={t('ct.remove')} aria-label={t('ct.remove')}>×</button>
        </div>
        {#if sendFor === c.pk}
          <div class="ct-send">
            <input class="input" type="number" min="0.01" step="0.01" placeholder={t('ct.amountPlaceholder')}
              bind:value={sendAmount} onkeydown={(e) => e.key === 'Enter' && sendTo(c)} />
            <button class="btn btn-primary" onclick={() => sendTo(c)} disabled={sendBusy}>
              {#if sendBusy}{t('ct.sending')}{:else}{t('ct.sendToPre')}@{c.username}{/if}
            </button>
          </div>
          {#if sendMsg}
            <div style="font-size:12px;margin:6px 0 10px;color:{sendMsg.ok ? 'var(--color-green)' : 'var(--color-red)'};">{sendMsg.text}</div>
          {/if}
        {/if}
      {/each}
    {/if}
  </div>
</div>

<style>
  .cc-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .cc-item {
    background: var(--color-bg-2); border: 1px solid var(--color-border);
    border-radius: 12px; padding: 14px 16px; display: flex; flex-direction: column; gap: 8px;
  }
  .cc-k { font-size: 11px; text-transform: uppercase; letter-spacing: .05em; color: var(--color-text-3); font-weight: 600; }
  .cc-v { font-size: 20px; font-weight: 700; color: var(--color-text-0); }
  .cc-item .copy-btn { align-self: flex-start; }

  .add-grid { display: grid; grid-template-columns: 1fr 1fr auto; gap: 12px; align-items: end; }

  .ct-row { display: flex; align-items: center; gap: 12px; padding: 10px 0; border-bottom: 1px solid var(--color-border); }
  .ct-row:last-child { border-bottom: none; }
  .ct-send { display: flex; gap: 8px; padding: 4px 0 6px 48px; }
  .ct-send .input { flex: 1; }

  @media (max-width: 720px) {
    .cc-grid, .add-grid { grid-template-columns: 1fr; }
  }
</style>
