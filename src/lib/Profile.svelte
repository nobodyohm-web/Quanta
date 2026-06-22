<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Identicon from "./Identicon.svelte";
  import { t } from "./i18n.svelte";

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

  // Pseudo unique @handle (adresse de wallet lisible)
  let username = $state<string | null>(null);

  // ─── Sécurité & Récupération ───────────────────────────────────
  let myCode = $state("");
  let recoveryOpen = $state(false);
  let recoveryPass = $state("");
  let recoveryPhrase = $state("");
  let recoveryErr = $state("");
  let revealing = $state(false);
  let phraseCopied = $state(false);
  let backedUp = $state(false);

  function loadLocal() {
    try { backedUp = localStorage.getItem("quanta-recovery-backed-up") === "1"; } catch {}
  }

  async function revealPhrase() {
    recoveryErr = "";
    if (!recoveryPass.trim()) { recoveryErr = t('pf.recoveryErr.required'); return; }
    revealing = true;
    try {
      await invoke("unlock_identity", { password: recoveryPass });   // re-vérifie le mot de passe
      recoveryPhrase = await invoke<string>("get_recovery_key");
      recoveryPass = "";
    } catch {
      recoveryErr = t('pf.recoveryErr.invalid');
    } finally {
      revealing = false;
    }
  }

  function copyPhrase() {
    navigator.clipboard?.writeText(recoveryPhrase);
    phraseCopied = true;
    setTimeout(() => (phraseCopied = false), 2000);
  }

  function markBackedUp() {
    try { localStorage.setItem("quanta-recovery-backed-up", "1"); } catch {}
    backedUp = true;
    recoveryPhrase = "";
    recoveryOpen = false;
  }

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
    try {
      username = await invoke<string | null>("get_my_username");
    } catch {}
    try {
      myCode = await invoke<string>("get_my_connection_code");
    } catch {}
  }

  $effect(() => {
    loadLocal();
    refresh();
    const iv = setInterval(refresh, 10000);
    return () => clearInterval(iv);
  });

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
  const badgeLabel: Record<string, string> = { 'Actif': t('pf.mode.miner'), 'Guardian': t('pf.mode.guardian'), 'Recherche': t('pf.mode.research') };
</script>

