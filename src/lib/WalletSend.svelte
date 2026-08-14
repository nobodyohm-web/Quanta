<script lang="ts">
  import { untrack, tick } from "svelte";
  import { t } from "./i18n.svelte";
  import { translateError } from "./errors";
  import {
    parsePaymentUri, splitTransfer, fmtQ, shortAddr, isAddress, TICKER,
  } from "./quanta";
  import { resolveUsername, validateAddress, ledgerTransfer } from "./api";
  import { walletOverview as walletStore, recentTxs as recentTxsStore } from "./stores.svelte";

  type Feedback = { ok: boolean; msg: string };
  let {
    initialTo = "",
    onFeedback,
    onDone,
  }: {
    initialTo?: string;
    onFeedback: (fb: Feedback | null) => void;
    onDone: () => void;
  } = $props();

  // ── Données du portefeuille : store partagé (solde CHAUD entre navigations). ──
  $effect(() => walletStore.subscribe());
  const ov = $derived(walletStore.value);

  /** Re-fetch impératif du portefeuille après une action signée. */
  function refreshWallet() {
    return Promise.all([walletStore.refresh(), recentTxsStore.refresh()]);
  }

  let toAddress = $state("");
  let sendAmount = $state("");
  let sendBusy = $state(false);
  let preview = $state<null | { toLabel: string; to: string; amount: number; net: number; burn: number; balanceAfter: number }>(null);
  // BAS-1 — vrai quand le destinataire a été saisi en hexadécimal nu (sans somme
  // de contrôle). Remis à zéro à chaque préparation d'aperçu.
  let rawHex = $state(false);
  let preparing = $state(false);

  // Pré-remplissage par l'intent inter-vue (Contacts « Envoyer » → single send
  // engine). Le panneau est monté frais par le parent avec `initialTo` : on
  // remplit le destinataire, on laisse le montant vide et on le focus ; le flux
  // standard Continuer → aperçu net/burn → Confirmer (signer) s'applique tel quel.
  $effect(() => {
    const to = untrack(() => initialTo);
    if (!to) return;
    toAddress = to;
    tick().then(() => document.getElementById("w-amt")?.focus());
  });

  // ── Envoi — étape 1 : décoder (accepte @pseudo, adresse 64-hex, ou
  // lien de paiement `quanta:`) et calculer la ventilation EXACTE (mêmes
  // maths µQTA que le ledger). On ne signe RIEN ici.
  async function prepareSend() {
    const raw = toAddress.trim();
    if (!raw) {
      onFeedback({ ok: false, msg: t("wallet.err.addrAmountRequired") });
      return;
    }
    preparing = true; onFeedback(null);
    try {
      const parsed = parsePaymentUri(raw);
      if (!parsed) {
        // Le moment BlueWallet/Electrum : un novice colle une adresse Bitcoin.
        // Expliquer clairement vaut mieux qu'une erreur générique — envoyer
        // là-dessus détruirait les pièces (réseaux distincts).
        const looksBitcoin = /^bitcoin:/i.test(raw) || /^(bc1|[13])[a-zA-Z0-9]{25,62}$/.test(raw);
        onFeedback({ ok: false, msg: t(looksBitcoin ? "wallet.err.bitcoinAddress" : "wallet.err.badRecipient") });
        return;
      }
      // Un lien quanta:…?amount=… pré-remplit le montant si le champ est vide.
      if (parsed.amount != null && !sendAmount.trim()) {
        sendAmount = String(parsed.amount);
      }
      const amt = parseFloat(sendAmount);
      if (!isFinite(amt) || amt <= 0) {
        onFeedback({ ok: false, msg: t("wallet.err.invalidAmount") });
        return;
      }
      let to = parsed.to;
      let label = shortAddr(to);
      rawHex = false;
      if (!isAddress(to)) {
        const uname = to.replace(/^@/, "");
        const resolved = await resolveUsername(uname);
        if (!resolved) {
          onFeedback({ ok: false, msg: t("wallet.err.usernameNotFound") + " : @" + uname });
          return;
        }
        to = resolved; label = "@" + uname;
      } else if (to.toLowerCase().startsWith("qta1")) {
        // Public `qta1…` address — verify the Bech32m checksum now, so a mistyped
        // character is caught at preview instead of after signing.
        const okAddr = await validateAddress(to);
        if (!okAddr) {
          onFeedback({ ok: false, msg: t("wallet.err.badRecipient") });
          return;
        }
      } else {
        // BAS-1 — hexadécimal nu : aucune somme de contrôle. Un caractère faux
        // reste 64 caractères hexadécimaux parfaitement valides, donc rien, nulle
        // part, ne peut détecter la faute — les fonds partiraient vers une adresse
        // qui n'appartient à personne. On ne bloque pas (une adresse résolue depuis
        // un `@pseudo` ou lue sur la chaîne est légitime), on le DIT, à l'écran où
        // la décision se prend.
        rawHex = true;
      }
      const { net, burn } = splitTransfer(amt);
      const bal = ov?.spendable ?? 0;
      if (amt > bal) {
        onFeedback({ ok: false, msg: t("wallet.err.insufficientBalance") });
        return;
      }
      preview = { toLabel: label, to, amount: amt, net, burn, balanceAfter: bal - amt };
    } catch (e: unknown) {
      onFeedback({ ok: false, msg: translateError(e) });
    } finally { preparing = false; }
  }

  // Étape 2 — confirmer : c'est SEULEMENT ici qu'on signe et diffuse.
  async function confirmSend() {
    if (!preview) return;
    sendBusy = true; onFeedback(null);
    try {
      await ledgerTransfer(preview.to, preview.amount);
      onFeedback({ ok: true, msg: preview.amount.toFixed(2) + " QUANTA " + t("wallet.ok.sentTo") + " " + preview.toLabel });
      toAddress = ""; sendAmount = ""; preview = null; onDone();
      await refreshWallet();
    } catch (e: unknown) {
      onFeedback({ ok: false, msg: translateError(e) });
    } finally { sendBusy = false; }
  }

  function cancelPreview() { preview = null; }
