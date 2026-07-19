<script lang="ts">
  import Identicon from "./Identicon.svelte";
  import Qr from "./Qr.svelte";
  import { t } from "./i18n.svelte";
  import { formatPaymentUri, FEEDBACK_COPY_MS } from "./quanta";
  import {
    walletOverview as walletStore, myUsername as myUsernameStore,
    myConnectionCode as myConnectionCodeStore,
  } from "./stores.svelte";

  // ── Identité + adresse : stores partagés (chauds entre navigations). ──
  $effect(() => walletStore.subscribe());
  $effect(() => myUsernameStore.subscribe());
  $effect(() => myConnectionCodeStore.subscribe());

  const ov = $derived(walletStore.value);
  const myUsername = $derived(myUsernameStore.value);
  const connectionCode = $derived(myConnectionCodeStore.value ?? "");

  const myPk = $derived(ov?.address ?? "");
  // Public, human-facing receive address (`qta1…`, checksummed). `myPk` (hex) stays
  // the identity used for the identicon; `myAddress` is what we show, copy, QR and
  // put in the payment URI. Falls back to hex until loaded.
  const myAddress = $derived(ov?.address_bech32 || ov?.address || "");

  let requestAmount = $state("");
  let codeCopied = $state(false);
  let pkCopied = $state(false);
  let unameCopied = $state(false);
  let uriCopied = $state(false);

  // ── Réception : QR + lien de paiement (format standard type BIP-21) ──
  const receiveTarget = $derived(myUsername ? "@" + myUsername : myAddress);
  const requestAmountNum = $derived.by(() => {
    const a = parseFloat(requestAmount);
    return isFinite(a) && a > 0 ? a : null;
  });
  const paymentUri = $derived(receiveTarget ? formatPaymentUri(receiveTarget, requestAmountNum) : "");

  async function copyText(s: string, mark: (v: boolean) => void) {
    if (!s) return;
    await navigator.clipboard.writeText(s);
    mark(true);
    setTimeout(() => mark(false), FEEDBACK_COPY_MS);
  }

  function copyCode() { copyText(connectionCode, (v) => (codeCopied = v)); }
  function copyPk() { copyText(myAddress, (v) => (pkCopied = v)); }
  function copyUsername() { copyText("@" + (myUsername ?? ""), (v) => (unameCopied = v)); }
  function copyUri() { copyText(paymentUri, (v) => (uriCopied = v)); }
</script>

