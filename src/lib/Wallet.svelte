<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Aurora from "./Aurora.svelte";
  import Identicon from "./Identicon.svelte";
  import Torus3D from "./Torus3D.svelte";
  import Qr from "./Qr.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { untrack } from "svelte";
  import { t, type TKey } from "./i18n.svelte";
  import { getPrefs, setPrefs } from "./prefs";
  import {
    parsePaymentUri, formatPaymentUri, splitTransfer, fmtQ, shortAddr, blocksToEta, isAddress,
  } from "./quanta";

  // ── Vérité on-chain du portefeuille (get_wallet_overview) ──────
  interface UnbondingEntry { amount: number; unlock_height: number; blocks_remaining: number }
  interface WalletOverview {
    address: string;
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
  let energyKwh = $state(0);
  let myUsername = $state<string | null>(null);
  let connectionCode = $state("");
  let codeCopied = $state(false);
  let loading = $state(true);
  let nodeStatus = $state<any>(null);
  let pulse = $state(0);
  let lastHeight = 0;

  const myPk = $derived(ov?.address ?? "");
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
  let shownBalance = $state(0);
  $effect(() => {
    const target = ov?.spendable ?? 0;
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

  async function refresh() {
    try {
      ov = await invoke<WalletOverview>("get_wallet_overview");
      if (ov.height > lastHeight) { if (lastHeight > 0) pulse++; lastHeight = ov.height; }
    } catch { /* ignore */ }
    try { txs = await invoke<LedgerTx[]>("get_recent_txs"); } catch { /* ignore */ }
    try { const e = await invoke<any>("get_energy_stats"); energyKwh = e?.kwh_consumed ?? 0; } catch { /* optional */ }
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
  const receiveTarget = $derived(myUsername ? "@" + myUsername : myPk);
  const requestAmountNum = $derived.by(() => {
    const a = parseFloat(requestAmount);
    return isFinite(a) && a > 0 ? a : null;
  });
  const paymentUri = $derived(receiveTarget ? formatPaymentUri(receiveTarget, requestAmountNum) : "");

  async function copyText(s: string, mark: (v: boolean) => void) {
    if (!s) return;
    await navigator.clipboard.writeText(s);
    mark(true);
    setTimeout(() => mark(false), 1800);
  }

  function copyCode() { copyText(connectionCode, (v) => (codeCopied = v)); }
  function copyPk() { copyText(myPk, (v) => (pkCopied = v)); }
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

  <!-- ── Hero : le solde, la vérité de la chaîne ── -->
  <div class="w-hero">
    {#if loading}
      <div class="skeleton sk-bal"></div>
      <div class="skeleton sk-unit"></div>
    {:else}
      <div class="w-coin3d"><Torus3D height={120} {peers} {pulse} /></div>
      <div class="w-bal-row">
        <div class="w-balance mono" class:amt-private={privacy}>{shownBalance.toFixed(2)}</div>
        <button class="w-eye" onclick={togglePrivacy}
          aria-label={t('wallet.privacyToggle')} title={t('wallet.privacyToggle')}>
          {#if privacy}
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M3 3l18 18M10.6 10.7a2.8 2.8 0 003.9 3.9M6.6 6.7C4.3 8.1 2.7 10.2 2 12c1.6 4 5.4 7 10 7 1.9 0 3.7-.5 5.2-1.4M12 5c4.6 0 8.4 3 10 7-.4 1.1-1.1 2.2-2 3.2"/></svg>
          {:else}
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M2 12c1.6-4 5.4-7 10-7s8.4 3 10 7c-1.6 4-5.4 7-10 7S3.6 16 2 12z"/><circle cx="12" cy="12" r="3"/></svg>
          {/if}
        </button>
      </div>
      <div class="w-unit">QUANTA</div>
      <div class="w-meta" class:amt-private={privacy}>
        <span class="w-pos">+{(ov?.earned ?? 0).toFixed(2)} {t('wallet.earned')}</span>
        <span class="w-sep">·</span>
        <span>{(ov?.staked ?? 0).toFixed(2)} {t('wallet.stakedShort')}</span>
        {#if (ov?.unbonding ?? 0) > 0}
          <span class="w-sep">·</span>
          <span>{(ov?.unbonding ?? 0).toFixed(2)} {t('wallet.unbondingShort')}</span>
        {/if}
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
  </div>

  <!-- ── Feedback ── -->
  {#if feedback}
    <div class="w-fb" class:w-fb-ok={feedback.ok} class:w-fb-err={!feedback.ok} role="status">
      {feedback.msg}
    </div>
  {/if}

  <!-- ── Panel Envoyer ── -->
  {#if panel === "send"}
    <div class="w-panel">
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
          <div class="section-label">{t('wallet.send.verifyBeforeSign')}</div>
          <div class="st-row"><span class="st-k">{t('wallet.send.recipient')}</span><span class="st-v">{preview.toLabel}</span></div>
          <div class="st-row"><span class="st-k">{t('wallet.send.youSend')}</span><span class="st-v mono">{fmtQ(preview.amount)} QTA</span></div>
          <div class="st-row"><span class="st-k">{t('wallet.send.recipientGets')}</span><span class="st-v mono recv">{fmtQ(preview.net)} QTA</span></div>
          <div class="st-row"><span class="st-k">{t('wallet.send.burned')} <span class="st-pill">{t('wallet.send.deflationary')}</span></span><span class="st-v mono burn">−{fmtQ(preview.burn)} QTA</span></div>
          <div class="st-row st-total"><span class="st-k">{t('wallet.send.balanceAfter')}</span><span class="st-v mono">{fmtQ(preview.balanceAfter)} QTA</span></div>
          <button class="st-confirm" onclick={confirmSend} disabled={sendBusy}>
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
    <div class="w-panel">
      <Aurora radius={18}>
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
      </Aurora>

      <div class="w-field" style="margin-top:16px;">
        <label for="w-req-amt">{t('wallet.recv.requestAmount')}</label>
        <input id="w-req-amt" class="input" type="number" min="0.01" step="0.01"
          placeholder="0.00 — {t('wallet.recv.optional')}" bind:value={requestAmount} />
      </div>

      <div class="section-label" style="margin-top:16px;">{t('wallet.recv.uriLabel')}</div>
      <div class="w-pk-box">
        <code class="w-pk mono">{paymentUri || t('loading')}</code>
        <button class="w-copy" onclick={copyUri} disabled={!paymentUri}>
          {uriCopied ? t('wallet.recv.copied') : t('wallet.recv.copy')}
        </button>
      </div>

      {#if myUsername}
        <div class="w-pk-box" style="margin-top:8px;">
          <code class="w-pk mono" style="font-size:16px;font-weight:700;color:var(--color-accent);">@{myUsername}</code>
          <button class="w-copy" onclick={copyUsername}>
            {unameCopied ? t('wallet.recv.copied') : t('wallet.recv.copy')}
          </button>
        </div>
      {/if}

      <details style="margin-top:10px;">
        <summary style="font-size:12px;color:var(--color-text-2);cursor:pointer;">{t('wallet.recv.showPublicKey')}</summary>
        <div class="w-pk-box" style="margin-top:8px;">
          <code class="w-pk mono">{myPk || t('loading')}</code>
          <button class="w-copy" onclick={copyPk} disabled={!myPk}>
            {pkCopied ? t('wallet.recv.copied') : t('wallet.recv.copy')}
          </button>
        </div>
      </details>

      <p class="w-interop">{@html t('wallet.recv.interop')}</p>
    </div>
  {/if}

  <!-- ── Panel Staking — l'enjeu on-chain, celui qui compte au consensus ── -->
  {#if panel === "stake"}
    <div class="w-panel">
      <div class="section-label">{t('wallet.stake.title')}</div>

      <div class="w-staked-row">
        <span>{t('wallet.stake.bonded')}</span>
        <span class="mono">
          {(ov?.staked ?? 0).toFixed(2)} QUANTA
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
        <div class="section-label" style="margin-top:16px;">{t('wallet.stake.unbondingTitle')}</div>
        <div class="stk-unbond-list">
          {#each ov.unbonding_entries as e, i (i + ':' + e.unlock_height)}
            <div class="stk-unbond-row">
              <span class="mono">{e.amount.toFixed(2)} QTA</span>
              <span class="stk-eta">{etaLabel(e.blocks_remaining)} · {e.blocks_remaining.toLocaleString()} {t('wallet.stake.blocks')}</span>
            </div>
          {/each}
          {#if ov.pending_unstake > 0}
            <div class="stk-unbond-row">
              <span class="mono">{ov.pending_unstake.toFixed(2)} QTA</span>
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

      <div class="stk-warn">
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
        <div class="w-cell c-green">
          <div class="w-cell-k">{t('wallet.available')}</div>
          <div class="w-cell-v mono" class:amt-private={privacy}>{(ov?.spendable ?? 0).toFixed(2)}</div>
          <div class="w-cell-s">{t('wallet.availableSub')}</div>
        </div>
        <div class="w-cell c-violet">
          <div class="w-cell-k">{t('wallet.inStaking')}</div>
          <div class="w-cell-v mono" class:amt-private={privacy}>{(ov?.staked ?? 0).toFixed(2)}</div>
          <div class="w-cell-s">{t('wallet.inStakingSub')}</div>
        </div>
        <div class="w-cell c-amber">
          <div class="w-cell-k">{t('wallet.unbonding')}</div>
          <div class="w-cell-v mono" class:amt-private={privacy}>{(ov?.unbonding ?? 0).toFixed(2)}</div>
          <div class="w-cell-s">{t('wallet.unbondingSub')}</div>
        </div>
        <div class="w-cell c-teal">
          <div class="w-cell-k">{t('wallet.forged')}</div>
          <div class="w-cell-v mono" class:amt-private={privacy}>+{(ov?.earned ?? 0).toFixed(2)}</div>
          <div class="w-cell-s">{t('wallet.forgedSub')}</div>
        </div>
      </div>
      <div class="w-energy-foot">
        <span class="w-dot" style="background:{online ? 'var(--color-green)' : 'var(--color-text-3)'}"></span>
        <span>{online ? `${t('wallet.connected')} · ${peers} ${peers === 1 ? t('wallet.peer') : t('wallet.peers')}` : t('wallet.offline')}</span>
        <span class="w-sep">·</span>
        <span>{txs.length} {t('wallet.recentTx')}</span>
        <span class="w-sep">·</span>
        <span>⚡ {energyKwh.toFixed(3)} kWh</span>
      </div>
    {/if}
  </div>

  <!-- ── Activité ── -->
  <div class="w-section">
    <div class="section-label">{t('wallet.activity')}</div>

    <div class="w-filters" role="tablist" aria-label={t('wallet.activity')}>
      <button class="w-pill" class:w-pill-on={filter === "all"} onclick={() => setFilter("all")} role="tab" aria-selected={filter === "all"}>{t('wallet.f.all')}</button>
      <button class="w-pill" class:w-pill-on={filter === "out"} onclick={() => setFilter("out")} role="tab" aria-selected={filter === "out"}>{t('wallet.f.out')}</button>
      <button class="w-pill" class:w-pill-on={filter === "in"} onclick={() => setFilter("in")} role="tab" aria-selected={filter === "in"}>{t('wallet.f.in')}</button>
      <button class="w-pill" class:w-pill-on={filter === "mining"} onclick={() => setFilter("mining")} role="tab" aria-selected={filter === "mining"}>{t('wallet.f.mining')}</button>
      <button class="w-pill" class:w-pill-on={filter === "stakeOps"} onclick={() => setFilter("stakeOps")} role="tab" aria-selected={filter === "stakeOps"}>{t('wallet.f.stakeOps')}</button>
      <button class="w-pill" class:w-pill-on={filter === "burn"} onclick={() => setFilter("burn")} role="tab" aria-selected={filter === "burn"}>{t('wallet.f.burn')}</button>
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
              <div class="w-tx-left">
                <span class="w-tx-label">⚡ {t('wallet.mining')} · {row.label}</span>
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
            <button class="w-pager-btn"
              onclick={() => page = Math.max(0, safePage - 1)}
              disabled={safePage === 0}
              aria-label={t('wallet.prevAria')}>
              {t('wallet.prev')}
            </button>
            <span class="w-pager-info mono">{safePage + 1} / {totalPages}</span>
            <button class="w-pager-btn"
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

<style>
  /* Hero */
  .w-hero {
    display: flex; flex-direction: column; align-items: center;
    padding: 8px var(--space-5) var(--space-8); gap: var(--space-1);
  }
  .w-balance {
    font-size: 48px; font-weight: 700; letter-spacing: -0.03em; line-height: 1;
    color: var(--color-text-0);
  }
  .w-unit {
    font-size: 14px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase;
    color: var(--color-text-2); margin-top: var(--space-1);
  }
  .w-meta {
    display: flex; align-items: center; gap: var(--space-2); flex-wrap: wrap; justify-content: center;
    font-size: 13px; color: var(--color-text-2); margin-top: var(--space-3);
  }
  .w-sep { opacity: 0.5; }
  .w-pos { color: var(--quanta-accent); }
  .w-coin3d { width: 100%; max-width: 340px; margin-bottom: 4px; }
  .w-bal-row { display: flex; align-items: center; gap: 10px; }
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

  .sk-bal  { width: 180px; height: 54px; border-radius: var(--radius-sm); }
  .sk-unit { width: 48px; height: 18px; border-radius: 4px; margin-top: 8px; }
  .sk-row  { width: 100%; height: 44px; border-radius: var(--radius-sm); margin-bottom: 6px; }

  /* Actions */
  .w-actions {
    display: flex; gap: var(--space-2);
    padding: 0 var(--space-5); margin-bottom: var(--space-6);
  }
  .w-btn {
    flex: 1; display: flex; flex-direction: column; align-items: center; gap: 6px;
    padding: var(--space-3) var(--space-2); min-height: 44px;
    background: var(--quanta-bg-elevated);
    border: 1px solid var(--quanta-border); border-radius: var(--radius);
    color: var(--color-text-1);
    font-family: inherit; font-size: 12px; font-weight: 500;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
  }
  .w-btn:hover { background: var(--color-bg-2); border-color: var(--quanta-border-h); color: var(--color-text-0); }
  .w-active   { border-color: var(--quanta-accent) !important; color: var(--color-text-0) !important; }

  /* Feedback */
  .w-fb {
    margin: 0 var(--space-5) var(--space-4);
    padding: 10px 14px; border-radius: var(--radius-sm);
    font-size: 13px; animation: fadeIn 0.15s ease-out;
  }
  .w-fb-ok  { background: var(--quanta-accent-dim); color: var(--quanta-accent); border: 1px solid rgba(11,165,160,0.2); }
  .w-fb-err { background: rgba(255,68,68,0.06); color: var(--quanta-negative); border: 1px solid rgba(255,68,68,0.15); }

  /* Panels */
  .w-panel {
    margin: 0 var(--space-5) var(--space-6);
    padding: var(--space-5);
    background: var(--color-bg-1); border: 1px solid var(--quanta-border);
    border-radius: var(--radius); animation: fadeIn 0.15s ease-out;
  }
  .w-panel .section-label { margin-bottom: var(--space-4); }
  .w-field-hint { font-size: 11px; color: var(--color-text-3); margin-top: 4px; line-height: 1.45; }

  /* Carte Aurora (Recevoir) — le "moment" partageable, QR au centre. */
  .rc-card {
    display: flex; flex-direction: column; align-items: center; text-align: center;
    padding: 24px 22px; gap: 14px; color: #fff;
  }
  .rc-top { display: flex; align-items: center; gap: 12px; }
  .rc-card :global(.identicon) { border: 2px solid rgba(255,255,255,0.7); box-shadow: 0 4px 14px rgba(0,0,0,0.18); }
  .rc-pseudo { font-size: 22px; font-weight: 800; letter-spacing: 0.01em; }
  .rc-code {
    display: inline-flex; align-items: center; gap: 10px;
    background: rgba(255,255,255,0.16); border: 1px solid rgba(255,255,255,0.4);
    color: #fff; border-radius: 999px; padding: 7px 15px; cursor: pointer;
    font-size: 14px; font-weight: 700; letter-spacing: 0.12em;
    backdrop-filter: blur(6px); transition: background 0.15s ease;
  }
  .rc-code:hover { background: rgba(255,255,255,0.26); }
  .rc-code-lab { font-size: 10px; font-weight: 700; letter-spacing: 0.1em; opacity: 0.8; }
  .rc-code-act { font-size: 11px; font-weight: 600; opacity: 0.85; text-transform: lowercase; letter-spacing: 0; }
  .rc-hint { font-size: 12.5px; line-height: 1.5; opacity: 0.95; max-width: 320px; }

  /* Interop — la note honnête (standards, pas de fausse promesse). */
  .w-interop {
    margin-top: 14px; font-size: 11.5px; line-height: 1.55;
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
    font-size: 14px; gap: 12px;
  }
  .st-k { color: var(--color-text-2); display: flex; align-items: center; gap: 8px; }
  .st-v { font-weight: 600; color: var(--color-text-0); }
  .st-v.recv { color: var(--color-green); }
  .st-v.burn { color: var(--color-amber); }
  .st-total { border-bottom: 0; padding-top: 14px; }
  .st-total .st-k, .st-total .st-v { font-weight: 700; font-size: 15px; color: var(--color-text-0); }
  .st-pill {
    font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: .04em;
    padding: 2px 7px; border-radius: 999px;
    background: var(--color-accent-dim); color: var(--color-accent-hover);
  }
  .st-confirm {
    margin-top: 16px; width: 100%; padding: 14px; border: 0; cursor: pointer;
    border-radius: 999px; color: #fff; font-size: 14px; font-weight: 700;
    background: linear-gradient(120deg, #0BA5A0, #3D6FE0);
    box-shadow: 0 8px 24px rgba(11,165,160,0.28);
    transition: transform 0.12s ease, box-shadow 0.15s ease;
  }
  .st-confirm:hover:not(:disabled) { transform: translateY(-1px); box-shadow: 0 12px 30px rgba(11,165,160,0.34); }
  .st-confirm:active:not(:disabled) { transform: translateY(0) scale(0.99); }
  .st-confirm:disabled { opacity: 0.6; cursor: default; }
  .st-cancel {
    margin-top: 8px; width: 100%; padding: 10px; cursor: pointer;
    background: none; border: 0; color: var(--color-text-2); font-size: 13px;
  }
  .st-cancel:hover:not(:disabled) { color: var(--color-text-0); }
  .st-note {
    margin-top: 12px; text-align: center; font-size: 11.5px;
    color: var(--color-text-2); line-height: 1.5;
  }
  @media (prefers-reduced-motion: reduce) { .s-tray { animation: none; } }

  /* Vue d'ensemble — cartes colorées */
  .w-grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: 10px; }
  @media (min-width: 660px) { .w-grid { grid-template-columns: repeat(4, 1fr); } }
  .w-cell {
    background: var(--color-bg-1); border: 1px solid var(--color-border);
    border-radius: 12px; padding: 14px 16px;
    transition: border-color 0.15s ease, transform 0.15s ease;
  }
  .w-cell:hover { border-color: var(--color-border-hover); transform: translateY(-1px); }
  .w-cell-k {
    font-size: 11px; color: var(--color-text-3); text-transform: uppercase;
    letter-spacing: 0.04em; font-weight: 600; margin-bottom: 8px;
  }
  .w-cell-v { font-size: 22px; font-weight: 700; color: var(--color-text-0); line-height: 1; }
  .w-cell-s { font-size: 11px; color: var(--color-text-2); margin-top: 7px; line-height: 1.4; }
  .w-energy-foot {
    display: flex; flex-wrap: wrap; gap: 6px; align-items: center;
    margin-top: 12px; font-size: 11.5px; color: var(--color-text-3);
  }
  .w-dot { width: 7px; height: 7px; border-radius: 50%; display: inline-block; }
  .w-cell.c-green  { background: rgba(16,163,74,0.06);  border-color: rgba(16,163,74,0.28); }
  .w-cell.c-green  .w-cell-v { color: var(--color-green); }
  .w-cell.c-violet { background: rgba(124,58,237,0.06); border-color: rgba(124,58,237,0.28); }
  .w-cell.c-violet .w-cell-v { color: #7c3aed; }
  .w-cell.c-teal   { background: rgba(11,165,160,0.07); border-color: rgba(11,165,160,0.30); }
  .w-cell.c-teal   .w-cell-v { color: var(--color-accent); }
  .w-cell.c-amber  { background: rgba(232,129,12,0.06); border-color: rgba(232,129,12,0.28); }
  .w-cell.c-amber  .w-cell-v { color: var(--color-amber); }

  .w-form  { display: flex; flex-direction: column; gap: var(--space-4); }
  .w-field { display: flex; flex-direction: column; gap: 6px; }
  .w-field label { font-size: 12px; font-weight: 500; color: var(--color-text-1); }

  .w-pk-box {
    display: flex; align-items: flex-start; gap: var(--space-3);
    padding: var(--space-4); background: var(--color-bg-2);
    border-radius: var(--radius-sm); margin-top: var(--space-3);
  }
  .w-pk { flex: 1; font-size: 12px; line-height: 1.7; color: var(--color-text-0); word-break: break-all; }
  .w-copy {
    flex-shrink: 0; padding: 8px 16px; min-height: 44px;
    background: transparent; border: 1px solid var(--quanta-border);
    border-radius: var(--radius-sm); color: var(--color-text-0);
    font-family: inherit; font-size: 13px; font-weight: 500; cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease;
  }
  .w-copy:hover    { background: var(--color-bg-3); border-color: var(--quanta-border-h); }
  .w-copy:disabled { opacity: 0.4; cursor: not-allowed; }

  /* Staking */
  .w-staked-row {
    display: flex; justify-content: space-between; align-items: center;
    padding: var(--space-3) 0; border-bottom: 1px solid var(--quanta-border);
    font-size: 14px; color: var(--color-text-1);
  }
  .w-staked-row .mono { color: var(--color-text-0); font-weight: 500; }
  .stk-pending { font-size: 11px; color: var(--color-amber); margin-left: 6px; font-weight: 600; }
  .stk-validator {
    margin-top: 10px; font-size: 12px; color: var(--color-text-2);
    padding: 8px 12px; background: var(--color-bg-2); border-radius: 8px; line-height: 1.5;
  }
  .stk-validator.ok { color: var(--color-green); font-weight: 600; background: rgba(22,163,74,0.07); }
  .stk-unbond-list { display: flex; flex-direction: column; }
  .stk-unbond-row {
    display: flex; justify-content: space-between; align-items: center;
    padding: 9px 0; border-bottom: 1px solid var(--color-border);
    font-size: 13px;
  }
  .stk-unbond-row:last-child { border-bottom: none; }
  .stk-eta { font-size: 12px; color: var(--color-text-2); }
  .stk-forms {
    display: grid; grid-template-columns: 1fr 1fr; gap: var(--space-4);
    margin-top: var(--space-5);
  }
  @media (max-width: 640px) { .stk-forms { grid-template-columns: 1fr; } }
  .stk-form { align-self: end; }
  .stk-warn {
    display: flex; align-items: flex-start; gap: 10px;
    margin-top: var(--space-5); padding: 12px 14px;
    background: rgba(232,129,12,0.06); border: 1px solid rgba(232,129,12,0.22);
    border-radius: var(--radius-sm);
    font-size: 12px; color: var(--color-text-1); line-height: 1.55;
  }
  .stk-warn-ic {
    width: 18px; height: 18px; min-width: 18px; border-radius: 50%;
    background: var(--color-amber); color: #fff;
    display: flex; align-items: center; justify-content: center;
    font-size: 11px; font-weight: 700;
  }

  /* Sections */
  .w-section { padding: 0 var(--space-5); margin-bottom: var(--space-8); }

  /* Transactions */
  .w-tx-row {
    display: flex; justify-content: space-between; align-items: center;
    padding: 14px 0; border-bottom: 1px solid var(--quanta-border);
  }
  .w-tx-row:last-child { border-bottom: none; }
  .w-tx-left  { display: flex; flex-direction: column; gap: 2px; }
  .w-tx-right { display: flex; flex-direction: column; align-items: flex-end; gap: 2px; }
  .w-tx-label { font-size: 14px; font-weight: 500; color: var(--color-text-0); }
  .w-tx-sub   { font-size: 12px; color: var(--color-text-2); }
  .w-tx-time  { font-size: 12px; color: var(--color-text-2); }
  .w-tx-amt   { font-size: 14px; font-weight: 600; }
  .tx-in      { color: var(--quanta-accent); }
  .tx-out     { color: var(--color-text-1); }
  .tx-neutral { color: var(--color-text-2); }
  .tx-slash   { color: var(--color-red); }

  /* Filtres */
  .w-filters {
    display: flex; flex-wrap: wrap; gap: 6px;
    margin-bottom: var(--space-4);
  }
  .w-pill {
    padding: 6px 12px;
    background: var(--color-bg-1);
    border: 1px solid var(--quanta-border);
    border-radius: 999px;
    color: var(--color-text-1);
    font-family: inherit;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background 200ms ease, border-color 200ms ease, color 200ms ease;
  }
  .w-pill:hover { background: var(--color-bg-2); color: var(--color-text-0); }
  .w-pill-on { background: var(--color-bg-3); border-color: var(--quanta-border-h); color: var(--color-text-0); }

  .w-tx-burn { font-size: 11px; color: var(--color-text-2); font-weight: 400; }

  /* Pagination */
  .w-pager {
    display: flex; align-items: center; justify-content: center;
    gap: var(--space-4);
    padding: var(--space-5) 0 var(--space-2);
    border-top: 1px solid var(--quanta-border);
    margin-top: var(--space-2);
  }
  .w-pager-btn {
    padding: 8px 14px; min-height: 36px;
    background: transparent;
    border: 1px solid var(--quanta-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-1);
    font-family: inherit;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: background 200ms ease, color 200ms ease, border-color 200ms ease;
  }
  .w-pager-btn:hover:not(:disabled) {
    background: var(--color-bg-2);
    border-color: var(--quanta-border-h);
    color: var(--color-text-0);
  }
  .w-pager-btn:disabled { opacity: 0.35; cursor: not-allowed; }
  .w-pager-info { font-size: 13px; color: var(--color-text-1); min-width: 48px; text-align: center; }
</style>
