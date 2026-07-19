<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { check } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { getPrefs, setPrefs, applyTheme, type Prefs } from "./prefs";
  import TrustCharter from "./TrustCharter.svelte";
  import LanguageSelect from "./LanguageSelect.svelte";
  import { t } from "./i18n.svelte";
  import { FEEDBACK_COPY_MS, FEEDBACK_OK_MS } from "./quanta";

  let prefs = $state<Prefs>(getPrefs());
  let nodeTicket = $state<string>("");
  let ticketCopied = $state(false);
  let economy = $state<any>(null);
  let peerInput = $state("");
  let connectStatus = $state<"idle" | "ok" | "error">("idle");
  let connectMsg = $state("");

  $effect(() => {
    setPrefs(prefs);
    applyTheme(prefs.theme);
  });

  $effect(() => {
    refresh();
  });

  async function refresh() {
    try { nodeTicket = await invoke<string>("get_node_ticket"); } catch { nodeTicket = t('set.offline'); }
    try { economy = await invoke("get_economy_stats"); } catch {}
  }

  async function copyTicket() {
    if (!nodeTicket || nodeTicket === t('set.offline')) return;
    await navigator.clipboard.writeText(nodeTicket);
    ticketCopied = true;
    setTimeout(() => ticketCopied = false, FEEDBACK_COPY_MS);
  }

  async function connectPeer() {
    if (!peerInput.trim()) return;
    connectStatus = "idle";
    connectMsg = "";
    try {
      await invoke("connect_peer", { peerId: peerInput.trim() });
      connectStatus = "ok";
      connectMsg = t('set.connected');
      peerInput = "";
      setTimeout(() => { connectStatus = "idle"; connectMsg = ""; }, FEEDBACK_OK_MS);
    } catch (e: any) {
      connectStatus = "error";
      connectMsg = String(e);
    }
  }

  function setTheme(t: "light" | "dark" | "auto") {
    prefs = { ...prefs, theme: t };
  }

  function setLockMinutes(m: number) {
    prefs = { ...prefs, lockMinutes: m };
  }

  // ── OTA Update ──
  let updateStatus = $state<"idle" | "checking" | "downloading" | "ready" | "latest" | "error">("idle");
  let updateVersion = $state("");
  let updateError = $state("");
  let downloadProgress = $state(0);

  async function checkForUpdate() {
    updateStatus = "checking";
    updateError = "";
    try {
      const update = await check();
      if (update) {
        updateVersion = update.version;
        updateStatus = "downloading";
        let totalBytes = 0;
        let downloadedBytes = 0;
        await update.downloadAndInstall((event) => {
          if ('contentLength' in event && event.contentLength) {
            totalBytes = event.contentLength as number;
          }
          if ('chunkLength' in event && event.chunkLength) {
            downloadedBytes += event.chunkLength as number;
            if (totalBytes > 0) {
              downloadProgress = Math.round((downloadedBytes / totalBytes) * 100);
            }
          }
        });
        updateStatus = "ready";
      } else {
        updateStatus = "latest";
        setTimeout(() => updateStatus = "idle", FEEDBACK_OK_MS);
      }
    } catch (e) {
      updateError = String(e);
      updateStatus = "error";
    }
  }

  async function doRelaunch() {
    await relaunch();
  }

</script>