<div class="page">
  <div class="page-header">
    <div>
      <div class="page-title">{t('pf.title')}</div>
      <div class="page-sub">{t('pf.subtitle')}</div>
    </div>
    <span class="tag {modeColors[mode] ?? 'tag-dim'}">{badgeLabel[mode] ?? mode}</span>
  </div>

  <!-- Identity card -->
  <div class="card" style="margin-bottom:12px;display:flex;gap:24px;align-items:flex-start;">
    <Identicon pubkey={pk} size={80} />
    <div style="flex:1;">
      <div style="display:flex;align-items:center;gap:10px;margin-bottom:8px;">
        <span style="font-size:18px;font-weight:700;color:{username ? 'var(--color-accent)' : 'var(--color-text-2)'};">{username ? '@' + username : t('pf.noUsername')}</span>
        <span class="tag {modeColors[mode] ?? 'tag-dim'}">{badgeLabel[mode] ?? mode}</span>
      </div>
      <div style="margin-bottom:12px;">
        <div class="stat-label" style="margin-bottom:5px;">{t('pf.publicKey')}</div>
        <button class="copy-btn" onclick={copyPk}>
          {#if copied}✓ {t('pf.copied')}{:else}{shortPk(pk)}{/if}
        </button>
      </div>
      <div style="display:flex;gap:20px;flex-wrap:wrap;">
        <div>
          <div style="font-size:11px;color:var(--color-text-3);">{t('pf.seniority')}</div>
          <div class="mono" style="font-size:14px;font-weight:600;margin-top:2px;color:var(--color-text-2);">{formatJoined(joined)}</div>
        </div>
        <div>
          <div style="font-size:11px;color:var(--color-text-3);">{t('pf.uptime')}</div>
          <div class="mono" style="font-size:14px;font-weight:600;margin-top:2px;">{formatUptime(uptime)}</div>
        </div>
      </div>
    </div>
  </div>

  <!-- Public stats -->
  <div class="card" style="margin-bottom:12px;">
    <div style="display:flex;align-items:center;gap:8px;margin-bottom:16px;">
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="var(--color-text-3)" stroke-width="1.5"><circle cx="8" cy="8" r="6"/><path d="M2 8h12M8 2c-2 2-3 4-3 6s1 4 3 6M8 2c2 2 3 4 3 6s-1 4-3 6"/></svg>
      <div style="font-size:13px;font-weight:600;color:var(--color-text-2);letter-spacing:0.04em;">{t('pf.whatOthersSee')}</div>
    </div>
    <div class="grid-3">
      <div>
        <div class="stat-label">{t('pf.balance')}</div>
        <div class="stat-val sm mono">{balance.toFixed(2)}</div>
        <div class="stat-sub">QNT</div>
      </div>
      <div>
        <div class="stat-label">{t('pf.totalMined')}</div>
        <div class="stat-val sm mono">{earned.toFixed(2)}</div>
        <div class="stat-sub">QNT</div>
      </div>
      <div>
        <div class="stat-label">{t('pf.trustScore')}</div>
        <div class="stat-val sm mono" style="color:{trustScore > 80 ? 'var(--color-green)' : 'var(--color-amber)'};">{trustScore}%</div>
        <div style="margin-top:8px;">
          <div class="trust-bar-bg"><div class="trust-bar-fill" style="width:{trustScore}%;"></div></div>
        </div>
      </div>
    </div>
    <div class="divider"></div>
    <div class="grid-3">
      <div>
        <div class="stat-label">{t('pf.peers')}</div>
        <div class="stat-val sm mono">{peers}</div>
      </div>
      <div>
        <div class="stat-label">{t('pf.energy')}</div>
        <div class="stat-val sm mono">{energyKwh.toFixed(1)} <span style="font-size:11px;">kWh</span></div>
      </div>
      <div>
        <div class="stat-label">{t('pf.mode')}</div>
        <div style="margin-top:6px;"><span class="tag {modeColors[mode] ?? 'tag-dim'}">{badgeLabel[mode] ?? mode}</span></div>
      </div>
    </div>
  </div>

  <!-- Ta contribution — effet IKEA : « tu as forgé ça » (honnête : le minage est une vraie contribution) -->
  <div class="card" style="margin-bottom:12px;">
    <div class="card-title">{t('pf.contribTitle')}</div>
    <p style="font-size:13px;color:var(--color-text-2);margin-bottom:16px;line-height:1.6;">
      {@html t('pf.contribText')}
    </p>
    <div class="grid-3">
      <div>
        <div class="stat-label">{t('pf.networkMaintained')}</div>
        <div class="stat-val sm mono">{formatUptime(uptime)}</div>
      </div>
      <div>
        <div class="stat-label">{t('pf.quantaForged')}</div>
        <div class="stat-val sm mono" style="color:var(--color-accent);">{earned.toFixed(2)}</div>
      </div>
      <div>
        <div class="stat-label">{t('pf.energyInvested')}</div>
        <div class="stat-val sm mono">{energyKwh.toFixed(1)} <span style="font-size:11px;">kWh</span></div>
      </div>
    </div>
  </div>

  <!-- Sécurité & Récupération -->
  <div class="card sec-card" style="margin-bottom:12px;">
    <div class="sec-head">
      <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="var(--color-accent)" stroke-width="1.5"><path d="M8 1.5l5.5 2.2V8c0 3.3-2.3 5.6-5.5 6.5C4.8 13.6 2.5 11.3 2.5 8V3.7L8 1.5z"/><path d="M5.8 8l1.6 1.6L10.4 6.6"/></svg>
      <div class="sec-title">{t('pf.secTitle')}</div>
      <span class="tag {backedUp ? 'tag-green' : 'tag-orange'}" style="margin-left:auto;">
        {backedUp ? t('pf.backedUp') : t('pf.toBackup')}
      </span>
    </div>
    <p class="sec-intro">
      {@html t('pf.secIntro')}
    </p>

    <!-- Code de connexion -->
    <div class="sec-block">
      <div class="sec-k">{t('pf.connectionCode')}</div>
      <div class="sec-row">
        <code class="mono sec-code">{myCode || '—'}</code>
        <button class="copy-btn" onclick={() => { navigator.clipboard?.writeText(myCode); }}>{t('pf.copy')}</button>
      </div>
      <div class="sec-hint">{t('pf.connectionCodeHint')}</div>
    </div>

    <!-- Phrase de récupération -->
    <div class="sec-block">
      <div class="sec-k">{t('pf.recoveryPhrase')}</div>
      {#if !recoveryOpen}
        <div class="sec-row">
          <span class="sec-hint" style="flex:1;">{t('pf.recoveryPhraseHint')}</span>
          <button class="btn btn-ghost btn-sm" onclick={() => { recoveryOpen = true; recoveryErr=''; recoveryPhrase=''; }}>{t('pf.reviewSave')}</button>
        </div>
      {:else if !recoveryPhrase}
        <div class="sec-reveal">
          <div class="sec-hint">{t('pf.confirmPassword')}</div>
          <div class="sec-row">
            <input class="input" type="password" placeholder={t('pf.password')} bind:value={recoveryPass}
              onkeydown={(e) => e.key === 'Enter' && revealPhrase()} />
            <button class="btn btn-primary btn-sm" onclick={revealPhrase} disabled={revealing}>{revealing ? '…' : t('pf.show')}</button>
            <button class="btn btn-ghost btn-sm" onclick={() => { recoveryOpen=false; recoveryPass=''; recoveryErr=''; }}>{t('pf.cancel')}</button>
          </div>
          {#if recoveryErr}<div style="font-size:12px;color:var(--color-red);">{recoveryErr}</div>{/if}
        </div>
      {:else}
        <div class="sec-phrase-box">
          <code class="mono sec-phrase">{recoveryPhrase}</code>
        </div>
        <div class="sec-warn">{t('pf.phraseWarn')}</div>
        <div class="sec-row">
          <button class="copy-btn" onclick={copyPhrase}>{phraseCopied ? t('pf.copied') : t('pf.copy')}</button>
          <button class="btn btn-primary btn-sm" onclick={markBackedUp}>{t('pf.savedSafely')}</button>
        </div>
      {/if}
    </div>

    <!-- Facteurs de récupération d'urgence (autonomie totale) -->
    <div class="sec-block" style="border-bottom:none;padding-bottom:0;">
      <div class="sec-k">{t('pf.emergencyRecovery')}</div>
      <div class="sec-hint" style="margin-bottom:10px;">
        {t('pf.emergencyRecoveryHint')}
      </div>
      <div class="sec-factor">
        <div class="sec-factor-ic">⚷</div>
        <div style="flex:1;">
          <div class="sec-factor-t">{t('pf.biometric')}</div>
          <div class="sec-hint">{t('pf.biometricHint')}</div>
        </div>
        <span class="tag tag-dim">{t('pf.soon')}</span>
      </div>
      <div class="sec-factor">
        <div class="sec-factor-ic">✉</div>
        <div style="flex:1;">
          <div class="sec-factor-t">{t('pf.emailVault')}</div>
          <div class="sec-hint">{t('pf.emailVaultHint')}</div>
        </div>
        <span class="tag tag-dim">{t('pf.soon')}</span>
      </div>
    </div>
  </div>

  <!-- Private section -->
  <div class="card" style="opacity:0.65;">
    <div style="display:flex;align-items:center;gap:8px;margin-bottom:16px;">
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="var(--color-text-3)" stroke-width="1.5"><rect x="3" y="7" width="10" height="8" rx="1.5"/><path d="M5 7V5a3 3 0 016 0v2"/></svg>
      <div style="font-size:13px;font-weight:600;color:var(--color-text-2);letter-spacing:0.04em;">{t('pf.privateInfo')}</div>
    </div>
    <div style="display:flex;align-items:center;gap:12px;">
      <div style="width:40px;height:40px;border-radius:8px;background:var(--color-bg-3);display:flex;align-items:center;justify-content:center;">
        <svg width="18" height="18" viewBox="0 0 16 16" fill="none" stroke="var(--color-text-3)" stroke-width="1.5"><rect x="3" y="7" width="10" height="8" rx="1.5"/><path d="M5 7V5a3 3 0 016 0v2"/></svg>
      </div>
      <div>
        <div style="font-size:14px;font-weight:600;color:var(--color-text-2);">{t('pf.privateKey')}</div>
        <div style="font-size:12px;color:var(--color-text-3);margin-top:3px;">{t('pf.privateKeyHint')}</div>
      </div>
      <div style="margin-left:auto;display:flex;gap:4px;">
        {#each Array(12) as _}
          <span style="font-size:8px;color:var(--color-text-3);">●</span>
        {/each}
      </div>
    </div>
  </div>
</div>

<style>
  .sec-card { border: 1px solid var(--color-border); }
  .sec-head { display: flex; align-items: center; gap: 8px; margin-bottom: 12px; }
  .sec-title { font-size: 14px; font-weight: 700; color: var(--color-text-0); }
  .sec-intro { font-size: 13px; color: var(--color-text-2); margin-bottom: 16px; line-height: 1.6; }
  .sec-block { padding: 14px 0; border-bottom: 1px solid var(--color-border); }
  .sec-k { font-size: 11px; text-transform: uppercase; letter-spacing: .05em; color: var(--color-text-3); font-weight: 600; margin-bottom: 8px; }
  .sec-row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .sec-row .input { flex: 1; min-width: 140px; }
  .sec-hint { font-size: 12px; color: var(--color-text-2); }
  .sec-code { font-size: 18px; font-weight: 700; letter-spacing: .08em; color: var(--color-text-0); }
  .sec-reveal { display: flex; flex-direction: column; gap: 8px; }
  .sec-phrase-box { background: var(--color-bg-2); border: 1px solid var(--color-border); border-radius: 10px; padding: 14px; margin-bottom: 8px; }
  .sec-phrase { font-size: 13px; line-height: 1.8; color: var(--color-text-0); word-break: break-all; user-select: all; }
  .sec-warn { font-size: 12px; color: var(--color-red); margin-bottom: 10px; }
  .sec-factor { display: flex; align-items: center; gap: 12px; padding: 10px 0; }
  .sec-factor-ic {
    width: 34px; height: 34px; border-radius: 9px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
    background: var(--color-accent-dim); color: var(--color-accent); font-size: 16px;
  }
  .sec-factor-t { font-size: 13px; font-weight: 600; color: var(--color-text-0); }
</style>

