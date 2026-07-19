<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Identicon from "./Identicon.svelte";
  import Qr from "./Qr.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { untrack, tick } from "svelte";
  import { t, type TKey } from "./i18n.svelte";
  import { getPrefs, setPrefs } from "./prefs";
  import { takeSendIntent } from "./intents.svelte";
  import {
    parsePaymentUri, formatPaymentUri, splitTransfer, fmtQ, shortAddr, blocksToEta, isAddress,
    TICKER, FEEDBACK_COPY_MS,
  } from "./quanta";

  // ── Vérité on-chain du portefeuille (get_wallet_overview) ──────
  interface UnbondingEntry { amount: number; unlock_height: number; blocks_remaining: number }
  interface WalletOverview {
    address: string;          // canonical on-chain hex (ledger key, tx from/to)
    address_bech32: string;   // public checksummed `qta1…` form (share/QR/send)
    height: number;
    spendable: number;
    staked: number;
    unbonding: number;
    unbonding_entries: UnbondingEntry[];
    pending_stake: number;
    pending_unstake: number;
    earned: number;
    min_validator_stake: number;
    unbonding_period_blocks: number;
  }
  interface LedgerTx {
    id: string;
    from: string;
    to: string;
    amount: number;
    tx_type: string;
    timestamp: string;
    hash: string;
  }

  type Filter = "all" | "out" | "in" | "mining" | "stakeOps" | "burn";
  const PAGE_SIZE = 10;

  let ov = $state<WalletOverview | null>(null);
  let txs = $state<LedgerTx[]>([]);
  let myUsername = $state<string | null>(null);
  let connectionCode = $state("");
  let codeCopied = $state(false);
  let loading = $state(true);
  let nodeStatus = $state<any>(null);

  const myPk = $derived(ov?.address ?? "");
  // Public, human-facing receive address (`qta1…`, checksummed). `myPk` (hex) stays
  // the identity used for tx-direction checks and the identicon; `myAddress` is what
  // we show, copy, QR and put in the payment URI. Falls back to hex until loaded.
  const myAddress = $derived(ov?.address_bech32 || ov?.address || "");
  const peers = $derived(nodeStatus?.peer_count ?? 0);
  const online = $derived(nodeStatus?.is_online ?? false);

  let panel = $state<"send" | "receive" | "stake" | null>(null);

  // Envoi
  let toAddress = $state("");
  let sendAmount = $state("");
  let sendBusy = $state(false);
  let preview = $state<null | { toLabel: string; to: string; amount: number; net: number; burn: number; balanceAfter: number }>(null);
  let preparing = $state(false);

  // Réception
  let requestAmount = $state("");

  // Staking
  let stakeAmount = $state("");
  let stakeBusy = $state(false);
  let unstakeAmount = $state("");
  let unstakeBusy = $state(false);

  let feedback = $state<{ ok: boolean; msg: string } | null>(null);
  let pkCopied = $state(false);
  let unameCopied = $state(false);
  let uriCopied = $state(false);

  let filter = $state<Filter>("all");
  let page = $state(0);
  function setFilter(f: Filter) { filter = f; page = 0; }

  // ── Mode privé : montants floutés jusqu'au survol (regard par-dessus l'épaule).
  let privacy = $state(getPrefs().privacy);
  function togglePrivacy() {
    privacy = !privacy;
    setPrefs({ ...getPrefs(), privacy });
  }

  // ── Solde animé : le montant COMPTE jusqu'à sa nouvelle valeur (ticker).
  // « Solde total » = tout ce qu'on détient sur la chaîne (dépensable + staké
  // + en-déverrouillage) ; la ventilation vit dans « Ton argent » plus bas.
  let shownBalance = $state(0);
  $effect(() => {
    const target = ov ? ov.spendable + ov.staked + ov.unbonding : 0;
    const start = untrack(() => shownBalance);
    if (Math.abs(target - start) < 1e-9) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      shownBalance = target;
      return;
    }
    const t0 = performance.now();
    const dur = 750;
    let raf = 0;
    const step = (now: number) => {
      const p = Math.min(1, (now - t0) / dur);
      const e = 1 - Math.pow(1 - p, 3);
      shownBalance = start + (target - start) * e;
      if (p < 1) raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  });

  $effect(() => {
    refresh();
    const iv = setInterval(refresh, 15_000);
    return () => clearInterval(iv);
  });

  // Cross-view send intent (Contacts « Envoyer » → single send engine). The
  // {#key view} wrapper in +page remounts this component on each navigation, so
  // a one-shot mount effect is enough — the Wallet is never kept alive in the
  // background. Pre-fill the recipient, leave the amount empty and focus it; the
  // standard Continue → net/burn preview → Confirm (sign) flow then applies as-is.
  $effect(() => {
    const to = untrack(() => takeSendIntent());
    if (!to) return;
    panel = "send";
    preview = null;
    feedback = null;
    toAddress = to;
    sendAmount = "";
    tick().then(() => document.getElementById("w-amt")?.focus());
  });

  async function refresh() {
    try {
      ov = await invoke<WalletOverview>("get_wallet_overview");
    } catch { /* ignore */ }
    try { txs = await invoke<LedgerTx[]>("get_recent_txs"); } catch { /* ignore */ }
    try { myUsername = await invoke<string | null>("get_my_username"); } catch { /* ignore */ }
    try { connectionCode = await invoke<string>("get_my_connection_code"); } catch { /* ignore */ }
    try { nodeStatus = await invoke<any>("get_node_status"); } catch { /* ignore */ }
    loading = false;
  }

  function togglePanel(p: "send" | "receive" | "stake") {
    panel = panel === p ? null : p;
    feedback = null;
  }

  // ── Envoi — étape 1 : décoder (accepte @pseudo, adresse 64-hex, ou
  // lien de paiement `quanta:`) et calculer la ventilation EXACTE (mêmes
  // maths µQTA que le ledger). On ne signe RIEN ici.
  async function prepareSend() {
    const raw = toAddress.trim();
    if (!raw) {
      feedback = { ok: false, msg: t("wallet.err.addrAmountRequired") };
      return;
    }
    preparing = true; feedback = null;
    try {
      const parsed = parsePaymentUri(raw);
      if (!parsed) {
        // Le moment BlueWallet/Electrum : un novice colle une adresse Bitcoin.
        // Expliquer clairement vaut mieux qu'une erreur générique — envoyer
        // là-dessus détruirait les pièces (réseaux distincts).
        const looksBitcoin = /^bitcoin:/i.test(raw) || /^(bc1|[13])[a-zA-Z0-9]{25,62}$/.test(raw);
        feedback = { ok: false, msg: t(looksBitcoin ? "wallet.err.bitcoinAddress" : "wallet.err.badRecipient") };
        return;
      }
      // Un lien quanta:…?amount=… pré-remplit le montant si le champ est vide.
      if (parsed.amount != null && !sendAmount.trim()) {
        sendAmount = String(parsed.amount);
      }
      const amt = parseFloat(sendAmount);
      if (!isFinite(amt) || amt <= 0) {
        feedback = { ok: false, msg: t("wallet.err.invalidAmount") };
        return;
      }
      let to = parsed.to;
      let label = shortAddr(to);
      if (!isAddress(to)) {
        const uname = to.replace(/^@/, "");
        const resolved = await invoke<string | null>("resolve_username", { username: uname });
        if (!resolved) {
          feedback = { ok: false, msg: t("wallet.err.usernameNotFound") + " : @" + uname };
          return;
        }
        to = resolved; label = "@" + uname;
      } else if (to.toLowerCase().startsWith("qta1")) {
        // Public `qta1…` address — verify the Bech32m checksum now, so a mistyped
        // character is caught at preview instead of after signing.
        const okAddr = await invoke<boolean>("validate_address", { address: to });
        if (!okAddr) {
          feedback = { ok: false, msg: t("wallet.err.badRecipient") };
          return;
        }
      }
      const { net, burn } = splitTransfer(amt);
      const bal = ov?.spendable ?? 0;
      if (amt > bal) {
        feedback = { ok: false, msg: t("wallet.err.insufficientBalance") };
        return;
      }
      preview = { toLabel: label, to, amount: amt, net, burn, balanceAfter: bal - amt };
    } catch (e: unknown) {
      feedback = { ok: false, msg: e instanceof Error ? e.message : String(e) };
    } finally { preparing = false; }
  }

  // Étape 2 — confirmer : c'est SEULEMENT ici qu'on signe et diffuse.
  async function confirmSend() {
    if (!preview) return;
    sendBusy = true; feedback = null;
    try {
      await invoke("ledger_transfer", { to: preview.to, amount: preview.amount });
      feedback = { ok: true, msg: preview.amount.toFixed(2) + " QUANTA " + t("wallet.ok.sentTo") + " " + preview.toLabel };
      toAddress = ""; sendAmount = ""; preview = null; panel = null;
      await refresh();
    } catch (e: unknown) {
      feedback = { ok: false, msg: e instanceof Error ? e.message : String(e) };
    } finally { sendBusy = false; }
  }

  function cancelPreview() { preview = null; }

  // ── Staking on-chain (ONCHAIN-STAKE-1) — le VRAI enjeu de consensus ──
  async function stake() {
    const amt = parseFloat(stakeAmount);
    if (!isFinite(amt) || amt <= 0) {
      feedback = { ok: false, msg: t("wallet.err.invalidAmount") };
      return;
    }
    stakeBusy = true; feedback = null;
    try {
      await invoke("ledger_stake", { amount: amt });
      feedback = { ok: true, msg: amt.toFixed(2) + " QUANTA " + t("wallet.ok.staked") };
      stakeAmount = "";
      await refresh();
    } catch (e: unknown) {
      feedback = { ok: false, msg: e instanceof Error ? e.message : String(e) };
    } finally { stakeBusy = false; }
  }

  async function unstake() {
    const amt = parseFloat(unstakeAmount);
    if (!isFinite(amt) || amt <= 0) {
      feedback = { ok: false, msg: t("wallet.err.invalidAmount") };
      return;
    }
    unstakeBusy = true; feedback = null;
    try {
      await invoke("ledger_unstake", { amount: amt });
      feedback = { ok: true, msg: amt.toFixed(2) + " QUANTA " + t("wallet.ok.unstaked") };
      unstakeAmount = "";
      await refresh();
    } catch (e: unknown) {
      feedback = { ok: false, msg: e instanceof Error ? e.message : String(e) };
    } finally { unstakeBusy = false; }
  }

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

  function timeAgo(ts: string): string {
    const diff = Math.floor((Date.now() - new Date(ts).getTime()) / 1000);
    if (!isFinite(diff) || diff < 0) return "";
    if (diff < 60) return t("time.now");
    if (diff < 3600) return Math.floor(diff / 60) + " " + t("time.min");
    if (diff < 86400) return Math.floor(diff / 3600) + " " + t("time.h");
    return Math.floor(diff / 86400) + " " + t("time.d");
  }

  function etaLabel(blocks: number): string {
    const { days, hours, minutes } = blocksToEta(blocks);
    if (days > 0) return `≈ ${days} ${t("time.d")} ${hours} ${t("time.h")}`;
    if (hours > 0) return `≈ ${hours} ${t("time.h")} ${minutes} ${t("time.min")}`;
    return `≈ ${Math.max(1, minutes)} ${t("time.min")}`;
  }

  const TX_KNOWN: Record<string, true> = { Transfer: true, Mining: true, Burn: true, Stake: true, Unstake: true, Slash: true };
  function txLabel(type: string): string {
    return TX_KNOWN[type] ? t(("tx." + type) as TKey) : type;
  }

  function isIncoming(tx: LedgerTx): boolean {
    return tx.to === myPk && tx.from !== myPk;
  }
  function isOutgoing(tx: LedgerTx): boolean {
    return tx.from === myPk && tx.to !== myPk;
  }

  /// Burn implicite d'un transfert sortant (le montant affiché est le NET 99 %).
  function impliedBurn(tx: LedgerTx): number | null {
    if (tx.tx_type !== "Transfer" || !isOutgoing(tx)) return null;
    return tx.amount / 99;
  }

  // ── Feed d'activité : mouvements réels + minage agrégé par jour ──
  type FeedRow =
    | { kind: "tx"; tx: LedgerTx }
    | { kind: "mine"; key: string; label: string; sum: number; count: number; ts: string };

  function dayLabel(ts: string): string {
    const d = new Date(ts);
    if (!isFinite(d.getTime())) return t("wallet.mining");
    const today = new Date();
    const yest = new Date(); yest.setDate(today.getDate() - 1);
    const same = (a: Date, b: Date) => a.toDateString() === b.toDateString();
    if (same(d, today)) return t("time.today");
    if (same(d, yest)) return t("time.yesterday");
    return d.toLocaleDateString(undefined, { day: "numeric", month: "long" });
  }

  let miningDaily = $derived.by((): FeedRow[] => {
    const map = new Map<string, { sum: number; count: number; ts: string }>();
    for (const tx of txs) {
      if (tx.tx_type !== "Mining") continue;
      const d = new Date(tx.timestamp);
      const key = isFinite(d.getTime()) ? d.toDateString() : "—";
      const a = map.get(key) ?? { sum: 0, count: 0, ts: tx.timestamp };
      a.sum += tx.amount; a.count += 1;
      if (new Date(tx.timestamp).getTime() > new Date(a.ts).getTime()) a.ts = tx.timestamp;
      map.set(key, a);
    }
    return [...map.entries()]
      .sort((x, y) => new Date(y[1].ts).getTime() - new Date(x[1].ts).getTime())
      .map(([key, a]) => ({ kind: "mine" as const, key, label: dayLabel(a.ts), sum: a.sum, count: a.count, ts: a.ts }));
  });

  const STAKE_TYPES: Record<string, true> = { Stake: true, Unstake: true, Slash: true };
  let feed = $derived.by((): FeedRow[] => {
    if (filter === "mining") return miningDaily;
    const pass = (tx: LedgerTx) => {
      switch (filter) {
        case "all": return tx.tx_type !== "Mining";
        case "out": return tx.tx_type === "Transfer" && isOutgoing(tx);
        case "in": return tx.tx_type === "Transfer" && isIncoming(tx);
        case "stakeOps": return !!STAKE_TYPES[tx.tx_type];
        case "burn": return tx.tx_type === "Burn";
        default: return false;
      }
    };
    return txs.filter(pass).map((tx) => ({ kind: "tx" as const, tx }));
  });

  let totalPages = $derived(Math.max(1, Math.ceil(feed.length / PAGE_SIZE)));
  let safePage = $derived(Math.min(page, totalPages - 1));
  let pageItems = $derived(feed.slice(safePage * PAGE_SIZE, (safePage + 1) * PAGE_SIZE));

  /// Sous-titre d'une ligne de tx suivant son type (langage simple).
  function txSub(tx: LedgerTx): string {
    switch (tx.tx_type) {
      case "Stake": return t("wallet.tx.stakeSub");
      case "Unstake": return t("wallet.tx.unstakeSub");
      case "Slash": return t("wallet.tx.slashSub");
      case "Burn": return t("wallet.tx.burnSub");
      default:
        return isIncoming(tx) ? `${t("wallet.tx.from")} ${shortAddr(tx.from)}` : `→ ${shortAddr(tx.to)}`;
    }
  }
