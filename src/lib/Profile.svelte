<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Identicon from "./Identicon.svelte";

  let pk = $state("");
  let balance = $state(0);
  let earned = $state(0);
  let staked = $state(0);
  let trustScore = $state(0);
  let uptime = $state(0);
  let energyKwh = $state(0);
  let mode = $state("Actif");
  let peers = $state(0);
  let joined = $state("");
  let copied = $state(false);

  // P2P Web Page
  let pageTitle = $state("");
  let pageContent = $state("");
  let pageExists = $state(false);
  let publishing = $state(false);
  let publishMsg = $state("");

  async function refresh() {
    try {
      const r = await invoke<any>("get_my_reputation");
      pk = r?.public_key ?? "";
      balance = r?.atn_balance ?? 0;
      earned = r?.atn_earned ?? 0;
      staked = r?.atn_staked ?? 0;
      trustScore = r?.trust_score ?? 0;
      uptime = r?.uptime_minutes ?? 0;
      energyKwh = r?.energy_kwh ?? 0;
      joined = r?.joined_at ?? "";
    } catch {}
    try {
      const s = await invoke<any>("get_node_status");
      mode = s?.mode ?? "Actif";
      peers = s?.peer_count ?? 0;
    } catch {}
    // Load existing page
    if (pk && !pageExists) {
      try {
        const p = await invoke<any>("get_page", { pk });
        if (p) {
          pageTitle = p.title ?? "";
          pageContent = p.content ?? "";
          pageExists = true;
        }
      } catch {}
    }
  }

  $effect(() => {
    refresh();
    const t = setInterval(refresh, 10000);
    return () => clearInterval(t);
  });

  async function publishPage() {
    if (!pageTitle.trim()) { publishMsg = "Titre requis"; return; }
    if (!pageContent.trim()) { publishMsg = "Contenu requis"; return; }
    publishing = true;
    publishMsg = "";
    try {
      await invoke("publish_page", { title: pageTitle, content: pageContent });
      pageExists = true;
      publishMsg = "✓ Page publiée sur le réseau !";
      setTimeout(() => publishMsg = "", 3000);
    } catch (e) {
      publishMsg = "Erreur : " + String(e);
    }
    publishing = false;
  }

  function copyPk() {
    navigator.clipboard?.writeText(pk);
    copied = true;
    setTimeout(() => copied = false, 2000);
  }

  function shortPk(k: string) {
    if (k.length < 12) return k;
    return k.slice(0, 8) + '…' + k.slice(-8);
  }

  function formatUptime(min: number) {
    const h = Math.floor(min / 60);
    const m = min % 60;
    return `${h}h${m}m`;
  }

  function formatJoined(d: string) {
    if (!d) return '—';
    try { return new Date(d).toLocaleDateString('fr-FR'); } catch { return d; }
  }

  const modeColors: Record<string, string> = { 'Actif': 'tag-green', 'Guardian': 'tag-cyan', 'Recherche': 'tag-orange' };
  const badgeLabel: Record<string, string> = { 'Actif': 'Mineur', 'Guardian': 'Guardian', 'Recherche': 'Recherche' };
</script>

