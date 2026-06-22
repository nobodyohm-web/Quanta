<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import Aurora from "./Aurora.svelte";
  import Identicon from "./Identicon.svelte";
  import Network3D from "./Network3D.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { t, type TKey } from "./i18n.svelte";

  interface Reputation {
    public_key: string;
    atn_balance: number;
    atn_earned: number;
    atn_staked: number;
    trust_score: number;
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
  interface EnergyStats {
    kwh_consumed: number;
    atn_mined: number;
    uptime_minutes: number;
  }

  type Filter = "all" | "out" | "in" | "mining" | "burn";
  const PAGE_SIZE = 10;

  let rep     = $state<Reputation | null>(null);
  let txs     = $state<LedgerTx[]>([]);
  let energy  = $state<EnergyStats | null>(null);
  let myPk    = $state("");
  let myUsername = $state<string | null>(null);
  let unameCopied = $state(false);
  let connectionCode = $state("");
  let codeCopied = $state(false);
  let loading = $state(true);

  // Infos réseau + rareté (pertinentes et concrètes pour l'utilisateur).
  let nodeStatus = $state<any>(null);
  let chain = $state<any>(null);
  // Infos PROPRES au portefeuille (pas de doublon avec la page Réseau).
  const avail = $derived(Math.max(0, (rep?.atn_balance ?? 0) - (rep?.atn_staked ?? 0)));
  // Part RÉELLE de l'offre en circulation détenue par ce portefeuille (0 si chaîne vide).
  // AUCUNE conversion en euros : QUANTA n'est coté sur aucun marché — tout prix € serait inventé.
  const circulating = $derived(chain?.total_supply_qta ?? 0);
  const supplyShare = $derived(circulating > 0 ? ((rep?.atn_balance ?? 0) / circulating) * 100 : 0);
  const peers = $derived(nodeStatus?.peer_count ?? 0);
  const online = $derived(nodeStatus?.is_online ?? false);
  const txCount = $derived(txs?.length ?? 0);

  let panel = $state<"send" | "receive" | "stake" | null>(null);

  let toAddress  = $state("");
  let sendAmount = $state("");
  let sendBusy   = $state(false);
  // Aperçu décodé avant signature (anti-signature aveugle + moment de confiance).
  let preview = $state<null | { toLabel: string; to: string; amount: number; net: number; burn: number; balanceAfter: number }>(null);
  let preparing = $state(false);

  let stakeAmount = $state("");
  let stakeBusy   = $state(false);

  let feedback = $state<{ ok: boolean; msg: string } | null>(null);
  let pkCopied = $state(false);

  // ── Filter + pagination state ──
  let filter = $state<Filter>("all");
  let page   = $state(0);

  function setFilter(f: Filter) {
    filter = f;
    page = 0; // tout changement de filtre repart à la page 0
  }

  onMount(() => {
    refresh();
    const iv = setInterval(refresh, 30_000);
    return () => clearInterval(iv);
  });

  async function refresh() {
    try {
      rep = await invoke<Reputation>("get_my_reputation");
      txs = await invoke<LedgerTx[]>("get_recent_txs");
    } catch { /* ignore */ }
    try { energy = await invoke<EnergyStats>("get_energy_stats"); } catch { /* optional */ }
    try { myPk   = await invoke<string>("get_public_key"); }       catch { /* ignore */ }
    try { myUsername = await invoke<string | null>("get_my_username"); } catch { /* ignore */ }
    try { connectionCode = await invoke<string>("get_my_connection_code"); } catch { /* ignore */ }
    try { nodeStatus = await invoke<any>("get_node_status"); } catch { /* ignore */ }
    try { chain = await invoke<any>("get_chain_overview", { limit: 1 }); } catch { /* ignore */ }
    loading = false;
  }

  function copyCode() {
    if (!connectionCode) return;
    navigator.clipboard?.writeText(connectionCode);
    codeCopied = true;
    setTimeout(() => (codeCopied = false), 1500);
  }

  function togglePanel(p: "send" | "receive" | "stake") {
    panel = panel === p ? null : p;
    feedback = null;
  }

  // Étape 1 — décoder l'envoi : résoudre le destinataire et calculer la
  // ventilation EXACTE (mêmes maths que le ledger : µQTA, burn = floor(amount/100)).
  // On ne signe RIEN ici : l'utilisateur voit d'abord ce qu'il va signer.
  async function prepareSend() {
    const amt = parseFloat(sendAmount);
    const raw = toAddress.trim();
    if (!raw || !isFinite(amt) || amt <= 0) {
      feedback = { ok: false, msg: t("wallet.err.addrAmountRequired") };
      return;
    }
    preparing = true; feedback = null;
    try {
      let to = raw;
      let label = shortPk(raw);
      const looksLikeKey = /^[0-9a-fA-F]{64}$/.test(raw);
      if (!looksLikeKey) {
        const uname = raw.replace(/^@/, "");
        const resolved = await invoke<string | null>("resolve_username", { username: uname });
        if (!resolved) {
          feedback = { ok: false, msg: t("wallet.err.usernameNotFound") + " : @" + uname };
          preparing = false;
          return;
        }
        to = resolved; label = "@" + uname;
      }
      // Maths identiques au backend : burn = floor(montant_µQTA / 100), net = montant - burn.
      const amtMicro = Math.round(amt * 1_000_000);
      const burnMicro = Math.floor(amtMicro / 100);
      const burn = burnMicro / 1_000_000;
      const net = (amtMicro - burnMicro) / 1_000_000;
      const bal = rep?.atn_balance ?? 0;
      if (amt > bal) {
        feedback = { ok: false, msg: t("wallet.err.insufficientBalance") };
        preparing = false;
        return;
      }
      preview = { toLabel: label, to, amount: amt, net, burn, balanceAfter: bal - amt };
    } catch (e: unknown) {
      feedback = { ok: false, msg: e instanceof Error ? e.message : String(e) };
    } finally { preparing = false; }
  }

  // Étape 2 — confirmer : c'est SEULEMENT ici qu'on signe et qu'on diffuse.
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

  function fmtQ(n: number): string {
    return n.toLocaleString("fr-FR", { minimumFractionDigits: 2, maximumFractionDigits: 6 });
  }

  async function stake() {
    const amt = parseFloat(stakeAmount);
    if (!isFinite(amt) || amt <= 0) {
      feedback = { ok: false, msg: t("wallet.err.invalidAmount") };
      return;
    }
    stakeBusy = true; feedback = null;
    try {
      await invoke("stake_atn", { amount: amt });
      feedback = { ok: true, msg: amt.toFixed(2) + " QUANTA " + t("wallet.ok.staked") };
      stakeAmount = "";
      await refresh();
    } catch (e: unknown) {
      feedback = { ok: false, msg: e instanceof Error ? e.message : String(e) };
    } finally { stakeBusy = false; }
  }

  async function copyPk() {
    if (!myPk) return;
    await navigator.clipboard.writeText(myPk);
    pkCopied = true;
    setTimeout(() => { pkCopied = false; }, 2000);
  }

  async function copyUsername() {
    if (!myUsername) return;
    await navigator.clipboard.writeText("@" + myUsername);
    unameCopied = true;
    setTimeout(() => { unameCopied = false; }, 2000);
  }

  function timeAgo(ts: string): string {
    const diff = Math.floor((Date.now() - new Date(ts).getTime()) / 1000);
    if (!isFinite(diff) || diff < 0) return "";
    if (diff < 60) return t("time.now");
    if (diff < 3600) return Math.floor(diff / 60) + " " + t("time.min");
    if (diff < 86400) return Math.floor(diff / 3600) + " " + t("time.h");
    return Math.floor(diff / 86400) + " " + t("time.d");
  }

  function shortPk(s: string): string {
    return s.length > 14 ? s.slice(0, 6) + "…" + s.slice(-4) : s;
  }

  const TX_KNOWN: Record<string, true> = { Transfer: true, Mining: true, Burn: true, Stake: true, Unstake: true };
  function txLabel(type: string): string {
    return TX_KNOWN[type] ? t(("tx." + type) as TKey) : type;
  }

  function isIncoming(tx: LedgerTx): boolean {
    return tx.to === myPk && tx.from !== myPk;
  }

  function isOutgoing(tx: LedgerTx): boolean {
    return tx.from === myPk && tx.to !== myPk;
  }

  /// Le ledger applique 1 % de burn-and-mint sur chaque transfert.
  /// Le montant affiché sur la tx Transfer est le NET (99 %), donc
  /// burn implicite = montant_net / 99. Affiché uniquement sur les
  /// transferts sortants (l'expéditeur est celui qui paie le burn).
  function impliedBurn(tx: LedgerTx): number | null {
    if (tx.tx_type !== "Transfer" || !isOutgoing(tx)) return null;
    return tx.amount / 99;
  }

  // ── Feed d'activité : mouvements RÉELS + minage agrégé par jour ──
  // Le minage frappe une récompense/minute → afficher chaque ligne est du bruit
  // redondant (déjà résumé par la carte « Total forgé »). On le regroupe par jour.
  type FeedRow =
    | { kind: "tx"; tx: LedgerTx }
    | { kind: "mine"; key: string; label: string; sum: number; count: number; ts: string };

  function dayLabel(ts: string): string {
    const d = new Date(ts);
    if (!isFinite(d.getTime())) return "Minage";
    const today = new Date();
    const yest = new Date(); yest.setDate(today.getDate() - 1);
    const same = (a: Date, b: Date) => a.toDateString() === b.toDateString();
    if (same(d, today)) return "Aujourd'hui";
    if (same(d, yest)) return "Hier";
    return d.toLocaleDateString("fr-FR", { day: "numeric", month: "long" });
  }

  // Minage regroupé par jour (1 ligne/jour, plus récent d'abord).
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

  // « Tout » = mouvements réels (transferts, burns) — le minage a son onglet dédié.
  let feed = $derived.by((): FeedRow[] => {
    if (filter === "mining") return miningDaily;
    const pass = (tx: LedgerTx) => {
      switch (filter) {
        case "all":  return tx.tx_type !== "Mining";
        case "out":  return tx.tx_type === "Transfer" && isOutgoing(tx);
        case "in":   return tx.tx_type === "Transfer" && isIncoming(tx);
        case "burn": return tx.tx_type === "Burn";
        default:     return false;
      }
    };
    return txs.filter(pass).map((tx) => ({ kind: "tx" as const, tx }));
  });

  let totalPages = $derived(Math.max(1, Math.ceil(feed.length / PAGE_SIZE)));
  let safePage  = $derived(Math.min(page, totalPages - 1));
  let pageItems = $derived(feed.slice(safePage * PAGE_SIZE, (safePage + 1) * PAGE_SIZE));
</script>

<div class="page">

  <!-- ── Hero ────────────────────────────────── -->
  <div class="w-hero">
    {#if loading}
      <div class="skeleton sk-bal"></div>
      <div class="skeleton sk-unit"></div>
    {:else}
      <div class="w-coin3d"><Network3D size={132} caption={false} /></div>
      <div class="w-balance mono">{(rep?.atn_balance ?? 0).toFixed(2)}</div>
      <div class="w-unit">QUANTA</div>
      <div class="w-meta">
        <span class="w-pos">+{(rep?.atn_earned ?? 0).toFixed(2)} {t('wallet.earned')}</span>
        <span class="w-sep">·</span>
        <span>{(rep?.atn_staked ?? 0).toFixed(2)} {t('wallet.stakedShort')}</span>
      </div>
    {/if}
  </div>

  <!-- ── Actions ──────────────────────────────── -->
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
        <path d="M12 2v20M17 5H9.5a3.5 3.5 0 000 7h5a3.5 3.5 0 010 7H6"/>
      </svg>
      <span>{t('wallet.stake')}</span>
    </button>
  </div>

  <!-- ── Feedback ─────────────────────────────── -->
  {#if feedback}
    <div class="w-fb" class:w-fb-ok={feedback.ok} class:w-fb-err={!feedback.ok} role="status">
      {feedback.msg}
    </div>
  {/if}

  <!-- ── Panel Envoyer ────────────────────────── -->
  {#if panel === "send"}
    <div class="w-panel">
      {#if !preview}
        <div class="section-label">{t('wallet.send.title')}</div>
        <div class="w-form">
          <div class="w-field">
            <label for="w-to">{t('wallet.send.toLabel')}</label>
            <input id="w-to" class="input" type="text"
              placeholder={t('wallet.send.toPlaceholder')} bind:value={toAddress} />
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
          <div class="st-row"><span class="st-k">{t('wallet.send.youSend')}</span><span class="st-v mono">{fmtQ(preview.amount)} QNT</span></div>
          <div class="st-row"><span class="st-k">{t('wallet.send.recipientGets')}</span><span class="st-v mono recv">{fmtQ(preview.net)} QNT</span></div>
          <div class="st-row"><span class="st-k">{t('wallet.send.burned')} <span class="st-pill">{t('wallet.send.deflationary')}</span></span><span class="st-v mono burn">−{fmtQ(preview.burn)} QNT</span></div>
          <div class="st-row st-total"><span class="st-k">{t('wallet.send.balanceAfter')}</span><span class="st-v mono">{fmtQ(preview.balanceAfter)} QNT</span></div>
          <button class="st-confirm" onclick={confirmSend} disabled={sendBusy}>
            {sendBusy ? t('wallet.send.signing') : t('wallet.send.confirm')}
          </button>
          <button class="st-cancel" onclick={cancelPreview} disabled={sendBusy}>{t('wallet.send.cancel')}</button>
          <div class="st-note">{t('wallet.send.signNote')}</div>
        </div>
      {/if}
    </div>
  {/if}

  <!-- ── Panel Recevoir ───────────────────────── -->
  {#if panel === "receive"}
    <div class="w-panel">
      {#if myUsername}
        <Aurora radius={18}>
          <div class="rc-card">
            <div class="rc-ident"><Identicon pubkey={myPk} size={50} /></div>
            <div class="rc-pseudo">@{myUsername}</div>
            <button class="rc-code" onclick={copyCode} title={t('wallet.recv.copyCodeTitle')}>
              <span class="rc-code-lab">{t('wallet.recv.codeLabel')}</span>
              <span class="mono">{connectionCode || "····-····"}</span>
              <span class="rc-code-act">{codeCopied ? t('wallet.recv.copiedLower') : t('wallet.recv.copyLower')}</span>
            </button>
            <div class="rc-hint">{@html t('wallet.recv.hint')}</div>
          </div>
        </Aurora>
        <div class="section-label" style="margin-top:18px;">{t('wallet.recv.yourAddress')}</div>
        <div class="w-pk-box">
          <code class="w-pk mono" style="font-size:18px;font-weight:700;color:var(--color-accent);">@{myUsername}</code>
          <button class="w-copy" onclick={copyUsername}>
            {unameCopied ? t('wallet.recv.copied') : t('wallet.recv.copy')}
          </button>
        </div>
        <p class="w-hint">{t('wallet.recv.shareUsername1')} <b>@{myUsername}</b> {t('wallet.recv.shareUsername2')}</p>
        <details style="margin-top:10px;">
          <summary style="font-size:12px;color:var(--color-text-2);cursor:pointer;">{t('wallet.recv.showPublicKey')}</summary>
          <div class="w-pk-box" style="margin-top:8px;">
            <code class="w-pk mono">{myPk || t('loading')}</code>
            <button class="w-copy" onclick={copyPk} disabled={!myPk}>
              {pkCopied ? t('wallet.recv.copied') : t('wallet.recv.copy')}
            </button>
          </div>
        </details>
      {:else}
        <div class="section-label">{t('wallet.recv.yourPublicKey')}</div>
        <div class="w-pk-box">
          <code class="w-pk mono">{myPk || t('loading')}</code>
          <button class="w-copy" onclick={copyPk} disabled={!myPk}>
            {pkCopied ? t('wallet.recv.copied') : t('wallet.recv.copy')}
          </button>
        </div>
        <p class="w-hint">
          {@html t('wallet.recv.shareKeyHint')}
        </p>
      {/if}
    </div>
  {/if}

  <!-- ── Panel Staker ─────────────────────────── -->
  {#if panel === "stake"}
    <div class="w-panel">
      <div class="section-label">{t('wallet.stake.title')}</div>
      <div class="w-staked-row">
        <span>{t('wallet.stake.currentlyStaked')}</span>
        <span class="mono">{(rep?.atn_staked ?? 0).toFixed(2)} QUANTA</span>
      </div>
      <div class="w-form">
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
    </div>
  {/if}

  <!-- ── Ton argent — cartes colorées, propres au portefeuille (0 doublon Réseau) ── -->
  <div class="w-section">
    <div class="section-label">{t('wallet.yourMoney')}</div>
    {#if loading}
      <div class="w-info-list"><div class="skeleton sk-row"></div><div class="skeleton sk-row"></div></div>
    {:else}
      <div class="w-grid">
        <div class="w-cell c-green">
          <div class="w-cell-k">{t('wallet.available')}</div>
          <div class="w-cell-v mono">{avail.toFixed(2)}</div>
          <div class="w-cell-s">{t('wallet.availableSub')}</div>
        </div>
        <div class="w-cell c-violet">
          <div class="w-cell-k">{t('wallet.inStaking')}</div>
          <div class="w-cell-v mono">{(rep?.atn_staked ?? 0).toFixed(2)}</div>
          <div class="w-cell-s">{t('wallet.inStakingSub')}</div>
        </div>
        <div class="w-cell c-teal">
          <div class="w-cell-k">{t('wallet.forged')}</div>
          <div class="w-cell-v mono">+{(rep?.atn_earned ?? 0).toFixed(2)}</div>
          <div class="w-cell-s">{t('wallet.forgedSub')}</div>
        </div>
        <div class="w-cell c-amber">
          <div class="w-cell-k">{t('wallet.share')}</div>
          <div class="w-cell-v mono">{supplyShare > 0 && supplyShare < 0.01 ? "<0.01" : supplyShare.toFixed(2)}<span class="w-cell-u">%</span></div>
          <div class="w-cell-s">{t('wallet.shareSub')}</div>
        </div>
      </div>
      <div class="w-energy-foot">
        <span class="w-dot" style="background:{online ? 'var(--color-green)' : 'var(--color-text-3)'}"></span>
        <span>{online ? `${t('wallet.connected')} · ${peers} ${peers === 1 ? t('wallet.peer') : t('wallet.peers')}` : t('wallet.offline')}</span>
        <span class="w-sep">·</span>
        <span>{txCount} {t('wallet.recentTx')}</span>
        <span class="w-sep">·</span>
        <span>⚡ {(energy?.kwh_consumed ?? 0).toFixed(3)} kWh</span>
      </div>
    {/if}
  </div>

  <!-- ── Activité ─────────────────────────────── -->
  <div class="w-section">
    <div class="section-label">{t('wallet.activity')}</div>

    <!-- Filtres -->
    <div class="w-filters" role="tablist" aria-label={t('wallet.activity')}>
      <button class="w-pill" class:w-pill-on={filter === "all"}    onclick={() => setFilter("all")}    role="tab" aria-selected={filter === "all"}>{t('wallet.f.all')}</button>
      <button class="w-pill" class:w-pill-on={filter === "out"}    onclick={() => setFilter("out")}    role="tab" aria-selected={filter === "out"}>{t('wallet.f.out')}</button>
      <button class="w-pill" class:w-pill-on={filter === "in"}     onclick={() => setFilter("in")}     role="tab" aria-selected={filter === "in"}>{t('wallet.f.in')}</button>
      <button class="w-pill" class:w-pill-on={filter === "mining"} onclick={() => setFilter("mining")} role="tab" aria-selected={filter === "mining"}>{t('wallet.f.mining')}</button>
      <button class="w-pill" class:w-pill-on={filter === "burn"}   onclick={() => setFilter("burn")}   role="tab" aria-selected={filter === "burn"}>{t('wallet.f.burn')}</button>
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
            <div class="w-tx-row">
              <div class="w-tx-left">
                <span class="w-tx-label">{txLabel(tx.tx_type)}</span>
                <span class="w-tx-sub mono">
                  {inc ? `de ${shortPk(tx.from)}` : `→ ${shortPk(tx.to)}`}
                </span>
              </div>
              <div class="w-tx-right">
                <span class="w-tx-amt mono" class:tx-in={inc} class:tx-out={!inc}>
                  {inc ? "+" : "−"}{tx.amount.toFixed(2)}
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

        <!-- Pagination -->
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
    padding: 16px var(--space-5) var(--space-8); gap: var(--space-1);
  }
  .w-balance {
    font-size: 48px; font-weight: 700; letter-spacing: -0.03em; line-height: 1;
    color: var(--quanta-text-0);
  }
  .w-unit {
    font-size: 14px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase;
    color: var(--quanta-text-2); margin-top: var(--space-1);
  }
  .w-meta {
    display: flex; align-items: center; gap: var(--space-2);
    font-size: 13px; color: var(--quanta-text-2); margin-top: var(--space-3);
  }
  .w-sep { opacity: 0.5; }
  .w-pos { color: var(--quanta-accent); }

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
    color: var(--quanta-text-1);
    font-family: inherit; font-size: 12px; font-weight: 500;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
  }
  .w-btn:hover { background: var(--quanta-bg-2); border-color: var(--quanta-border-h); color: var(--quanta-text-0); }
  .w-active   { border-color: var(--quanta-accent) !important; color: var(--quanta-text-0) !important; }

  /* Feedback */
  .w-fb {
    margin: 0 var(--space-5) var(--space-4);
    padding: 10px 14px; border-radius: var(--radius-sm);
    font-size: 13px; animation: fadeIn 0.15s ease-out;
  }
  .w-fb-ok  { background: var(--quanta-accent-dim); color: var(--quanta-accent); border: 1px solid rgba(0,220,130,0.15); }
  .w-fb-err { background: rgba(255,68,68,0.06);   color: var(--quanta-negative); border: 1px solid rgba(255,68,68,0.15); }

  /* Panels */
  .w-panel {
    margin: 0 var(--space-5) var(--space-6);
    padding: var(--space-5);
    background: var(--quanta-bg-1); border: 1px solid var(--quanta-border);
    border-radius: var(--radius); animation: fadeIn 0.15s ease-out;
  }
  .w-panel .section-label { margin-bottom: var(--space-4); }

  /* Carte d'identité Aurora (panneau Recevoir) — le "moment" partageable. */
  .rc-card {
    display: flex; flex-direction: column; align-items: center; text-align: center;
    padding: 26px 22px; gap: 12px; color: #fff;
  }
  .rc-card :global(.identicon) { border: 2px solid rgba(255,255,255,0.7); box-shadow: 0 4px 14px rgba(0,0,0,0.18); }
  .rc-pseudo { font-size: 24px; font-weight: 800; letter-spacing: 0.01em; }
  .rc-code {
    display: inline-flex; align-items: center; gap: 10px;
    background: rgba(255,255,255,0.16); border: 1px solid rgba(255,255,255,0.4);
    color: #fff; border-radius: 999px; padding: 8px 16px; cursor: pointer;
    font-size: 15px; font-weight: 700; letter-spacing: 0.12em;
    backdrop-filter: blur(6px); transition: background 0.15s ease;
  }
  .rc-code:hover { background: rgba(255,255,255,0.26); }
  .rc-code-lab { font-size: 10px; font-weight: 700; letter-spacing: 0.1em; opacity: 0.8; }
  .rc-code-act { font-size: 11px; font-weight: 600; opacity: 0.85; text-transform: lowercase; letter-spacing: 0; }
  .rc-hint { font-size: 12.5px; line-height: 1.5; opacity: 0.95; max-width: 300px; }

  /* Tiroir d'envoi décodé — morphe à l'apparition (modèle Family). */
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

  /* Pièce 3D discrète dans le hero du portefeuille. */
  .w-coin3d { display: flex; justify-content: center; margin-bottom: 12px; }

  /* Vue d'ensemble — cartes d'infos utiles. */
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
  .w-cell-u { font-size: 12px; color: var(--color-text-2); font-weight: 600; margin-left: 4px; }
  .w-cell-s { font-size: 11px; color: var(--color-text-2); margin-top: 7px; line-height: 1.4; }
  .w-energy-foot {
    display: flex; flex-wrap: wrap; gap: 6px; align-items: center;
    margin-top: 12px; font-size: 11.5px; color: var(--color-text-3);
  }
  .w-dot { width: 7px; height: 7px; border-radius: 50%; display: inline-block; }
  /* Cartes colorées — chaque bloc a sa couleur. */
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
  .w-field label { font-size: 12px; font-weight: 500; color: var(--quanta-text-1); }

  .w-pk-box {
    display: flex; align-items: flex-start; gap: var(--space-3);
    padding: var(--space-4); background: var(--quanta-bg-2);
    border-radius: var(--radius-sm); margin-top: var(--space-3);
  }
  .w-pk { flex: 1; font-size: 12px; line-height: 1.7; color: var(--quanta-text-0); word-break: break-all; }
  .w-copy {
    flex-shrink: 0; padding: 8px 16px; min-height: 44px;
    background: transparent; border: 1px solid var(--quanta-border);
    border-radius: var(--radius-sm); color: var(--quanta-text-0);
    font-family: inherit; font-size: 13px; font-weight: 500; cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease;
  }
  .w-copy:hover    { background: var(--quanta-bg-3); border-color: var(--quanta-border-h); }
  .w-copy:disabled { opacity: 0.4; cursor: not-allowed; }
  .w-hint          { font-size: 12px; color: var(--quanta-text-2); margin-top: var(--space-2); }

  .w-staked-row {
    display: flex; justify-content: space-between; align-items: center;
    padding: var(--space-3) 0; border-bottom: 1px solid var(--quanta-border);
    font-size: 14px; color: var(--quanta-text-1); margin-bottom: var(--space-2);
  }
  .w-staked-row .mono { color: var(--quanta-text-0); font-weight: 500; }

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
  .w-tx-label { font-size: 14px; font-weight: 500; color: var(--quanta-text-0); }
  .w-tx-sub   { font-size: 12px; color: var(--quanta-text-2); }
  .w-tx-time  { font-size: 12px; color: var(--quanta-text-2); }
  .w-tx-amt   { font-size: 14px; font-weight: 600; }
  .tx-in      { color: var(--quanta-accent); }
  .tx-out     { color: var(--quanta-text-1); }


  /* Filtres — pills sobres, accent discret quand actif. Inspiration Apple Settings. */
  .w-filters {
    display: flex; flex-wrap: wrap; gap: 6px;
    margin-bottom: var(--space-4);
  }
  .w-pill {
    padding: 6px 12px;
    background: var(--quanta-bg-1);
    border: 1px solid var(--quanta-border);
    border-radius: 999px;
    color: var(--quanta-text-1);
    font-family: inherit;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background 200ms ease, border-color 200ms ease, color 200ms ease;
  }
  .w-pill:hover {
    background: var(--quanta-bg-2);
    color: var(--quanta-text-0);
  }
  .w-pill-on {
    background: var(--quanta-bg-3);
    border-color: var(--quanta-border-h);
    color: var(--quanta-text-0);
  }

  /* Burn ligne sous le montant — typographie discrète, même couleur que les méta. */
  .w-tx-burn {
    font-size: 11px;
    color: var(--quanta-text-2);
    font-weight: 400;
  }

  /* Pagination — minimaliste, Précédent · X/Y · Suivant. */
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
    color: var(--quanta-text-1);
    font-family: inherit;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: background 200ms ease, color 200ms ease, border-color 200ms ease;
  }
  .w-pager-btn:hover:not(:disabled) {
    background: var(--quanta-bg-2);
    border-color: var(--quanta-border-h);
    color: var(--quanta-text-0);
  }
  .w-pager-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  .w-pager-info {
    font-size: 13px;
    color: var(--quanta-text-1);
    min-width: 48px;
    text-align: center;
  }
</style>
