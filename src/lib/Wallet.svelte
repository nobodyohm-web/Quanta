<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

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
    atn_floor_eur: number;
  }

  let rep     = $state<Reputation | null>(null);
  let txs     = $state<LedgerTx[]>([]);
  let energy  = $state<EnergyStats | null>(null);
  let myPk    = $state("");
  let loading = $state(true);

  let panel = $state<"send" | "receive" | "stake" | null>(null);

  let toAddress  = $state("");
  let sendAmount = $state("");
  let sendBusy   = $state(false);

  let stakeAmount = $state("");
  let stakeBusy   = $state(false);

  let feedback = $state<{ ok: boolean; msg: string } | null>(null);
  let pkCopied = $state(false);

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
    loading = false;
  }

  function togglePanel(p: "send" | "receive" | "stake") {
    panel = panel === p ? null : p;
    feedback = null;
  }

  async function send() {
    const amt = parseFloat(sendAmount);
    if (!toAddress.trim() || !isFinite(amt) || amt <= 0) {
      feedback = { ok: false, msg: "Adresse et montant requis" };
      return;
    }
    sendBusy = true; feedback = null;
    try {
      await invoke("ledger_transfer", { to: toAddress.trim(), amount: amt });
      feedback = { ok: true, msg: `${amt.toFixed(2)} ATN envoyés` };
      toAddress = ""; sendAmount = "";
      await refresh();
    } catch (e: unknown) {
      feedback = { ok: false, msg: e instanceof Error ? e.message : String(e) };
    } finally { sendBusy = false; }
  }

  async function stake() {
    const amt = parseFloat(stakeAmount);
    if (!isFinite(amt) || amt <= 0) {
      feedback = { ok: false, msg: "Montant invalide" };
      return;
    }
    stakeBusy = true; feedback = null;
    try {
      await invoke("stake_atn", { amount: amt });
      feedback = { ok: true, msg: `${amt.toFixed(2)} ATN stakés` };
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

  function timeAgo(ts: string): string {
    const diff = Math.floor((Date.now() - new Date(ts).getTime()) / 1000);
    if (!isFinite(diff) || diff < 0) return "";
    if (diff < 60) return "à l'instant";
    if (diff < 3600) return Math.floor(diff / 60) + " min";
    if (diff < 86400) return Math.floor(diff / 3600) + " h";
    return Math.floor(diff / 86400) + " j";
  }

  function shortPk(s: string): string {
    return s.length > 14 ? s.slice(0, 6) + "…" + s.slice(-4) : s;
  }

  function txLabel(type: string): string {
    const m: Record<string, string> = {
      Mining: "Mining", Transfer: "Transfert", Like: "Like",
      Help: "Aide", Create: "Création", View: "Vue",
      Stake: "Stake", Unstake: "Unstake",
    };
    return m[type] ?? type;
  }

  function isIncoming(tx: LedgerTx): boolean {
    return tx.to === myPk && tx.from !== myPk;
  }
</script>

<div class="page">

  <!-- ── Hero ────────────────────────────────── -->
  <div class="w-hero">
    {#if loading}
      <div class="skeleton sk-bal"></div>
      <div class="skeleton sk-unit"></div>
    {:else}
      <div class="w-balance mono">{(rep?.atn_balance ?? 0).toFixed(2)}</div>
      <div class="w-unit">ATN</div>
      <div class="w-meta">
        <span class="w-pos">+{(rep?.atn_earned ?? 0).toFixed(2)} gagnés</span>
        <span class="w-sep">·</span>
        <span>{(rep?.atn_staked ?? 0).toFixed(2)} stakés</span>
      </div>
      {#if energy && energy.atn_floor_eur > 0}
        <div class="w-floor">
          ≈ {((rep?.atn_balance ?? 0) * energy.atn_floor_eur).toFixed(3)} EUR
        </div>
      {/if}
    {/if}
  </div>

  <!-- ── Actions ──────────────────────────────── -->
  <div class="w-actions">
    <button class="w-btn" class:w-active={panel === "send"} onclick={() => togglePanel("send")}>
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M22 2L11 13M22 2l-7 20-4-9-9-4 20-7z"/>
      </svg>
      <span>Envoyer</span>
    </button>
    <button class="w-btn" class:w-active={panel === "receive"} onclick={() => togglePanel("receive")}>
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M7 10l5 5 5-5M12 15V3"/>
      </svg>
      <span>Recevoir</span>
    </button>
    <button class="w-btn" class:w-active={panel === "stake"} onclick={() => togglePanel("stake")}>
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M12 2v20M17 5H9.5a3.5 3.5 0 000 7h5a3.5 3.5 0 010 7H6"/>
      </svg>
      <span>Staker</span>
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
      <div class="section-label">ENVOYER</div>
      <div class="w-form">
        <div class="w-field">
          <label for="w-to">Clé publique du destinataire</label>
          <input id="w-to" class="input" type="text"
            placeholder="64 caractères hex…" bind:value={toAddress} />
        </div>
        <div class="w-field">
          <label for="w-amt">Montant (ATN)</label>
          <input id="w-amt" class="input" type="number"
            min="0.01" step="0.01" placeholder="0.00" bind:value={sendAmount}
            onkeydown={(e) => e.key === "Enter" && send()} />
        </div>
        <button class="btn btn-primary" onclick={send} disabled={sendBusy}>
          {sendBusy ? "Signature…" : "Confirmer l'envoi"}
        </button>
      </div>
    </div>
  {/if}

  <!-- ── Panel Recevoir ───────────────────────── -->
  {#if panel === "receive"}
    <div class="w-panel">
      <div class="section-label">VOTRE CLÉ PUBLIQUE</div>
      <div class="w-pk-box">
        <code class="w-pk mono">{myPk || "Chargement…"}</code>
        <button class="w-copy" onclick={copyPk} disabled={!myPk}>
          {pkCopied ? "Copié !" : "Copier"}
        </button>
      </div>
      <p class="w-hint">Partagez cette clé pour recevoir des ATN.</p>
    </div>
  {/if}

  <!-- ── Panel Staker ─────────────────────────── -->
  {#if panel === "stake"}
    <div class="w-panel">
      <div class="section-label">STAKING</div>
      <div class="w-staked-row">
        <span>Actuellement stakés</span>
        <span class="mono">{(rep?.atn_staked ?? 0).toFixed(2)} ATN</span>
      </div>
      <div class="w-form">
        <div class="w-field">
          <label for="w-stake-amt">Montant à verrouiller (ATN)</label>
          <input id="w-stake-amt" class="input" type="number"
            min="0.01" step="0.01" placeholder="0.00" bind:value={stakeAmount}
            onkeydown={(e) => e.key === "Enter" && stake()} />
        </div>
        <button class="btn btn-primary" onclick={stake} disabled={stakeBusy}>
          {stakeBusy ? "Staking…" : "Staker"}
        </button>
      </div>
    </div>
  {/if}

  <!-- ── Énergie ──────────────────────────────── -->
  <div class="w-section">
    <div class="section-label">ÉNERGIE</div>
    <div class="w-info-list">
      {#if loading}
        <div class="skeleton sk-row"></div>
        <div class="skeleton sk-row"></div>
        <div class="skeleton sk-row"></div>
      {:else}
        <div class="w-info-row">
          <span>Consommation</span>
          <span class="mono">{(energy?.kwh_consumed ?? 0).toFixed(3)} kWh</span>
        </div>
        <div class="w-info-row">
          <span>ATN minés</span>
          <span class="mono w-pos">+{(energy?.atn_mined ?? rep?.atn_earned ?? 0).toFixed(3)}</span>
        </div>
        <div class="w-info-row">
          <span>Uptime</span>
          <span class="mono">{energy?.uptime_minutes ?? 0} min</span>
        </div>
        {#if (energy?.atn_floor_eur ?? 0) > 0}
          <div class="w-info-row">
            <span>Plancher 1 ATN</span>
            <span class="mono">{(energy?.atn_floor_eur ?? 0).toFixed(5)} EUR</span>
          </div>
        {/if}
      {/if}
    </div>
  </div>

  <!-- ── Activité ─────────────────────────────── -->
  <div class="w-section">
    <div class="section-label">ACTIVITÉ</div>
    <div class="w-tx-list">
      {#if loading}
        <div class="skeleton sk-row"></div>
        <div class="skeleton sk-row"></div>
        <div class="skeleton sk-row"></div>
      {:else if txs.length === 0}
        <p class="w-empty">Le mining démarre automatiquement.</p>
      {:else}
        {#each txs.slice(0, 20) as tx (tx.id)}
          {@const inc = isIncoming(tx)}
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
              <span class="w-tx-time">{timeAgo(tx.timestamp)}</span>
            </div>
          </div>
        {/each}
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
    color: var(--sova-text-0);
  }
  .w-unit {
    font-size: 14px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase;
    color: var(--sova-text-2); margin-top: var(--space-1);
  }
  .w-meta {
    display: flex; align-items: center; gap: var(--space-2);
    font-size: 13px; color: var(--sova-text-2); margin-top: var(--space-3);
  }
  .w-sep { opacity: 0.5; }
  .w-pos { color: var(--sova-accent); }
  .w-floor { font-size: 12px; color: var(--sova-text-2); margin-top: var(--space-1); }

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
    background: var(--sova-bg-elevated);
    border: 1px solid var(--sova-border); border-radius: var(--radius);
    color: var(--sova-text-1);
    font-family: inherit; font-size: 12px; font-weight: 500;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
  }
  .w-btn:hover { background: var(--sova-bg-2); border-color: var(--sova-border-h); color: var(--sova-text-0); }
  .w-active   { border-color: var(--sova-accent) !important; color: var(--sova-text-0) !important; }

  /* Feedback */
  .w-fb {
    margin: 0 var(--space-5) var(--space-4);
    padding: 10px 14px; border-radius: var(--radius-sm);
    font-size: 13px; animation: fadeIn 0.15s ease-out;
  }
  .w-fb-ok  { background: var(--sova-accent-dim); color: var(--sova-accent); border: 1px solid rgba(0,220,130,0.15); }
  .w-fb-err { background: rgba(255,68,68,0.06);   color: var(--sova-negative); border: 1px solid rgba(255,68,68,0.15); }

  /* Panels */
  .w-panel {
    margin: 0 var(--space-5) var(--space-6);
    padding: var(--space-5);
    background: var(--sova-bg-1); border: 1px solid var(--sova-border);
    border-radius: var(--radius); animation: fadeIn 0.15s ease-out;
  }
  .w-panel .section-label { margin-bottom: var(--space-4); }

  .w-form  { display: flex; flex-direction: column; gap: var(--space-4); }
  .w-field { display: flex; flex-direction: column; gap: 6px; }
  .w-field label { font-size: 12px; font-weight: 500; color: var(--sova-text-1); }

  .w-pk-box {
    display: flex; align-items: flex-start; gap: var(--space-3);
    padding: var(--space-4); background: var(--sova-bg-2);
    border-radius: var(--radius-sm); margin-top: var(--space-3);
  }
  .w-pk { flex: 1; font-size: 12px; line-height: 1.7; color: var(--sova-text-0); word-break: break-all; }
  .w-copy {
    flex-shrink: 0; padding: 8px 16px; min-height: 44px;
    background: transparent; border: 1px solid var(--sova-border);
    border-radius: var(--radius-sm); color: var(--sova-text-0);
    font-family: inherit; font-size: 13px; font-weight: 500; cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease;
  }
  .w-copy:hover    { background: var(--sova-bg-3); border-color: var(--sova-border-h); }
  .w-copy:disabled { opacity: 0.4; cursor: not-allowed; }
  .w-hint          { font-size: 12px; color: var(--sova-text-2); margin-top: var(--space-2); }

  .w-staked-row {
    display: flex; justify-content: space-between; align-items: center;
    padding: var(--space-3) 0; border-bottom: 1px solid var(--sova-border);
    font-size: 14px; color: var(--sova-text-1); margin-bottom: var(--space-2);
  }
  .w-staked-row .mono { color: var(--sova-text-0); font-weight: 500; }

  /* Sections */
  .w-section { padding: 0 var(--space-5); margin-bottom: var(--space-8); }

  .w-info-row {
    display: flex; justify-content: space-between; align-items: center;
    padding: 14px 0; border-bottom: 1px solid var(--sova-border); font-size: 14px;
  }
  .w-info-row:last-child { border-bottom: none; }
  .w-info-row > span:first-child { color: var(--sova-text-1); }
  .w-info-row .mono { color: var(--sova-text-0); font-weight: 500; }

  /* Transactions */
  .w-tx-row {
    display: flex; justify-content: space-between; align-items: center;
    padding: 14px 0; border-bottom: 1px solid var(--sova-border);
  }
  .w-tx-row:last-child { border-bottom: none; }
  .w-tx-left  { display: flex; flex-direction: column; gap: 2px; }
  .w-tx-right { display: flex; flex-direction: column; align-items: flex-end; gap: 2px; }
  .w-tx-label { font-size: 14px; font-weight: 500; color: var(--sova-text-0); }
  .w-tx-sub   { font-size: 12px; color: var(--sova-text-2); }
  .w-tx-time  { font-size: 12px; color: var(--sova-text-2); }
  .w-tx-amt   { font-size: 14px; font-weight: 600; }
  .tx-in      { color: var(--sova-accent); }
  .tx-out     { color: var(--sova-text-1); }

  .w-empty { padding: var(--space-6) 0; font-size: 13px; color: var(--sova-text-2); text-align: center; }
</style>