<div class="page">
  <div class="page-header">
    <div>
      <div class="page-title">Profil</div>
      <div class="page-sub">Votre identité sur le réseau QUANTA</div>
    </div>
    <span class="tag {modeColors[mode] ?? 'tag-dim'}">{badgeLabel[mode] ?? mode}</span>
  </div>

  <!-- Identity card -->
  <div class="card" style="margin-bottom:12px;display:flex;gap:24px;align-items:flex-start;">
    <Identicon pubkey={pk} size={80} />
    <div style="flex:1;">
      <div style="display:flex;align-items:center;gap:10px;margin-bottom:8px;">
        <span style="font-size:16px;font-weight:700;">Nœud anonyme</span>
        <span class="tag {modeColors[mode] ?? 'tag-dim'}">{badgeLabel[mode] ?? mode}</span>
      </div>
      <div style="margin-bottom:12px;">
        <div class="stat-label" style="margin-bottom:5px;">Clé publique</div>
        <button class="copy-btn" onclick={copyPk}>
          {#if copied}✓ Copié !{:else}{shortPk(pk)}{/if}
        </button>
      </div>
      <div style="display:flex;gap:20px;flex-wrap:wrap;">
        <div>
          <div style="font-size:11px;color:var(--color-text-3);">Ancienneté</div>
          <div class="mono" style="font-size:14px;font-weight:600;margin-top:2px;color:var(--color-text-2);">{formatJoined(joined)}</div>
        </div>
        <div>
          <div style="font-size:11px;color:var(--color-text-3);">Uptime</div>
          <div class="mono" style="font-size:14px;font-weight:600;margin-top:2px;">{formatUptime(uptime)}</div>
        </div>
      </div>
    </div>
  </div>

  <!-- Public stats -->
  <div class="card" style="margin-bottom:12px;">
    <div style="display:flex;align-items:center;gap:8px;margin-bottom:16px;">
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="var(--color-text-3)" stroke-width="1.5"><circle cx="8" cy="8" r="6"/><path d="M2 8h12M8 2c-2 2-3 4-3 6s1 4 3 6M8 2c2 2 3 4 3 6s-1 4-3 6"/></svg>
      <div style="font-size:13px;font-weight:600;color:var(--color-text-2);letter-spacing:0.04em;">Ce que les autres voient</div>
    </div>
    <div class="grid-3">
      <div>
        <div class="stat-label">Solde</div>
        <div class="stat-val sm mono">{balance.toFixed(2)}</div>
        <div class="stat-sub">QNT</div>
      </div>
      <div>
        <div class="stat-label">Total miné</div>
        <div class="stat-val sm mono">{earned.toFixed(2)}</div>
        <div class="stat-sub">QNT</div>
      </div>
      <div>
        <div class="stat-label">Score confiance</div>
        <div class="stat-val sm mono" style="color:{trustScore > 80 ? 'var(--color-green)' : 'var(--color-amber)'};">{trustScore}%</div>
        <div style="margin-top:8px;">
          <div class="trust-bar-bg"><div class="trust-bar-fill" style="width:{trustScore}%;"></div></div>
        </div>
      </div>
    </div>
    <div class="divider"></div>
    <div class="grid-3">
      <div>
        <div class="stat-label">Peers</div>
        <div class="stat-val sm mono">{peers}</div>
      </div>
      <div>
        <div class="stat-label">Énergie</div>
        <div class="stat-val sm mono">{energyKwh.toFixed(1)} <span style="font-size:11px;">kWh</span></div>
      </div>
      <div>
        <div class="stat-label">Mode</div>
        <div style="margin-top:6px;"><span class="tag {modeColors[mode] ?? 'tag-dim'}">{mode}</span></div>
      </div>
    </div>
  </div>

  <!-- P2P Web Page -->
  <div class="card" style="margin-bottom:12px;">
    <div style="display:flex;align-items:center;gap:8px;margin-bottom:16px;">
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="var(--cyan)" stroke-width="1.5"><rect x="2" y="2" width="12" height="12" rx="2"/><path d="M2 5h12"/><circle cx="4" cy="3.5" r="0.5" fill="var(--cyan)"/><circle cx="6" cy="3.5" r="0.5" fill="var(--cyan)"/></svg>
      <div style="font-size:13px;font-weight:600;color:var(--cyan);letter-spacing:0.04em;">Ma page web P2P</div>
      {#if pageExists}<span class="tag tag-green">En ligne</span>{/if}
    </div>
    <div class="form-group">
      <div class="form-label">Titre</div>
      <input class="input" placeholder="Le titre de votre page…" bind:value={pageTitle} maxlength="100" />
    </div>
    <div class="form-group">
      <div class="form-label">Contenu (HTML)</div>
      <textarea class="input" rows="8" placeholder="<h1>Bienvenue</h1>&#10;<p>Ma page sur le réseau QUANTA</p>" bind:value={pageContent} style="resize:vertical;font-family:var(--font-mono);font-size:12px;"></textarea>
    </div>
    {#if publishMsg}
      <div style="font-size:12px;margin-bottom:10px;color:{publishMsg.startsWith('✓') ? 'var(--color-green)' : 'var(--color-red)'};">{publishMsg}</div>
    {/if}
    <button class="btn btn-primary" onclick={publishPage} disabled={publishing} style="width:100%;justify-content:center;">
      {publishing ? 'Publication…' : pageExists ? 'Mettre à jour' : 'Publier ma page'}
    </button>
    <div style="font-size:11px;color:var(--color-text-3);margin-top:8px;text-align:center;">
      Votre page est visible par tous via votre clé publique
    </div>
  </div>

  <!-- Private section -->
  <div class="card" style="opacity:0.65;">
    <div style="display:flex;align-items:center;gap:8px;margin-bottom:16px;">
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="var(--color-text-3)" stroke-width="1.5"><rect x="3" y="7" width="10" height="8" rx="1.5"/><path d="M5 7V5a3 3 0 016 0v2"/></svg>
      <div style="font-size:13px;font-weight:600;color:var(--color-text-2);letter-spacing:0.04em;">Infos privées</div>
    </div>
    <div style="display:flex;align-items:center;gap:12px;">
      <div style="width:40px;height:40px;border-radius:8px;background:var(--color-bg-3);display:flex;align-items:center;justify-content:center;">
        <svg width="18" height="18" viewBox="0 0 16 16" fill="none" stroke="var(--color-text-3)" stroke-width="1.5"><rect x="3" y="7" width="10" height="8" rx="1.5"/><path d="M5 7V5a3 3 0 016 0v2"/></svg>
      </div>
      <div>
        <div style="font-size:14px;font-weight:600;color:var(--color-text-2);">Clé privée</div>
        <div style="font-size:12px;color:var(--color-text-3);margin-top:3px;">Jamais affichée. Stockée localement, chiffrée. Ne la partagez jamais.</div>
      </div>
      <div style="margin-left:auto;display:flex;gap:4px;">
        {#each Array(12) as _}
          <span style="font-size:8px;color:var(--color-text-3);">●</span>
        {/each}
      </div>
    </div>
  </div>
</div>

