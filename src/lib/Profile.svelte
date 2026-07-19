<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Identicon from "./Identicon.svelte";
  import { t } from "./i18n.svelte";
  import { copySensitive } from "./quanta";

  let pk = $state("");
  let balance = $state(0);
  let earned = $state(0);
  let staked = $state(0);
  let trustScore = $state(0);
  let uptime = $state(0);
  let mode = $state("Active");
  let peers = $state(0);
  let joined = $state("");
  let copied = $state(false);

  // Adresse de réception publique — `qta1…` (Bech32m, checksummée), la forme
  // à partager/mettre en QR (voir `get_receive_address`). Distincte de la clé
  // publique hex ci-dessus, qui reste l'identité on-chain brute.
  let receiveAddr = $state("");
  let addrCopied = $state(false);

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

  // ─── Touch ID (déverrouillage rapide, Keychain gated par biométrie) ──
  let bioSupported = $state(false);
  let bioEnabled = $state(false);
  let bioForm = $state(false);
  let bioPass = $state("");
  let bioBusy = $state(false);
  let bioErr = $state("");
  let bioOk = $state(false);

  async function loadBioStatus() {
    try {
      const st = await invoke<{ supported: boolean; enabled: boolean }>("biometric_status");
      bioSupported = st.supported;
      bioEnabled = st.enabled;
    } catch { bioSupported = false; }
  }

  async function enableBio() {
    if (!bioPass) return;
    bioBusy = true; bioErr = "";
    try {
      await invoke("enable_biometric_unlock", { password: bioPass });
      bioPass = "";
      bioForm = false;
      bioEnabled = true;
      bioOk = true;
      setTimeout(() => (bioOk = false), 3000);
    } catch (e) {
      bioErr = String(e).replace(/^Error: /, "");
    } finally { bioBusy = false; }
  }

  async function disableBio() {
    bioBusy = true; bioErr = "";
    try {
      await invoke("disable_biometric_unlock");
      bioEnabled = false;
    } catch (e) {
      bioErr = String(e).replace(/^Error: /, "");
    } finally { bioBusy = false; }
  }

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
    // Sensitive copy: the clipboard is auto-wiped after 45 s (if unchanged) —
    // a forgotten recovery phrase in the clipboard is a real exfiltration path.
    copySensitive(recoveryPhrase).catch(() => {});
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
      joined = r?.joined_at ?? "";
    } catch {}
    try {
      const s = await invoke<any>("get_node_status");
      peers = s?.peer_count ?? 0;
    } catch {}
    try {
      // `mode` is NOT a field of NodeStatus — it lives in `get_node_mode`
      // ({ mode: "Active"|"Guardian"|"Research", watts, label }). Reading it off
      // get_node_status silently pinned the pill to the fallback forever.
      const m = await invoke<any>("get_node_mode");
      mode = m?.mode ?? "Active";
    } catch {}
    try {
      username = await invoke<string | null>("get_my_username");
    } catch {}
    try {
      myCode = await invoke<string>("get_my_connection_code");
    } catch {}
    try {
      receiveAddr = await invoke<string>("get_receive_address");
    } catch {}
  }

  $effect(() => {
    loadLocal();
    refresh();
    loadBioStatus();
    const iv = setInterval(refresh, 10000);
    return () => clearInterval(iv);
  });

  function copyPk() {
    navigator.clipboard?.writeText(pk);
    copied = true;
    setTimeout(() => copied = false, 2000);
  }

  function copyReceiveAddr() {
    navigator.clipboard?.writeText(receiveAddr);
    addrCopied = true;
    setTimeout(() => addrCopied = false, 2000);
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

  const modeColors: Record<string, string> = { 'Active': 'tag-green', 'Guardian': 'tag-cyan', 'Research': 'tag-orange' };
  const badgeLabel: Record<string, string> = { 'Active': t('pf.mode.miner'), 'Guardian': t('pf.mode.guardian'), 'Research': t('pf.mode.research') };
</script>

<div class="page">
  <div class="page-header">
    <div>
      <div class="page-title">{t('pf.title')}</div>
      <div class="page-sub">{t('pf.subtitle')}</div>
    </div>
    <span class="mode-pill" data-tone={modeColors[mode] ?? 'tag-dim'}>{badgeLabel[mode] ?? mode}</span>
  </div>

  <div class="cards">
    <!-- Identité — le seul moment de marque (@pseudo + adresse ML-DSA) -->
    <div class="card id-hero">
      <Identicon pubkey={pk} size={72} />
      <div class="id-main">
        <div class="id-handle" class:unnamed={!username}>{username ? '@' + username : t('pf.noUsername')}</div>
        <div class="addr-chips">
          <button class="addr-chip" onclick={copyReceiveAddr} disabled={!receiveAddr} title={t('pf.receiveAddressHint')}>
            <span class="addr-lbl">{t('pf.receiveAddress')}</span>
            <span class="addr-val mono">{#if addrCopied}✓ {t('pf.copied')}{:else}{receiveAddr ? shortPk(receiveAddr) : '—'}{/if}</span>
          </button>
          <button class="addr-chip" onclick={copyPk}>
            <span class="addr-lbl">{t('pf.publicKey')}</span>
            <span class="addr-val mono">{#if copied}✓ {t('pf.copied')}{:else}{shortPk(pk)}{/if}</span>
          </button>
        </div>
      </div>
      <div class="id-meta">
        <div>
          <div class="section-label">{t('pf.seniority')}</div>
          <div class="meta-v mono">{formatJoined(joined)}</div>
        </div>
        <div>
          <div class="section-label">{t('pf.uptime')}</div>
          <div class="meta-v mono">{formatUptime(uptime)}</div>
        </div>
      </div>
    </div>

    <!-- Public — ce que voient les autres -->
    <div class="card">
      <div class="card-title ct-row">
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="var(--color-text-3)" stroke-width="1.5"><circle cx="8" cy="8" r="6"/><path d="M2 8h12M8 2c-2 2-3 4-3 6s1 4 3 6M8 2c2 2 3 4 3 6s-1 4-3 6"/></svg>
        <span>{t('pf.whatOthersSee')}</span>
      </div>
      <div class="stat-grid">
        <div class="stat">
          <div class="section-label">{t('pf.balance')}</div>
          <div class="fig fig-md">{balance.toFixed(2)}</div>
          <div class="stat-unit">QNT</div>
        </div>
        <div class="stat">
          <div class="section-label">{t('pf.totalMined')}</div>
          <div class="fig fig-md">{earned.toFixed(2)}</div>
          <div class="stat-unit">QNT</div>
        </div>
        <div class="stat">
          <div class="section-label">{t('pf.trustScore')}</div>
          <div class="fig fig-md">{trustScore}<span class="fig-suffix">%</span></div>
          <div class="trust-bar-bg" style="margin-top:12px;"><div class="trust-bar-fill" style="width:{trustScore}%;"></div></div>
        </div>
      </div>
      <div class="divider"></div>
      <div class="stat-grid stat-grid-2">
        <div class="stat">
          <div class="section-label">{t('pf.peers')}</div>
          <div class="fig fig-md">{peers}</div>
        </div>
        <div class="stat">
          <div class="section-label">{t('pf.mode')}</div>
          <div><span class="mode-pill" data-tone={modeColors[mode] ?? 'tag-dim'}>{badgeLabel[mode] ?? mode}</span></div>
        </div>
      </div>
    </div>

    <!-- Ta contribution — « tu as forgé ça » (le minage est une vraie contribution) -->
    <div class="card">
      <div class="card-title">{t('pf.contribTitle')}</div>
      <p class="contrib-text">{@html t('pf.contribText')}</p>
      <div class="stat-grid stat-grid-2">
        <div class="stat">
          <div class="section-label">{t('pf.networkMaintained')}</div>
          <div class="fig fig-md">{formatUptime(uptime)}</div>
        </div>
        <div class="stat">
          <div class="section-label">{t('pf.quantaForged')}</div>
          <div class="fig fig-md accent">{earned.toFixed(2)}</div>
          <div class="stat-unit">QNT</div>
        </div>
      </div>
    </div>

    <!-- Sécurité & Récupération -->
    <div class="card">
      <div class="sec-head">
        <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="var(--color-accent)" stroke-width="1.5"><path d="M8 1.5l5.5 2.2V8c0 3.3-2.3 5.6-5.5 6.5C4.8 13.6 2.5 11.3 2.5 8V3.7L8 1.5z"/><path d="M5.8 8l1.6 1.6L10.4 6.6"/></svg>
        <div class="sec-title">{t('pf.secTitle')}</div>
        <span class="chip" class:chip-attn={!backedUp}>{backedUp ? t('pf.backedUp') : t('pf.toBackup')}</span>
      </div>
      <p class="sec-intro">{@html t('pf.secIntro')}</p>

      <!-- Code de connexion -->
      <div class="sec-block">
        <div class="section-label">{t('pf.connectionCode')}</div>
        <div class="sec-row">
          <code class="mono sec-code">{myCode || '—'}</code>
          <button class="copy-btn" onclick={() => { navigator.clipboard?.writeText(myCode); }}>{t('pf.copy')}</button>
        </div>
        <div class="sec-hint">{t('pf.connectionCodeHint')}</div>
      </div>

      <!-- Phrase de récupération -->
      <div class="sec-block">
        <div class="section-label">{t('pf.recoveryPhrase')}</div>
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
            {#if recoveryErr}<div class="sec-err">{recoveryErr}</div>{/if}
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

      <!-- Facteurs de récupération d'urgence -->
      <div class="sec-block sec-block-last">
        <div class="section-label">{t('pf.emergencyRecovery')}</div>
        <div class="sec-hint" style="margin-bottom:14px;">{t('pf.emergencyRecoveryHint')}</div>
        <div class="sec-factor">
          <div class="sec-factor-ic">⚷</div>
          <div class="sec-factor-body">
            <div class="sec-factor-t">{t('pf.biometric')}</div>
            <div class="sec-hint">{bioSupported ? t('pf.biometricHint') : t('pf.bioUnavailable')}</div>
          </div>
          {#if !bioSupported}
            <span class="chip">—</span>
          {:else if bioEnabled}
            <button class="btn btn-ghost btn-sm" onclick={disableBio} disabled={bioBusy}>
              {bioBusy ? "…" : t('pf.bioDisable')}
            </button>
          {:else if !bioForm}
            <button class="btn btn-primary btn-sm" onclick={() => { bioForm = true; bioErr = ""; }}>
              {t('pf.bioEnable')}
            </button>
          {/if}
        </div>
        {#if bioForm && !bioEnabled}
          <div class="bio-form">
            <div class="sec-hint" style="margin-bottom:8px;">{t('pf.bioConfirm')}</div>
            <div class="bio-form-row">
              <input class="input" type="password" placeholder={t('pf.password')}
                bind:value={bioPass}
                onkeydown={(e) => e.key === 'Enter' && enableBio()} />
              <button class="btn btn-primary btn-sm" onclick={enableBio} disabled={bioBusy || !bioPass}>
                {bioBusy ? "…" : t('pf.bioActivate')}
              </button>
              <button class="btn btn-ghost btn-sm" onclick={() => { bioForm = false; bioPass = ""; bioErr = ""; }}>
                {t('pf.cancel')}
              </button>
            </div>
            {#if bioErr}<div class="sec-err">{bioErr}</div>{/if}
            {#if bioOk}<div class="sec-ok">✓ {t('pf.bioEnabled')}</div>{/if}
          </div>
        {/if}
        {#if bioOk && bioEnabled}
          <div class="sec-ok" style="margin:-4px 0 12px 46px;">✓ {t('pf.bioEnabled')}</div>
        {/if}
        <div class="sec-factor">
          <div class="sec-factor-ic">✉</div>
          <div class="sec-factor-body">
            <div class="sec-factor-t">{t('pf.emailVault')}</div>
            <div class="sec-hint">{t('pf.emailVaultHint')}</div>
          </div>
          <span class="chip">{t('pf.soon')}</span>
        </div>
      </div>
    </div>

    <!-- Clé privée (jamais montrée) -->
    <div class="card private-card">
      <div class="card-title ct-row">
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="var(--color-text-3)" stroke-width="1.5"><rect x="3" y="7" width="10" height="8" rx="1.5"/><path d="M5 7V5a3 3 0 016 0v2"/></svg>
        <span>{t('pf.privateInfo')}</span>
      </div>
      <div class="private-row">
        <div class="private-ic">
          <svg width="18" height="18" viewBox="0 0 16 16" fill="none" stroke="var(--color-text-3)" stroke-width="1.5"><rect x="3" y="7" width="10" height="8" rx="1.5"/><path d="M5 7V5a3 3 0 016 0v2"/></svg>
        </div>
        <div>
          <div class="private-t">{t('pf.privateKey')}</div>
          <div class="private-hint">{t('pf.privateKeyHint')}</div>
        </div>
        <div class="private-dots">
          {#each Array(12) as _}
            <span></span>
          {/each}
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .cards { display: flex; flex-direction: column; gap: var(--space-4); }

  /* ── Mode — pill neutre (aucun accent de couleur sur le chrome) ── */
  .mode-pill {
    display: inline-flex; align-items: center;
    padding: 4px 11px; border-radius: 100px;
    background: var(--color-bg-1); border: 1px solid var(--color-border);
    font-size: 11px; font-weight: 600; letter-spacing: 0.06em;
    text-transform: uppercase; color: var(--color-text-2);
    white-space: nowrap;
  }

  /* ── Hero identité ── */
  .id-hero {
    display: flex; align-items: center; gap: var(--space-6);
    padding: 26px 28px; flex-wrap: wrap;
  }
  .id-main { flex: 1; min-width: 0; }
  .id-handle {
    font-family: var(--font-display);
    font-size: 26px; font-weight: 700; letter-spacing: -0.03em;
    color: var(--color-text-0); margin-bottom: 14px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .id-handle.unnamed { color: var(--color-text-3); font-weight: 600; }
  .addr-chips { display: flex; flex-direction: column; gap: 8px; align-items: flex-start; max-width: 100%; }
  .addr-chip {
    display: inline-flex; align-items: baseline; gap: 10px;
    padding: 8px 13px; border-radius: var(--radius-sm);
    background: var(--color-bg-1); border: 1px solid var(--color-border);
    cursor: pointer; max-width: 100%;
    font-family: inherit;
    transition: border-color 0.15s, background 0.15s;
  }
  .addr-chip:hover { border-color: var(--color-border-hover); background: var(--color-bg-2); }
  .addr-chip:disabled { cursor: default; opacity: 0.6; }
  .addr-lbl {
    font-size: 10px; font-weight: 600; letter-spacing: 0.08em;
    text-transform: uppercase; color: var(--color-text-3); flex-shrink: 0;
  }
  .addr-val { font-size: 13px; color: var(--color-text-1); letter-spacing: 0.02em; overflow: hidden; text-overflow: ellipsis; }
  .id-meta { display: flex; gap: var(--space-8); flex-shrink: 0; }
  .meta-v {
    font-family: var(--font-display); font-size: 15px; font-weight: 600;
    color: var(--color-text-1);
    font-variant-numeric: tabular-nums lining-nums;
  }

  /* ── Titre de carte avec icône ── */
  .ct-row { display: flex; align-items: center; gap: 7px; }
  .ct-row svg { flex-shrink: 0; }

  /* ── Grille de stats — chiffres confiants, tabulaires, beaucoup de vide ── */
  .stat-grid {
    display: grid; grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 28px 24px;
  }
  .stat-grid-2 { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .fig {
    font-family: var(--font-display);
    font-variant-numeric: tabular-nums lining-nums;
    font-feature-settings: 'tnum', 'lnum';
    color: var(--color-text-0); line-height: 1; letter-spacing: -0.02em;
  }
  .fig-md { font-size: 26px; font-weight: 600; }
  .fig.accent { color: var(--color-accent); }
  .fig-suffix { font-size: 0.55em; font-weight: 500; color: var(--color-text-3); margin-left: 2px; letter-spacing: 0; }
  .stat-unit {
    font-size: 11px; font-weight: 500; letter-spacing: 0.06em;
    text-transform: uppercase; color: var(--color-text-3); margin-top: 8px;
  }
  .stat .section-label { margin-bottom: 9px; }

  /* ── Contribution ── */
  .contrib-text { font-size: 13.5px; color: var(--color-text-2); margin-bottom: 22px; line-height: 1.65; }

  /* ── Sécurité ── */
  .sec-head { display: flex; align-items: center; gap: 9px; margin-bottom: 12px; }
  .sec-head svg { flex-shrink: 0; }
  .sec-title { font-size: 15px; font-weight: 700; letter-spacing: -0.01em; color: var(--color-text-0); }
  .sec-head .chip { margin-left: auto; }
  .sec-intro { font-size: 13.5px; color: var(--color-text-2); margin-bottom: 18px; line-height: 1.65; }
  .sec-block { padding: 18px 0; border-bottom: 1px solid var(--color-border); }
  .sec-block-last { border-bottom: none; padding-bottom: 0; }
  .sec-row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .sec-row .input { flex: 1; min-width: 150px; }
  .sec-hint { font-size: 12px; color: var(--color-text-2); line-height: 1.5; }
  .sec-code {
    font-size: 18px; font-weight: 700; letter-spacing: 0.1em;
    color: var(--color-text-0);
  }
  .sec-reveal { display: flex; flex-direction: column; gap: 8px; }
  .sec-phrase-box {
    background: var(--color-bg-1); border: 1px solid var(--color-border);
    border-radius: var(--radius); padding: 16px; margin-bottom: 10px;
  }
  .sec-phrase { font-size: 13.5px; line-height: 1.9; color: var(--color-text-0); word-break: break-all; user-select: all; }
  /* Rouge sémantique — avertissement réellement critique (perte de fonds) */
  .sec-warn { font-size: 12px; color: var(--color-red); margin-bottom: 12px; line-height: 1.5; }
  .sec-err { font-size: 12px; color: var(--color-red); margin-top: 8px; }
  /* Confirmation positive — teal (le seul accent), pas de vert décoratif */
  .sec-ok { font-size: 12px; color: var(--color-accent); font-weight: 600; margin-top: 8px; }
  .bio-form { margin: 0 0 14px 46px; }
  .bio-form-row { display: flex; gap: 8px; }
  .bio-form-row .input { flex: 1; }
  .sec-factor { display: flex; align-items: center; gap: 14px; padding: 11px 0; }
  .sec-factor-body { flex: 1; min-width: 0; }
  .sec-factor-ic {
    width: 34px; height: 34px; border-radius: var(--radius-sm); flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
    background: var(--color-accent-dim); color: var(--color-accent); font-size: 16px;
  }
  .sec-factor-t { font-size: 13.5px; font-weight: 600; color: var(--color-text-0); margin-bottom: 2px; }

  /* ── Chip neutre (statut, « bientôt », sauvegarde) ── */
  .chip {
    display: inline-flex; align-items: center;
    padding: 3px 10px; border-radius: 100px;
    background: var(--color-bg-1); border: 1px solid var(--color-border);
    font-size: 11px; font-weight: 600; letter-spacing: 0.05em;
    text-transform: uppercase; color: var(--color-text-2); white-space: nowrap;
  }
  .chip-attn { color: var(--color-text-0); border-color: var(--color-border-hover); }

  /* ── Clé privée (jamais révélée) ── */
  .private-card { opacity: 0.7; }
  .private-row { display: flex; align-items: center; gap: 14px; }
  .private-ic {
    width: 40px; height: 40px; border-radius: var(--radius-sm); flex-shrink: 0;
    background: var(--color-bg-2); display: flex; align-items: center; justify-content: center;
  }
  .private-t { font-size: 14px; font-weight: 600; color: var(--color-text-1); }
  .private-hint { font-size: 12px; color: var(--color-text-3); margin-top: 3px; }
  .private-dots { margin-left: auto; display: flex; gap: 5px; }
  .private-dots span {
    width: 5px; height: 5px; border-radius: 50%; background: var(--color-text-3); opacity: 0.5;
  }

  @media (max-width: 620px) {
    .stat-grid { grid-template-columns: 1fr 1fr; }
    .id-meta { gap: var(--space-6); }
  }
</style>