</script>

<div class="card w-panel">
  {#if !preview}
    <div class="section-label">{t('wallet.send.title')}</div>
    <div class="w-form">
      <div class="w-field">
        <label for="w-to">{t('wallet.send.toLabel')}</label>
        <input id="w-to" class="input" type="text"
          placeholder={t('wallet.send.toPlaceholder')} bind:value={toAddress} />
        <span class="w-field-hint">{t('wallet.send.acceptsHint')}</span>
      </div>
      <div class="w-field">
        <label for="w-amt">{t('wallet.send.amountLabel')}</label>
        <input id="w-amt" class="input" type="number"
          min="0.01" step="0.01" placeholder="0.00" bind:value={sendAmount}
          onkeydown={(e) => e.key === "Enter" && prepareSend()} />
      </div>
      <button class="btn btn-primary" onclick={prepareSend} disabled={preparing}>
        {preparing ? t('wallet.send.checking') : t('wallet.send.continue')}
      </button>
    </div>
  {:else}
    <div class="s-tray">
      <div class="s-tray-title">{t('wallet.send.verifyBeforeSign')}</div>
      <div class="st-row"><span class="st-k">{t('wallet.send.recipient')}</span><span class="st-v">{preview.toLabel}</span></div>
      <!--
        A11 (audit 2026-08-13) — l'écran de confirmation n'affichait QUE le libellé
        (`@bob`) et jamais l'adresse résolue. Sur le chemin `@pseudo`, un
        détournement du registre de pseudos (R2) était donc totalement invisible :
        l'utilisateur relisait « @bob », signait, et les fonds partaient chez le
        voleur. L'adresse réellement signée est désormais montrée telle quelle —
        c'est le seul élément que l'écran ait à vérifier, puisque c'est le seul que
        la transaction porte.
      -->
      <div class="st-row"><span class="st-k">{t('wallet.send.recipientAddress')}</span><span class="st-v mono st-addr">{preview.to}</span></div>
      {#if rawHex}
        <div class="st-warn">{t('wallet.send.rawHexWarning')}</div>
      {/if}
      <div class="st-row"><span class="st-k">{t('wallet.send.youSend')}</span><span class="st-v mono">{fmtQ(preview.amount)} {TICKER}</span></div>
      <div class="st-row"><span class="st-k">{t('wallet.send.recipientGets')}</span><span class="st-v mono recv">{fmtQ(preview.net)} {TICKER}</span></div>
      <div class="st-row"><span class="st-k">{t('wallet.send.burned')} <span class="st-pill">{t('wallet.send.deflationary')}</span></span><span class="st-v mono burn">−{fmtQ(preview.burn)} {TICKER}</span></div>
      <div class="st-row st-total"><span class="st-k">{t('wallet.send.balanceAfter')}</span><span class="st-v mono">{fmtQ(preview.balanceAfter)} {TICKER}</span></div>
      <button class="btn btn-primary st-confirm" onclick={confirmSend} disabled={sendBusy}>
        {sendBusy ? t('wallet.send.signing') : t('wallet.send.confirm')}
      </button>
      <button class="st-cancel" onclick={cancelPreview} disabled={sendBusy}>{t('wallet.send.cancel')}</button>
      <div class="st-note">{t('wallet.send.signNote')}</div>
    </div>
  {/if}
</div>

<style>
  /* Panels — cartes blanches globales (.card), seul l'agencement reste local */
  .w-panel { margin-bottom: var(--space-3); animation: fadeIn 0.15s ease-out; }
  .w-panel .section-label { margin-bottom: var(--space-4); }
  .w-field-hint { font-size: var(--text-xs); color: var(--color-text-3); margin-top: var(--space-1); line-height: 1.45; }

  /* A11 — l'adresse signée, lisible en entier : elle doit pouvoir être comparée
     caractère par caractère, donc elle passe à la ligne au lieu d'être tronquée. */
  .st-warn {
    margin: 4px 0 8px;
    padding: 8px 10px;
    border-radius: 8px;
    font-size: 12px;
    line-height: 1.45;
    color: #b45309;
    background: rgba(180, 83, 9, 0.09);
    border: 1px solid rgba(180, 83, 9, 0.22);
  }
  .st-addr { font-size: var(--text-xs); word-break: break-all; text-align: right; max-width: 60%; }

  /* Titre de l'écran de confirmation (« vérifie avant de signer ») — ton sobre
     voulu : même hiérarchie que .section-label mais SANS majuscules forcées. */
  .s-tray-title {
    font-size: var(--text-xs); font-weight: 600; color: var(--color-text-3);
    margin-bottom: var(--space-4);
  }

  .w-form  { display: flex; flex-direction: column; gap: var(--space-4); }
  .w-field { display: flex; flex-direction: column; gap: 6px; }
  .w-field label { font-size: var(--text-sm); font-weight: 500; color: var(--color-text-1); }

  /* Tiroir d'envoi décodé */
  .s-tray { animation: tray-in 0.32s cubic-bezier(.2,.7,.3,1); }
  @keyframes tray-in {
    from { opacity: 0; transform: translateY(8px) scale(0.985); }
    to   { opacity: 1; transform: none; }
  }
  .st-row {
    display: flex; align-items: center; justify-content: space-between;
    padding: 11px 0; border-bottom: 1px solid var(--color-border);
    font-size: var(--text-base); gap: var(--space-3);
  }
  .st-k { color: var(--color-text-2); display: flex; align-items: center; gap: var(--space-2); }
  .st-v { font-weight: 600; color: var(--color-text-0); }
  .st-v.recv { color: var(--cyan); }
  .st-v.burn { color: var(--color-text-2); }
  .st-total { border-bottom: 0; padding-top: 14px; }
  .st-total .st-k, .st-total .st-v { font-weight: 700; font-size: var(--text-lg); color: var(--color-text-0); }
  .st-pill {
    font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: .04em;
    padding: 2px 7px; border-radius: 8px;
    background: var(--cyan-dim); color: var(--teal-700);
  }
  /* Confirmer = .btn-primary global (teal plein) ; seul l'agencement est local. */
  .st-confirm { margin-top: var(--space-4); width: 100%; }
  .st-cancel {
    margin-top: var(--space-2); width: 100%; padding: 10px; cursor: pointer;
    background: none; border: 0; color: var(--color-text-2); font-size: var(--text-base);
    font-family: inherit;
  }
  .st-cancel:hover:not(:disabled) { color: var(--color-text-0); }
  .st-note {
    margin-top: var(--space-3); text-align: center; font-size: var(--text-xs);
    color: var(--color-text-2); line-height: 1.5;
  }
  @media (prefers-reduced-motion: reduce) { .s-tray { animation: none; } }
</style>
