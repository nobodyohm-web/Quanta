<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  interface Reputation {
    public_key: string; atn_balance: number; atn_earned: number;
    atn_staked: number; trust_score: number; sites_created: number;
    likes_given: number; likes_received: number; helps_validated: number;
  }
  interface VerifyResult { verified: boolean; blocks_verified: number; txs_verified: number; }
  interface NodeStatus { is_online: boolean; peer_count: number; }

  let rep = $state<Reputation | null>(null);
  let pk = $state("");
  let chain = $state<VerifyResult | null>(null);
  let node = $state<NodeStatus | null>(null);

  onMount(async () => {
    try { rep = await invoke<Reputation>("get_my_reputation"); } catch { /* */ }
    try { pk = await invoke<string>("get_public_key"); } catch { /* */ }
    try { chain = await invoke<VerifyResult>("verify_ledger"); } catch { /* */ }
    try { node = await invoke<NodeStatus>("get_node_status"); } catch { /* */ }
  });

  function shortPk(s: string): string {
    return s.length > 16 ? s.slice(0, 8) + "…" + s.slice(-8) : s;
  }

  async function copyPk() {
    await navigator.clipboard.writeText(pk);
  }
</script>

<div class="page">
  <h1 class="page-title">Mon compte</h1>

  <!-- Identity -->
  <div class="identity">
    <div class="avatar">{pk ? pk.slice(0, 2).toUpperCase() : "?"}</div>
    <div class="id-info">
      <button class="id-pk mono" onclick={copyPk} title="Copier la clé">{shortPk(pk)}</button>
      <span class="id-trust">{rep?.trust_score ?? 0} points de confiance</span>
    </div>
  </div>

  <!-- Stats grid -->
  <div class="stats-grid">
    <div class="stat-card">
      <span class="stat-val mono">{(rep?.atn_balance ?? 0).toFixed(1)}</span>
      <span class="stat-label">ATN</span>
    </div>
    <div class="stat-card">
      <span class="stat-val mono">{rep?.sites_created ?? 0}</span>
      <span class="stat-label">SITES</span>
    </div>
    <div class="stat-card">
      <span class="stat-val mono">{rep?.trust_score ?? 0}</span>
      <span class="stat-label">CONFIANCE</span>
    </div>
  </div>

  <!-- Security -->
  <div class="section-label">SÉCURITÉ</div>
  <div class="info-list">
    <div class="info-row">
      <span class="info-key">Chaîne</span>
      <span class="info-val" class:positive={chain?.verified}>
        {#if chain?.verified}✓ {chain.blocks_verified} blocs vérifiés{:else}Non vérifiée{/if}
      </span>
    </div>
    <div class="info-row">
      <span class="info-key">Réseau</span>
      <span class="info-val">
        <span class="dot" class:dot-on={node?.is_online} class:dot-off={!node?.is_online}></span>
        {node?.is_online ? `En ligne · ${node?.peer_count ?? 0} pairs` : "Hors ligne"}
      </span>
    </div>
    <div class="info-row">
      <span class="info-key">Crypto</span>
      <span class="info-val">Ed25519 + AES-256-GCM + Argon2id</span>
    </div>
  </div>

  <!-- Activity -->
  <div class="section-label">ACTIVITÉ</div>
  <div class="info-list">
    <div class="info-row">
      <span class="info-key">Sites créés</span>
      <span class="info-val mono">{rep?.sites_created ?? 0}</span>
    </div>
    <div class="info-row">
      <span class="info-key">Likes reçus</span>
      <span class="info-val mono">{rep?.likes_received ?? 0}</span>
    </div>
    <div class="info-row">
      <span class="info-key">ATN gagnés</span>
      <span class="info-val mono positive">{(rep?.atn_earned ?? 0).toFixed(2)}</span>
    </div>
    <div class="info-row">
      <span class="info-key">ATN stakés</span>
      <span class="info-val mono">{(rep?.atn_staked ?? 0).toFixed(2)}</span>
    </div>
  </div>
</div>

<style>
  .identity {
    display: flex; align-items: center; gap: 16px;
    margin-bottom: 32px;
  }
  .avatar {
    width: 48px; height: 48px;
    border-radius: 50%;
    background: var(--color-bg-2);
    display: flex; align-items: center; justify-content: center;
    font-size: 16px; font-weight: 700;
    color: var(--color-text-1);
  }
  .id-info { display: flex; flex-direction: column; gap: 2px; }
  .id-pk {
    font-size: 13px; color: var(--color-text-0);
    background: none; border: none; cursor: pointer;
    text-align: left; padding: 0;
  }
  .id-pk:hover { color: var(--color-accent); }
  .id-trust { font-size: 12px; color: var(--color-text-2); }

  .stats-grid {
    display: grid; grid-template-columns: repeat(3, 1fr);
    gap: 8px; margin-bottom: 32px;
  }
  .stat-card {
    padding: 20px 16px;
    background: var(--color-bg-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius);
    text-align: center;
  }
  .stat-val { display: block; font-size: 24px; font-weight: 700; margin-bottom: 4px; }
  .stat-label {
    font-size: 10px; font-weight: 600;
    color: var(--color-text-2);
    letter-spacing: 0.06em;
  }

  .info-list {
    margin-bottom: 32px;
  }
  .info-row {
    display: flex; align-items: center; justify-content: space-between;
    padding: 14px 0;
    border-bottom: 1px solid var(--color-border);
    font-size: 14px;
  }
  .info-row:last-child { border-bottom: none; }
  .info-key { color: var(--color-text-1); }
  .info-val {
    display: flex; align-items: center; gap: 6px;
    color: var(--color-text-0);
    font-weight: 500;
  }
  .positive { color: var(--color-green); }
</style>