<div class="card w-panel">
  <div class="rc-card">
    {#if myUsername}
      <div class="rc-top">
        <Identicon pubkey={myPk} size={42} />
        <div class="rc-pseudo">@{myUsername}</div>
      </div>
    {/if}
    {#if paymentUri}
      <Qr data={paymentUri} size={188} />
    {/if}
    <div class="rc-hint">{@html t('wallet.recv.scanHint')}</div>
    {#if myUsername && connectionCode}
      <button class="rc-code" onclick={copyCode} title={t('wallet.recv.copyCodeTitle')}>
        <span class="rc-code-lab">{t('wallet.recv.codeLabel')}</span>
        <span class="mono">{connectionCode}</span>
        <span class="rc-code-act">{codeCopied ? t('wallet.recv.copiedLower') : t('wallet.recv.copyLower')}</span>
      </button>
    {/if}
  </div>

  <div class="w-field rc-amt-field">
    <label for="w-req-amt">{t('wallet.recv.requestAmount')}</label>
    <input id="w-req-amt" class="input" type="number" min="0.01" step="0.01"
      placeholder="0.00 — {t('wallet.recv.optional')}" bind:value={requestAmount} />
  </div>

  <div class="section-label rc-uri-label">{t('wallet.recv.uriLabel')}</div>
  <div class="w-pk-box">
    <code class="w-pk mono">{paymentUri || t('loading')}</code>
    <button class="copy-btn w-copy" onclick={copyUri} disabled={!paymentUri}>
      {uriCopied ? t('wallet.recv.copied') : t('wallet.recv.copy')}
    </button>
  </div>

  {#if myUsername}
    <div class="w-pk-box rc-box-gap">
      <code class="w-pk mono rc-uname">@{myUsername}</code>
      <button class="copy-btn w-copy" onclick={copyUsername}>
        {unameCopied ? t('wallet.recv.copied') : t('wallet.recv.copy')}
      </button>
    </div>
  {/if}

  <details class="rc-details">
    <summary class="rc-details-summary">{t('wallet.recv.showPublicKey')}</summary>
    <div class="w-pk-box rc-box-gap">
      <code class="w-pk mono">{myAddress || t('loading')}</code>
      <button class="copy-btn w-copy" onclick={copyPk} disabled={!myAddress}>
        {pkCopied ? t('wallet.recv.copied') : t('wallet.recv.copy')}
      </button>
    </div>
  </details>

  <p class="w-interop">{@html t('wallet.recv.interop')}</p>
</div>

<style>
  /* Panels — cartes blanches globales (.card), seul l'agencement reste local */
  .w-panel { margin-bottom: var(--space-3); animation: fadeIn 0.15s ease-out; }
  .w-panel .section-label { margin-bottom: var(--space-4); }

  .w-field { display: flex; flex-direction: column; gap: 6px; }
  .w-field label { font-size: var(--text-sm); font-weight: 500; color: var(--color-text-1); }

  /* Espacements ponctuels du panel Recevoir — remplacent les style="" inline */
  .rc-amt-field { margin-top: var(--space-4); }
  .rc-uri-label { margin-top: var(--space-4); }
  .rc-box-gap { margin-top: var(--space-2); }
  .rc-uname { font-size: var(--text-lg); font-weight: 700; color: var(--color-accent); }
  .rc-details { margin-top: 10px; }
  .rc-details-summary { font-size: var(--text-sm); color: var(--color-text-2); cursor: pointer; }

  /* Recevoir — moment de marque sanctionné : hairline Aurora + lavis très
     léger sur carte claire (jamais de dégradé plein), QR au centre. */
  .rc-card {
    display: flex; flex-direction: column; align-items: center; text-align: center;
    padding: var(--space-6) 22px; gap: 14px;
    border: 1px solid transparent; border-radius: var(--radius-lg);
    background:
      linear-gradient(150deg, rgba(20,200,184,0.06), rgba(255,255,255,0) 45%, rgba(124,58,237,0.05)) padding-box,
      linear-gradient(var(--surface), var(--surface)) padding-box,
      var(--aurora-grad) border-box;
  }
  .rc-top { display: flex; align-items: center; gap: var(--space-3); }
  .rc-card :global(.identicon) { border: 2px solid var(--color-border); box-shadow: var(--shadow-sm); }
  .rc-pseudo { font-size: 22px; font-weight: 800; letter-spacing: 0.01em; color: var(--color-text-0); }
  .rc-code {
    display: inline-flex; align-items: center; gap: 10px;
    background: var(--surface); border: 1px solid var(--color-border-hover);
    color: var(--color-text-0); border-radius: 999px; padding: 7px 15px; cursor: pointer;
    box-shadow: var(--shadow-sm);
    font-size: var(--text-base); font-weight: 700; letter-spacing: 0.12em;
    transition: border-color var(--dur-fast) ease, color var(--dur-fast) ease;
  }
  .rc-code:hover { border-color: var(--cyan); color: var(--teal-700); }
  .rc-code-lab {
    font-size: 10px; font-weight: 700; letter-spacing: 0.1em; color: var(--color-text-3);
    text-transform: uppercase;
  }
  .rc-code-act { font-size: var(--text-xs); font-weight: 600; color: var(--color-text-2); text-transform: lowercase; letter-spacing: 0; }
  .rc-hint { font-size: var(--text-sm); line-height: 1.5; color: var(--color-text-2); max-width: 320px; }

  /* Interop — la note honnête (standards, pas de fausse promesse). */
  .w-interop {
    margin-top: 14px; font-size: var(--text-xs); line-height: 1.55;
    color: var(--color-text-2);
    padding: 10px 14px; background: var(--color-bg-2); border-radius: var(--radius-sm);
  }

  .w-pk-box {
    display: flex; align-items: flex-start; gap: var(--space-3);
    padding: var(--space-4); background: var(--color-bg-2);
    border-radius: var(--radius-sm); margin-top: var(--space-3);
  }
  .w-pk { flex: 1; font-size: var(--text-sm); line-height: 1.7; color: var(--color-text-0); word-break: break-all; }
  /* Copier = .copy-btn global ; seuls la taille tactile et l'état disabled sont locaux. */
  .w-copy { flex-shrink: 0; padding: var(--space-2) 14px; font-size: var(--text-sm); }
  .w-copy:disabled { opacity: 0.4; cursor: not-allowed; }
</style>