<div class="page settings-page">
  <div class="page-header">
    <div>
      <h1 class="page-title">{t('set.title')}</h1>
      <p class="page-sub">{t('set.pageSub')}</p>
    </div>
  </div>

  <!-- Langue -->
  <section class="group">
    <div class="group-head">
      <h2 class="section-label">{t('settings.language')}</h2>
      <p class="group-sub">{t('settings.languageSub')}</p>
    </div>
    <div class="card group-body">
      <LanguageSelect />
    </div>
  </section>

  <!-- Apparence -->
  <section class="group">
    <div class="group-head">
      <h2 class="section-label">{t('set.appearance')}</h2>
      <p class="group-sub">{t('set.appearanceSub')}</p>
    </div>
    <div class="card group-body">
      <div class="row">
        <span class="row-label">{t('set.theme')}</span>
        <div class="seg">
          <button class="seg-btn" class:active={prefs.theme === "light"} onclick={() => setTheme("light")}>{t('set.themeLight')}</button>
          <button class="seg-btn" class:active={prefs.theme === "dark"} onclick={() => setTheme("dark")}>{t('set.themeDark')}</button>
          <button class="seg-btn" class:active={prefs.theme === "auto"} onclick={() => setTheme("auto")}>{t('set.themeAuto')}</button>
        </div>
      </div>
      <div class="row">
        <span class="row-label">{t('set.sounds')}</span>
        <div class="seg">
          <button class="seg-btn" class:active={prefs.sound} onclick={() => prefs = { ...prefs, sound: true }}>{t('set.soundsOn')}</button>
          <button class="seg-btn" class:active={!prefs.sound} onclick={() => prefs = { ...prefs, sound: false }}>{t('set.soundsOff')}</button>
        </div>
      </div>
    </div>
  </section>

  <!-- Sécurité -->
  <section class="group">
    <div class="group-head">
      <h2 class="section-label">{t('set.security')}</h2>
      <p class="group-sub">{t('set.securitySub')}</p>
    </div>
    <div class="card group-body">
      <div class="row">
        <span class="row-label">{t('set.autoLock')}</span>
        <div class="seg">
          {#each [0, 5, 15, 30, 60] as m}
            <button class="seg-btn" class:active={prefs.lockMinutes === m} onclick={() => setLockMinutes(m)}>
              {m === 0 ? t('set.autoLockNever') : m + " min"}
            </button>
          {/each}
        </div>
      </div>

      <div class="row">
        <span class="row-label">{t('set.confirmThreshold')}</span>
        <div class="row-control">
          <input class="input threshold-input" type="number" min="0" step="1"
            bind:value={prefs.confirmThreshold} placeholder="100" />
          <span class="unit">QUANTA</span>
        </div>
      </div>

      <div class="row">
        <span class="row-label">{t('set.privacyMode')}</span>
        <div class="seg">
          <button class="seg-btn" class:active={prefs.privacy} onclick={() => prefs = { ...prefs, privacy: true }}>{t('set.privacyOn')}</button>
          <button class="seg-btn" class:active={!prefs.privacy} onclick={() => prefs = { ...prefs, privacy: false }}>{t('set.privacyOff')}</button>
        </div>
      </div>
    </div>
  </section>

  <!-- Identité réseau -->
  <section class="group">
    <div class="group-head">
      <h2 class="section-label">{t('set.nodeShare')}</h2>
      <p class="group-sub">{t('set.nodeShareSub')}</p>
    </div>
    <div class="card group-body">
      <div class="ticket">
        <code class="ticket-val">{nodeTicket}</code>
        <button class="btn btn-ghost btn-sm" onclick={copyTicket} disabled={!nodeTicket || nodeTicket === t('set.offline')}>
          {ticketCopied ? t('set.copied') : t('set.copy')}
        </button>
      </div>
    </div>
  </section>

  <!-- Connecter un pair -->
  <section class="group">
    <div class="group-head">
      <h2 class="section-label">{t('set.connectPeer')}</h2>
      <p class="group-sub">{t('set.connectPeerSub')}</p>
    </div>
    <div class="card group-body">
      <div class="connect">
        <input class="input connect-input" type="text" bind:value={peerInput}
          placeholder={t('set.connectPlaceholder')} />
        <button class="btn btn-primary" onclick={connectPeer} disabled={!peerInput.trim()}>{t('set.connectBtn')}</button>
      </div>
      {#if connectMsg}
        <div class="connect-msg" class:ok={connectStatus === "ok"} class:err={connectStatus === "error"}>
          {connectMsg}
        </div>
      {/if}
    </div>
  </section>

  <!-- Économie QUANTA V2 -->
  {#if economy}
  <section class="group">
    <div class="group-head">
      <h2 class="section-label">{t('set.econTitle')}</h2>
      <p class="group-sub">{t('set.econSub')}</p>
    </div>
    <div class="card group-body">
      <div class="econ-grid">
        <div class="econ-cell">
          <span class="ec-lab">{t('set.econCirculating')}</span>
          <span class="ec-val">{economy.circulating?.toFixed(2) ?? "0"}</span>
          <span class="ec-meta">QUANTA</span>
        </div>
        <div class="econ-cell">
          <span class="ec-lab">{t('set.econBurned')}</span>
          <span class="ec-val">{economy.total_burned?.toFixed(2) ?? "0"}</span>
          <span class="ec-meta">{t('set.econBurnedMeta')}</span>
        </div>
        <div class="econ-cell">
          <span class="ec-lab">{t('set.econHardCap')}</span>
          <span class="ec-val">100M</span>
          <span class="ec-meta">{t('set.econHardCapMeta')}</span>
        </div>
        <div class="econ-cell">
          <span class="ec-lab">{t('set.econTotalMined')}</span>
          <span class="ec-val">{economy.total_mined?.toFixed(2) ?? "0"}</span>
          <span class="ec-meta">{t('set.econTotalMinedMeta')}</span>
        </div>
        <div class="econ-cell">
          <span class="ec-lab">{t('set.econEmission')}</span>
          <span class="ec-val">{economy.emission_per_hour?.toFixed(2) ?? "0"}</span>
          <span class="ec-meta">{t('set.econEmissionMeta')}</span>
        </div>
        <div class="econ-cell">
          <span class="ec-lab">{t('set.econProgress')}</span>
          <span class="ec-val">{economy.max_supply ? (economy.total_mined / economy.max_supply * 100).toFixed(3) : "0"} %</span>
          <span class="ec-meta">{t('set.econProgressMeta')}</span>
        </div>
      </div>
    </div>
  </section>
  {/if}

  <!-- Mise à jour OTA -->
  <section class="group">
    <div class="group-head">
      <h2 class="section-label">{t('set.update')}</h2>
      <p class="group-sub">{t('set.updateSub')}</p>
    </div>
    <div class="card group-body">
      <div class="update">
        {#if updateStatus === "idle"}
          <button class="btn btn-primary" onclick={checkForUpdate}>{t('set.updateCheck')}</button>
        {:else if updateStatus === "checking"}
          <div class="update-info">{t('set.updateChecking')}</div>
        {:else if updateStatus === "downloading"}
          <div class="update-info">{t('set.updateDownloading')} v{updateVersion}</div>
          <div class="progress">
            <div class="progress-fill" style="width:{downloadProgress}%"></div>
          </div>
          <div class="progress-pct">{downloadProgress}%</div>
        {:else if updateStatus === "ready"}
          <div class="update-info accent">v{updateVersion} {t('set.updateInstalled')}</div>
          <button class="btn btn-primary" onclick={doRelaunch}>{t('set.updateRelaunch')}</button>
        {:else if updateStatus === "latest"}
          <div class="update-info accent">{t('set.updateLatest')}</div>
        {:else if updateStatus === "error"}
          <div class="update-info err">{updateError}</div>
          <button class="btn btn-primary" onclick={checkForUpdate}>{t('set.updateRetry')}</button>
        {/if}
      </div>
    </div>
  </section>

  <!-- Charte d'intégrité — confiance -->
  <section class="group">
    <TrustCharter />
  </section>

  <!-- À propos -->
  <section class="group">
    <div class="group-head">
      <h2 class="section-label">{t('set.about')}</h2>
    </div>
    <div class="card group-body">
      <div class="about">
        <span class="about-line"><b>QUANTA</b> · {t('set.aboutTagline')}</span>
        <span class="about-line muted">Tauri 2.0 · Svelte 5 · libSQL · Iroh QUIC · Ed25519 · BLAKE3 · AES-256-GCM</span>
        <span class="about-line muted">{t('set.aboutPrivacy')}</span>
      </div>
    </div>
  </section>
</div>

<style>
  /* Écran calme, colonne étroite « app de réglages » : étiquette de groupe
     au-dessus d'une carte blanche, lignes séparées par des filets, teal seul
     en accent, encre pour tout le reste, vide généreux. Zéro Aurora. */
  .settings-page { max-width: 680px; }

  .group { margin-bottom: var(--space-8); }
  .group-head { padding: 0 var(--space-1); margin-bottom: var(--space-3); }
  .group-head .section-label { margin-bottom: 4px; }
  .group-sub { font-size: var(--text-base); color: var(--color-text-2); line-height: 1.5; }
  .group-body { padding: var(--space-2) var(--space-6); }

  /* ── Ligne de réglage : label à gauche (encre), contrôle à droite ── */
  .row {
    display: flex; align-items: center; gap: var(--space-4);
    min-height: 56px;
    padding: var(--space-3) 0;
    border-top: 1px solid var(--color-border);
  }
  .row:first-child { border-top: none; }
  .row-label {
    flex: 1; min-width: 0;
    font-size: var(--text-base); font-weight: 500; color: var(--color-text-0);
  }
  .row-control { display: flex; align-items: center; gap: var(--space-2); }
  .threshold-input {
    width: 96px; text-align: right;
    font-variant-numeric: tabular-nums lining-nums;
  }
  .unit {
    font-size: var(--text-xs); font-weight: 600; letter-spacing: 0.04em;
    color: var(--color-text-3); text-transform: uppercase;
  }

  /* ── Contrôle segmenté — piste gris clair, puce active blanche surélevée,
       texte teal (le seul signal d'état actif) ── */
  .seg {
    display: inline-flex;
    background: var(--color-bg-2); border-radius: 10px;
    padding: 2px;
  }
  .seg-btn {
    padding: 6px 13px; font-size: var(--text-sm); font-weight: 500;
    border: none; background: transparent; border-radius: 8px;
    color: var(--color-text-2); cursor: pointer; font-family: inherit;
    transition: background var(--dur-fast) var(--ease-out), color var(--dur-fast) var(--ease-out);
  }
  .seg-btn:hover { color: var(--color-text-0); }
  .seg-btn.active {
    background: var(--surface); color: var(--cyan); font-weight: 600;
    box-shadow: var(--shadow-sm);
  }

  /* ── Ticket de nœud ── */
  .ticket {
    display: flex; align-items: center; gap: var(--space-3);
    padding: var(--space-3) 0;
  }
  .ticket-val {
    flex: 1; min-width: 0;
    font-family: var(--font-mono); font-size: var(--text-xs);
    color: var(--color-text-1);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }

  /* ── Connexion d'un pair ── */
  .connect { display: flex; align-items: center; gap: var(--space-3); padding: var(--space-2) 0; }
  .connect-input { flex: 1; font-family: var(--font-mono); font-size: var(--text-sm); }
  .connect-msg {
    margin-top: var(--space-2);
    font-size: var(--text-sm); font-family: var(--font-mono);
    color: var(--color-text-2);
  }
  .connect-msg.ok { color: var(--cyan); }
  .connect-msg.err { color: var(--color-red); }

  /* ── Grille économie — gros chiffres tabulaires, la typo est le héros ── */
  .econ-grid {
    display: grid; grid-template-columns: repeat(3, 1fr);
    gap: var(--space-2);
    padding: var(--space-3) 0;
  }
  .econ-cell {
    display: flex; flex-direction: column; gap: 3px;
    padding: var(--space-3) var(--space-4) var(--space-3) 0;
  }
  .ec-lab {
    font-size: 10px; font-weight: 600; letter-spacing: 0.06em;
    text-transform: uppercase; color: var(--color-text-3);
  }
  .ec-val {
    font-family: var(--font-display); font-size: 22px; font-weight: 700;
    color: var(--color-text-0); line-height: 1.1;
    font-variant-numeric: tabular-nums lining-nums;
  }
  .ec-meta { font-size: var(--text-xs); color: var(--color-text-2); }

  /* ── Mise à jour ── */
  .update {
    display: flex; flex-direction: column; align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-2) 0;
  }
  .update-info { font-size: var(--text-base); color: var(--color-text-1); }
  .update-info.accent { color: var(--cyan); font-weight: 600; }
  .update-info.err { color: var(--color-red); font-size: var(--text-sm); font-family: var(--font-mono); }
  .progress {
    width: 100%; height: 6px; border-radius: 3px;
    background: var(--color-bg-3); overflow: hidden;
  }
  .progress-fill {
    height: 100%; border-radius: 3px;
    background: var(--color-accent);
    transition: width 0.4s var(--ease-out);
  }
  .progress-pct {
    font-size: var(--text-xs); color: var(--color-text-3);
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums lining-nums;
  }

  /* ── À propos ── */
  .about { display: flex; flex-direction: column; gap: 5px; padding: var(--space-2) 0; }
  .about-line { font-size: var(--text-sm); color: var(--color-text-1); line-height: 1.6; }
  .about-line.muted { color: var(--color-text-3); font-size: var(--text-xs); }

  @media (max-width: 640px) {
    .econ-grid { grid-template-columns: 1fr 1fr; }
  }
</style>