</script>

<div class="page">

  <!-- ── Hero : le solde total — LE moment de l'écran, la typo seule ── -->
  <div class="card w-hero">
    {#if loading}
      <div class="skeleton sk-label"></div>
      <div class="skeleton sk-bal"></div>
      <div class="skeleton sk-sub"></div>
    {:else}
      <div class="w-hero-top">
        <span class="w-hero-label">{t('wallet.totalBalance')}</span>
        <button class="w-eye" onclick={togglePrivacy}
          aria-label={t('wallet.privacyToggle')} title={t('wallet.privacyToggle')}>
          {#if privacy}
            <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M3 3l18 18M10.6 10.7a2.8 2.8 0 003.9 3.9M6.6 6.7C4.3 8.1 2.7 10.2 2 12c1.6 4 5.4 7 10 7 1.9 0 3.7-.5 5.2-1.4M12 5c4.6 0 8.4 3 10 7-.4 1.1-1.1 2.2-2 3.2"/></svg>
          {:else}
            <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M2 12c1.6-4 5.4-7 10-7s8.4 3 10 7c-1.6 4-5.4 7-10 7S3.6 16 2 12z"/><circle cx="12" cy="12" r="3"/></svg>
          {/if}
        </button>
      </div>
      <div class="w-balance-row" class:amt-private={privacy}>
        <span class="w-balance">{shownBalance.toFixed(2)}</span>
        <span class="w-cur">QUANTA</span>
      </div>
      <div class="w-hero-sub" class:amt-private={privacy}>
        <span>{(ov?.spendable ?? 0).toFixed(2)} {t('wallet.available')}</span>
        <span class="w-hero-sub-dot">·</span>
        <span>{(ov?.staked ?? 0).toFixed(2)} {t('wallet.stakedShort')}</span>
        <span class="w-hero-sub-dot">·</span>
        <span>{(ov?.unbonding ?? 0).toFixed(2)} {t('wallet.unbondingShort')}</span>
      </div>
    {/if}
  </div>

  <!-- ── Actions ── -->
  <div class="w-actions">
    <button class="w-btn" class:w-active={panel === "send"} onclick={() => togglePanel("send")}>
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M22 2L11 13M22 2l-7 20-4-9-9-4 20-7z"/>
      </svg>
      <span>{t('wallet.send')}</span>
    </button>
    <button class="w-btn" class:w-active={panel === "receive"} onclick={() => togglePanel("receive")}>
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M7 10l5 5 5-5M12 15V3"/>
      </svg>
      <span>{t('wallet.receive')}</span>
    </button>
    <button class="w-btn" class:w-active={panel === "stake"} onclick={() => togglePanel("stake")}>
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <rect x="4" y="10" width="16" height="10" rx="2"/><path d="M8 10V7a4 4 0 018 0v3"/>
      </svg>
      <span>{t('wallet.stake')}</span>
    </button>
    <button class="w-btn w-btn-soon" disabled aria-disabled="true" title={t('wallet.soon')}>
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M7 8h11M15 5l3 3-3 3M17 16H6M9 13l-3 3 3 3"/>
      </svg>
      <span>{t('wallet.exchange')}</span>
      <span class="w-soon">{t('wallet.soon')}</span>
    </button>
  </div>

  <!-- ── Feedback ── -->
  {#if feedback}
    <div class="w-fb" class:w-fb-ok={feedback.ok} class:w-fb-err={!feedback.ok} role="status">
      {feedback.msg}
    </div>
  {/if}

  <!-- ── Panel Envoyer ── -->
  {#if panel === "send"}
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
  {/if}

  <!-- ── Panel Recevoir — QR + lien de paiement, le geste universel ── -->
  {#if panel === "receive"}
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
  {/if}

  <!-- ── Panel Staking — l'enjeu on-chain, celui qui compte au consensus ── -->
  {#if panel === "stake"}
    <div class="card w-panel">
      <div class="section-label">{t('wallet.stake.title')}</div>

      <!-- Pourquoi staker — le rôle réel dans le protocole (pas de rendement) -->
      <div class="stk-why-title">{t('stk.why.title')}</div>
      <div class="stk-fn-grid">
        <div class="stk-fn-card">
          <svg class="stk-fn-ic" viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 2l8 4.5v9L12 20l-8-4.5v-9L12 2z"/></svg>
          <div class="stk-fn-title">{t('stk.fn.seal.title')}</div>
          <div class="stk-fn-desc">{t('stk.fn.seal.desc')}</div>
        </div>
        <div class="stk-fn-card">
          <svg class="stk-fn-ic" viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M8 12.5l2.5 2.5L16 9.5"/></svg>
          <div class="stk-fn-title">{t('stk.fn.vote.title')}</div>
          <div class="stk-fn-desc">{t('stk.fn.vote.desc')}</div>
        </div>
        <div class="stk-fn-card">
          <svg class="stk-fn-ic" viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3l7 3.2v5.3c0 4.6-3 7.7-7 8.8-4-1.1-7-4.2-7-8.8V6.2L12 3z"/></svg>
          <div class="stk-fn-title">{t('stk.fn.bond.title')}</div>
          <div class="stk-fn-desc">{t('stk.fn.bond.desc')}</div>
        </div>
      </div>

      <div class="stk-honesty">
        <div class="stk-honesty-title">{t('stk.honesty.title')}</div>
        <div class="stk-honesty-body">{t('stk.honesty.body')}</div>
      </div>

      <div class="w-staked-row">
        <span>{t('wallet.stake.bonded')}</span>
        <span class="mono">
          {(ov?.staked ?? 0).toFixed(2)} {TICKER}
          {#if (ov?.pending_stake ?? 0) > 0}
            <span class="stk-pending">+{(ov?.pending_stake ?? 0).toFixed(2)} {t('wallet.stake.pending')}</span>
          {/if}
        </span>
      </div>

      {#if ov && (ov.staked >= ov.min_validator_stake)}
        <div class="stk-validator ok">✓ {t('wallet.stake.validatorOk')}</div>
      {:else if ov}
        <div class="stk-validator">{t('wallet.stake.validatorHint1')} {ov.min_validator_stake.toFixed(0)} {t('wallet.stake.validatorHint2')}</div>
      {/if}

      {#if ov && (ov.unbonding_entries.length > 0 || ov.pending_unstake > 0)}
        <div class="section-label stk-unbond-label">{t('wallet.stake.unbondingTitle')}</div>
        <div class="stk-unbond-list">
          {#each ov.unbonding_entries as e, i (i + ':' + e.unlock_height)}
            <div class="stk-unbond-row">
              <span class="mono">{e.amount.toFixed(2)} {TICKER}</span>
              <span class="stk-eta">{etaLabel(e.blocks_remaining)} · {e.blocks_remaining.toLocaleString()} {t('wallet.stake.blocks')}</span>
            </div>
          {/each}
          {#if ov.pending_unstake > 0}
            <div class="stk-unbond-row">
              <span class="mono">{ov.pending_unstake.toFixed(2)} {TICKER}</span>
              <span class="stk-eta">{t('wallet.stake.pending')}</span>
            </div>
          {/if}
        </div>
      {/if}

      <div class="stk-forms">
        <div class="w-form stk-form">
          <div class="w-field">
            <label for="w-stake-amt">{t('wallet.stake.amountLabel')}</label>
            <input id="w-stake-amt" class="input" type="number"
              min="0.01" step="0.01" placeholder="0.00" bind:value={stakeAmount}
              onkeydown={(e) => e.key === "Enter" && stake()} />
          </div>
          <button class="btn btn-primary" onclick={stake} disabled={stakeBusy}>
            {stakeBusy ? t('wallet.stake.staking') : t('wallet.stake.stakeBtn')}
          </button>
        </div>
        <div class="w-form stk-form">
          <div class="w-field">
            <label for="w-unstake-amt">{t('wallet.stake.unstakeLabel')}</label>
            <input id="w-unstake-amt" class="input" type="number"
              min="0.01" step="0.01" placeholder="0.00" bind:value={unstakeAmount}
              onkeydown={(e) => e.key === "Enter" && unstake()} />
          </div>
          <button class="btn btn-ghost" onclick={unstake} disabled={unstakeBusy || !ov || ov.staked <= 0}>
            {unstakeBusy ? t('wallet.stake.unstaking') : t('wallet.stake.unstakeBtn')}
          </button>
        </div>
      </div>

      <div class="stk-warn stk-warn-amber">
        <span class="stk-warn-ic">!</span>
        <span>{t('wallet.stake.warn')}</span>
      </div>
    </div>
  {/if}

  <!-- ── Ton argent — la ventilation à trois compartiments (chaîne) ── -->
  <div class="w-section">
    <div class="section-label">{t('wallet.yourMoney')}</div>
    {#if loading}
      <div class="w-info-list"><div class="skeleton sk-row"></div><div class="skeleton sk-row"></div></div>
    {:else}
      <div class="w-grid">
        <div class="card w-cell">
          <div class="w-cell-k">{t('wallet.available')}</div>
          <div class="w-cell-v mono" class:amt-private={privacy}>{(ov?.spendable ?? 0).toFixed(2)}</div>
          <div class="w-cell-s">{t('wallet.availableSub')}</div>
        </div>
        <div class="card w-cell c-teal">
          <div class="w-cell-k">{t('wallet.inStaking')}</div>
          <div class="w-cell-v mono" class:amt-private={privacy}>{(ov?.staked ?? 0).toFixed(2)}</div>
          <div class="w-cell-s">{t('wallet.inStakingSub')}</div>
        </div>
        <div class="card w-cell c-amber">
          <div class="w-cell-k">{t('wallet.unbonding')}</div>
          <div class="w-cell-v mono" class:amt-private={privacy}>{(ov?.unbonding ?? 0).toFixed(2)}</div>
          <div class="w-cell-s">{t('wallet.unbondingSub')}</div>
        </div>
        <div class="card w-cell c-green">
          <div class="w-cell-k">{t('wallet.forged')}</div>
          <div class="w-cell-v mono" class:amt-private={privacy}>+{(ov?.earned ?? 0).toFixed(2)}</div>
          <div class="w-cell-s">{t('wallet.forgedSub')}</div>
        </div>
      </div>
      <div class="w-status-foot">
        <span class="w-dot" class:online></span>
        <span>{online ? `${t('wallet.connected')} · ${peers} ${peers === 1 ? t('wallet.peer') : t('wallet.peers')}` : t('wallet.offline')}</span>
        <span class="w-sep">·</span>
        <span>{txs.length} {t('wallet.recentTx')}</span>
      </div>
    {/if}
  </div>

  <!-- ── Activité ── -->
  <div class="w-section">
    <div class="section-label">{t('wallet.activity')}</div>

    <div class="card">
    <div class="filter-tabs w-filters" role="tablist" aria-label={t('wallet.activity')}>
      <button class="filter-tab" class:active={filter === "all"} onclick={() => setFilter("all")} role="tab" aria-selected={filter === "all"}>{t('wallet.f.all')}</button>
      <button class="filter-tab" class:active={filter === "out"} onclick={() => setFilter("out")} role="tab" aria-selected={filter === "out"}>{t('wallet.f.out')}</button>
      <button class="filter-tab" class:active={filter === "in"} onclick={() => setFilter("in")} role="tab" aria-selected={filter === "in"}>{t('wallet.f.in')}</button>
      <button class="filter-tab" class:active={filter === "mining"} onclick={() => setFilter("mining")} role="tab" aria-selected={filter === "mining"}>{t('wallet.f.mining')}</button>
      <button class="filter-tab" class:active={filter === "stakeOps"} onclick={() => setFilter("stakeOps")} role="tab" aria-selected={filter === "stakeOps"}>{t('wallet.f.stakeOps')}</button>
      <button class="filter-tab" class:active={filter === "burn"} onclick={() => setFilter("burn")} role="tab" aria-selected={filter === "burn"}>{t('wallet.f.burn')}</button>
    </div>

    <div class="w-tx-list">
      {#if loading}
        <div class="skeleton sk-row"></div>
        <div class="skeleton sk-row"></div>
        <div class="skeleton sk-row"></div>
      {:else if feed.length === 0}
        <EmptyState>
          {#if filter === "all"}{t('wallet.empty.all')}
          {:else if filter === "mining"}{t('wallet.empty.mining')}
          {:else}{t('wallet.empty.other')}{/if}
        </EmptyState>
      {:else}
        {#each pageItems as row (row.kind === "tx" ? row.tx.id : row.key)}
          {#if row.kind === "mine"}
            <div class="w-tx-row">
              <div class="tx-icon w-ic-mine" aria-hidden="true">
                <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M13 3L5 13h5l-1 8 8-10h-5l1-8z"/></svg>
              </div>
              <div class="w-tx-left">
                <span class="w-tx-label">{t('wallet.mining')} · {row.label}</span>
                <span class="w-tx-sub">{row.count} {row.count > 1 ? t('wallet.rewards') : t('wallet.reward')} · {t('wallet.auto')}</span>
              </div>
              <div class="w-tx-right">
                <span class="w-tx-amt mono tx-in">+{row.sum.toFixed(2)}</span>
                <span class="w-tx-time">{timeAgo(row.ts)}</span>
              </div>
            </div>
          {:else}
            {@const tx = row.tx}
            {@const inc = isIncoming(tx)}
            {@const burn = impliedBurn(tx)}
            {@const isSlash = tx.tx_type === "Slash"}
            {@const isUnstake = tx.tx_type === "Unstake"}
            <div class="w-tx-row">
              <div class="tx-icon"
                class:w-ic-slash={isSlash}
                class:w-ic-stake={!isSlash && (tx.tx_type === "Stake" || isUnstake)}
                class:w-ic-burn={tx.tx_type === "Burn"}
                class:w-ic-in={tx.tx_type === "Transfer" && inc}
                class:w-ic-out={!isSlash && !isUnstake && tx.tx_type !== "Stake" && tx.tx_type !== "Burn" && !(tx.tx_type === "Transfer" && inc)}
                aria-hidden="true">
                {#if isSlash}
                  <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4L3 19h18L12 4z"/><path d="M12 11v3M12 16.5h.01"/></svg>
                {:else if tx.tx_type === "Stake"}
                  <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 018 0v3"/></svg>
                {:else if isUnstake}
                  <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 017.6-1.7"/></svg>
                {:else if tx.tx_type === "Burn"}
                  <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4c2.5 3-4 5-4 9a4 4 0 008 0c0-4-6.5-6-4-9z"/></svg>
                {:else if inc}
                  <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M17 7L7 17M7 9v8h8"/></svg>
                {:else}
                  <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M7 17L17 7M9 7h8v8"/></svg>
                {/if}
              </div>
              <div class="w-tx-left">
                <span class="w-tx-label" class:tx-slash={isSlash}>{txLabel(tx.tx_type)}</span>
                <span class="w-tx-sub mono">{txSub(tx)}</span>
              </div>
              <div class="w-tx-right">
                <span class="w-tx-amt mono"
                  class:tx-in={inc && !isSlash}
                  class:tx-out={!inc && !isSlash && !isUnstake}
                  class:tx-slash={isSlash}
                  class:tx-neutral={isUnstake}>
                  {isSlash ? "−" : isUnstake ? "" : inc ? "+" : "−"}{tx.amount.toFixed(2)}
                </span>
                {#if burn !== null}
                  <span class="w-tx-burn mono">−{burn.toFixed(2)} {t('wallet.burned')}</span>
                {:else}
                  <span class="w-tx-time">{timeAgo(tx.timestamp)}</span>
                {/if}
              </div>
            </div>
          {/if}
        {/each}

        {#if totalPages > 1}
          <div class="w-pager">
            <button class="btn btn-ghost btn-sm"
              onclick={() => page = Math.max(0, safePage - 1)}
              disabled={safePage === 0}
              aria-label={t('wallet.prevAria')}>
              {t('wallet.prev')}
            </button>
            <span class="w-pager-info mono">{safePage + 1} / {totalPages}</span>
            <button class="btn btn-ghost btn-sm"
              onclick={() => page = Math.min(totalPages - 1, safePage + 1)}
              disabled={safePage >= totalPages - 1}
              aria-label={t('wallet.nextAria')}>
              {t('wallet.next')}
            </button>
          </div>
        {/if}
      {/if}
    </div>
    </div>
  </div>

</div>

<style>
  /* ── Hero — le solde total, la typo seule (niveau banque : Trade Republic) ── */
  .w-hero { padding: var(--space-10) var(--space-6) var(--space-8); margin-bottom: var(--space-4); }
  .w-hero-top {
    display: flex; align-items: center; justify-content: space-between;
    margin-bottom: var(--space-2);
  }
  .w-hero-label {
    font-size: var(--text-xs); font-weight: 600; letter-spacing: 0.08em; color: var(--color-text-2);
    text-transform: uppercase;
  }
  .w-balance-row { display: flex; align-items: baseline; gap: var(--space-3); flex-wrap: wrap; }
  .w-balance {
    font-size: clamp(46px, 6.4vw, 56px); font-weight: 700; letter-spacing: -0.03em;
    line-height: 1; color: var(--color-text-0);
    font-variant-numeric: tabular-nums lining-nums;
  }
  .w-cur {
    font-size: 17px; font-weight: 600; letter-spacing: 0.02em; color: var(--color-accent-hover);
  }
  .w-sep { opacity: 0.5; }
  /* Sous-ligne : la ventilation en un coup d'œil — dépensable · staké · en déverrouillage */
  .w-hero-sub {
    display: flex; flex-wrap: wrap; align-items: baseline; gap: var(--space-2);
    margin-top: var(--space-3);
    font-size: var(--text-base); color: var(--color-text-2);
    font-variant-numeric: tabular-nums lining-nums;
  }
  .w-hero-sub-dot { color: var(--color-text-3); }
  .w-eye {
    display: flex; align-items: center; justify-content: center;
    width: 30px; height: 30px; border-radius: 8px;
    background: none; border: none; cursor: pointer;
    color: var(--color-text-3);
    transition: color 0.15s ease, background 0.15s ease;
  }
  .w-eye:hover { color: var(--color-text-1); background: var(--color-bg-2); }
  /* Mode privé : flouté au repos, révélé au survol — le regard du propriétaire. */
  .amt-private { filter: blur(10px); transition: filter 0.2s ease; }
  .amt-private:hover { filter: none; }

  /* Skeletons — base (shimmer discret sur gris chaud) + tailles */
  .skeleton {
    background: linear-gradient(90deg, var(--color-bg-2) 25%, var(--color-bg-3) 50%, var(--color-bg-2) 75%);
    background-size: 200% 100%;
    animation: sk-shimmer 1.4s ease infinite;
  }
  @keyframes sk-shimmer { from { background-position: 200% 0; } to { background-position: -200% 0; } }
  @media (prefers-reduced-motion: reduce) { .skeleton { animation: none; } }
  .sk-label { width: 96px; height: 15px; border-radius: 5px; margin-bottom: var(--space-3); }
  .sk-bal  { width: 240px; height: 60px; border-radius: var(--radius-sm); }
  .sk-sub  { width: 200px; height: 14px; border-radius: 5px; margin-top: var(--space-3); }
  .sk-row  { width: 100%; height: 44px; border-radius: var(--radius-sm); margin-bottom: 6px; }

  /* Actions — trois tuiles blanches flottantes ; l'état actif porte le teal */
  .w-actions {
    display: flex; gap: var(--space-3); margin-bottom: var(--space-3);
  }
  .w-btn {
    flex: 1; display: flex; flex-direction: column; align-items: center; gap: 6px;
    padding: var(--space-3) var(--space-2); min-height: 44px;
    background: var(--surface);
    border: 1px solid var(--color-border); border-radius: var(--radius);
    box-shadow: var(--shadow-sm);
    color: var(--color-text-1);
    font-family: inherit; font-size: var(--text-sm); font-weight: 600;
    cursor: pointer;
    transition: background var(--dur-fast) ease, border-color var(--dur-fast) ease,
      color var(--dur-fast) ease, transform var(--dur-fast) var(--ease-out),
      box-shadow var(--dur-fast) ease;
  }
  .w-btn:hover { border-color: var(--color-border-hover); color: var(--color-text-0); transform: translateY(-1px); box-shadow: var(--shadow); }
  .w-btn.w-active { border-color: var(--cyan); color: var(--cyan); background: var(--cyan-dim); transform: none; box-shadow: var(--shadow-sm); }
  .w-btn:disabled { cursor: default; }
  .w-btn-soon { opacity: 0.7; }
  .w-btn-soon:hover { transform: none; border-color: var(--color-border); color: var(--color-text-1); box-shadow: var(--shadow-sm); }
  .w-soon {
    font-size: 9px; font-weight: 700; letter-spacing: 0.05em; text-transform: uppercase;
    color: var(--color-accent-hover); line-height: 1;
  }

  /* Feedback */
  .w-fb {
    margin: 0 0 var(--space-3);
    padding: 10px 14px; border-radius: 10px;
    font-size: var(--text-base); animation: fadeIn 0.15s ease-out;
  }
  .w-fb-ok  { background: var(--cyan-dim); color: var(--teal-700); border: 1px solid var(--cyan-mid); }
  .w-fb-err { background: var(--color-bg-2); color: var(--color-text-0); border: 1px solid var(--color-border); border-left: 3px solid var(--color-text-0); font-weight: 600; }

  /* Panels — cartes blanches globales (.card), seul l'agencement reste local */
  .w-panel { margin-bottom: var(--space-3); animation: fadeIn 0.15s ease-out; }
  .w-panel .section-label { margin-bottom: var(--space-4); }
  .w-field-hint { font-size: var(--text-xs); color: var(--color-text-3); margin-top: var(--space-1); line-height: 1.45; }

  /* Titre de l'écran de confirmation (« vérifie avant de signer ») — ton sobre
     voulu : même hiérarchie que .section-label mais SANS majuscules forcées. */
  .s-tray-title {
    font-size: var(--text-xs); font-weight: 600; color: var(--color-text-3);
    margin-bottom: var(--space-4);
  }

  /* Espacements ponctuels du panel Recevoir — remplacent les style="" inline */
  .rc-amt-field { margin-top: var(--space-4); }
  .rc-uri-label { margin-top: var(--space-4); }
  .rc-box-gap { margin-top: var(--space-2); }
  .rc-uname { font-size: var(--text-lg); font-weight: 700; color: var(--color-accent); }
  .rc-details { margin-top: 10px; }
  .rc-details-summary { font-size: var(--text-sm); color: var(--color-text-2); cursor: pointer; }

  /* Titre de la section unbonding du panel Staking */
  .stk-unbond-label { margin-top: var(--space-4); }

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

  /* Vue d'ensemble — cartes blanches globales (.card) ; la couleur ne vit
     que dans le MONTANT (teal = bondé, ambre = déverrouillage, vert = forgé) */
  .w-grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: var(--space-3); }
  @media (min-width: 660px) { .w-grid { grid-template-columns: repeat(4, 1fr); } }
  .w-cell { padding: var(--space-4) 18px; }
  .w-cell-k {
    font-size: var(--text-xs); color: var(--color-text-3); text-transform: uppercase;
    letter-spacing: 0.04em; font-weight: 600; margin-bottom: var(--space-2);
  }
  .w-cell-v { font-size: 22px; font-weight: 700; color: var(--color-text-0); line-height: 1; }
  .w-cell-s { font-size: var(--text-xs); color: var(--color-text-2); margin-top: 7px; line-height: 1.4; }
  .w-status-foot {
    display: flex; flex-wrap: wrap; gap: 6px; align-items: center;
    margin-top: var(--space-3); font-size: var(--text-xs); color: var(--color-text-3);
  }
  .w-dot { width: 7px; height: 7px; border-radius: 50%; display: inline-block; background: var(--color-text-3); }
  .w-dot.online { background: var(--cyan); }
  /* Discipline couleur (niveau banque) : un seul accent. Le teal marque le
     bondé (l'état actif) ; déverrouillage et forgé restent en encre. */
  .w-cell.c-teal  .w-cell-v { color: var(--cyan); }
  .w-cell.c-amber .w-cell-v { color: var(--color-text-0); }
  .w-cell.c-green .w-cell-v { color: var(--color-text-0); }

  .w-form  { display: flex; flex-direction: column; gap: var(--space-4); }
  .w-field { display: flex; flex-direction: column; gap: 6px; }
  .w-field label { font-size: var(--text-sm); font-weight: 500; color: var(--color-text-1); }

  .w-pk-box {
    display: flex; align-items: flex-start; gap: var(--space-3);
    padding: var(--space-4); background: var(--color-bg-2);
    border-radius: var(--radius-sm); margin-top: var(--space-3);
  }
  .w-pk { flex: 1; font-size: var(--text-sm); line-height: 1.7; color: var(--color-text-0); word-break: break-all; }
  /* Copier = .copy-btn global ; seuls la taille tactile et l'état disabled sont locaux. */
  .w-copy { flex-shrink: 0; padding: var(--space-2) 14px; font-size: var(--text-sm); }
  .w-copy:disabled { opacity: 0.4; cursor: not-allowed; }

  /* Staking — sobre : le teal marque le bondé, l'ambre le déverrouillage */
  .w-staked-row {
    display: flex; justify-content: space-between; align-items: center;
    padding: var(--space-3) 0; border-bottom: 1px solid var(--color-border);
    font-size: var(--text-base); color: var(--color-text-1);
  }
  .w-staked-row .mono { color: var(--cyan); font-weight: 600; }
  .stk-pending { font-size: var(--text-xs); color: var(--color-text-2); margin-left: 6px; font-weight: 600; }
  .stk-validator {
    margin-top: 10px; font-size: var(--text-sm); color: var(--color-text-2);
    padding: var(--space-2) var(--space-3); background: var(--color-bg-2); border-radius: 8px; line-height: 1.5;
  }
  .stk-validator.ok { color: var(--teal-700); font-weight: 600; background: var(--cyan-dim); }
  .stk-unbond-list { display: flex; flex-direction: column; }
  .stk-unbond-row {
    display: flex; justify-content: space-between; align-items: center;
    padding: 9px 0; border-bottom: 1px solid var(--color-border);
    font-size: var(--text-base);
  }
  .stk-unbond-row:last-child { border-bottom: none; }
  .stk-eta { font-size: var(--text-sm); color: var(--color-text-2); }
  .stk-forms {
    display: grid; grid-template-columns: 1fr 1fr; gap: var(--space-4);
    margin-top: var(--space-5);
  }
  @media (max-width: 640px) { .stk-forms { grid-template-columns: 1fr; } }
  .stk-form { align-self: end; }
  .stk-warn {
    display: flex; align-items: flex-start; gap: 10px;
    margin-top: var(--space-5); padding: var(--space-3) 14px;
    background: var(--color-bg-2); border: 1px solid var(--color-border-hover);
    border-radius: var(--radius-sm);
    font-size: var(--text-sm); color: var(--color-text-1); line-height: 1.55;
  }
  .stk-warn-ic {
    width: 18px; height: 18px; min-width: 18px; border-radius: 50%;
    background: var(--color-text-0); color: #fff;
    display: flex; align-items: center; justify-content: center;
    font-size: var(--text-xs); font-weight: 700;
  }
  .stk-warn-amber { border-color: rgba(232,129,12,0.32); background: rgba(232,129,12,0.06); }
  .stk-warn-amber .stk-warn-ic { background: var(--color-amber); }

  /* Pourquoi staker — 3 fonctions réelles du protocole, compactes */
  .stk-why-title {
    font-size: var(--text-xs); font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase;
    color: var(--color-text-3); margin-bottom: 10px;
  }
  .stk-fn-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: var(--space-3); margin-bottom: var(--space-5); }
  @media (max-width: 640px) { .stk-fn-grid { grid-template-columns: 1fr; } }
  .stk-fn-card { padding: var(--space-3); background: var(--color-bg-1); border: 1px solid var(--color-border); border-radius: var(--radius-sm); }
  .stk-fn-ic { color: var(--teal-700); margin-bottom: 6px; }
  .stk-fn-title { font-size: var(--text-sm); font-weight: 600; color: var(--color-text-0); margin-bottom: 3px; }
  .stk-fn-desc { font-size: var(--text-xs); color: var(--color-text-2); line-height: 1.45; }

  /* Honnêteté — pas d'intérêt aujourd'hui, pièces déplacées jamais brûlées */
  .stk-honesty {
    padding: 10px var(--space-3); background: var(--color-bg-2); border-radius: var(--radius-sm);
    margin-bottom: var(--space-4);
  }
  .stk-honesty-title {
    font-size: var(--text-xs); font-weight: 700; text-transform: uppercase; letter-spacing: 0.03em;
    color: var(--color-text-2); margin-bottom: var(--space-1);
  }
  .stk-honesty-body { font-size: var(--text-sm); color: var(--color-text-1); line-height: 1.5; }

  /* Sections — le canevas respire entre les cartes */
  .w-section { margin: 20px 0 var(--space-3); }

  /* Transactions — lignes aérées sur carte blanche, hairlines internes */
  .w-tx-row {
    display: flex; align-items: center; gap: var(--space-3);
    padding: 13px 0; border-bottom: 1px solid var(--color-border);
  }
  .w-tx-row:last-child { border-bottom: none; }
  .w-tx-left  { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .w-tx-right { display: flex; flex-direction: column; align-items: flex-end; gap: 2px; margin-left: auto; }
  .w-tx-label { font-size: var(--text-base); font-weight: 500; color: var(--color-text-0); }
  .w-tx-sub   { font-size: var(--text-sm); color: var(--color-text-2); }
  .w-tx-time  { font-size: var(--text-sm); color: var(--color-text-2); }
  .w-tx-amt   { font-size: var(--text-base); font-weight: 600; }
  .tx-in      { color: var(--cyan); }
  .tx-out     { color: var(--color-text-0); }
  .tx-neutral { color: var(--color-text-2); }
  .tx-slash   { color: var(--color-text-0); }

  /* Icônes de ligne — teal entrant, encre sortant, rouge sobre pour Slash */
  .w-ic-in    { background: var(--cyan-dim); color: var(--cyan); }
  .w-ic-out   { background: var(--color-bg-3); color: var(--color-text-1); }
  .w-ic-mine  { background: var(--cyan-dim); color: var(--cyan); }
  .w-ic-stake { background: var(--cyan-dim); color: var(--teal-700); }
  .w-ic-burn  { background: var(--color-bg-3); color: var(--color-text-1); }
  .w-ic-slash { background: var(--color-text-0); color: #fff; }

  /* Filtres — vocabulaire global .filter-tabs/.filter-tab ; seul le wrap est local */
  .w-filters { flex-wrap: wrap; margin-bottom: var(--space-3); }

  .w-tx-burn { font-size: var(--text-xs); color: var(--color-text-2); font-weight: 400; }

  /* Pagination — boutons .btn-ghost globaux ; seul l'agencement est local */
  .w-pager {
    display: flex; align-items: center; justify-content: center;
    gap: var(--space-4);
    padding: var(--space-4) 0 0;
    border-top: 1px solid var(--color-border);
    margin-top: var(--space-2);
  }
  .w-pager-info { font-size: var(--text-base); color: var(--color-text-1); min-width: 48px; text-align: center; }
</style>
