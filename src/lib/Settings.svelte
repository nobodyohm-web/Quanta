<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { check } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { getPrefs, setPrefs, applyTheme, type Prefs } from "./prefs";
  import TrustCharter from "./TrustCharter.svelte";
  import LanguageSelect from "./LanguageSelect.svelte";
  import { t } from "./i18n.svelte";

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
    setTimeout(() => ticketCopied = false, 1600);
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
      setTimeout(() => { connectStatus = "idle"; connectMsg = ""; }, 3000);
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
        setTimeout(() => updateStatus = "idle", 3000);
      }
    } catch (e) {
      updateError = String(e);
      updateStatus = "error";
    }
  }

  async function doRelaunch() {
    await relaunch();
  }

  // ── Dev API (Phase 3) ──
  let devApi = $state<{ enabled: boolean; endpoint: string; token: string } | null>(null);
  let devTokenVisible = $state(false);
  let devTokenCopied = $state(false);
  let devApiBusy = $state(false);

  async function loadDevApi() {
    try {
      devApi = await invoke("dev_api_status");
    } catch {
      devApi = null;
    }
  }

  $effect(() => { loadDevApi(); });

  async function toggleDevApi() {
    if (!devApi) return;
    devApiBusy = true;
    try {
      const next = !devApi.enabled;
      const enabled = await invoke<boolean>("dev_api_set_enabled", { enabled: next });
      devApi = { ...devApi, enabled };
    } catch (e) {
      console.warn("dev_api toggle failed", e);
    } finally {
      devApiBusy = false;
    }
  }

  async function copyDevToken() {
    if (!devApi?.token) return;
    await navigator.clipboard.writeText(devApi.token);
    devTokenCopied = true;
    setTimeout(() => (devTokenCopied = false), 1600);
  }

  async function rotateDevToken() {
    if (!confirm(t('set.regenConfirm'))) return;
    devApiBusy = true;
    try {
      const tok = await invoke<string>("dev_api_rotate_token");
      if (devApi) devApi = { ...devApi, token: tok };
    } finally {
      devApiBusy = false;
    }
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
  <section class="card set-card">
    <header class="set-head">
      <h2 class="section-label">{t('settings.language')}</h2>
      <p class="set-sub">{t('settings.languageSub')}</p>
    </header>
    <LanguageSelect />
  </section>

  <!-- Apparence -->
  <section class="card set-card">
    <header class="set-head">
      <h2 class="section-label">{t('set.appearance')}</h2>
      <p class="set-sub">{t('set.appearanceSub')}</p>
    </header>
    <div class="set-row">
      <span class="set-label">{t('set.theme')}</span>
      <div class="seg">
        <button class="seg-btn" class:active={prefs.theme === "light"} onclick={() => setTheme("light")}>{t('set.themeLight')}</button>
        <button class="seg-btn" class:active={prefs.theme === "dark"} onclick={() => setTheme("dark")}>{t('set.themeDark')}</button>
        <button class="seg-btn" class:active={prefs.theme === "auto"} onclick={() => setTheme("auto")}>{t('set.themeAuto')}</button>
      </div>
    </div>
    <div class="set-row">
      <span class="set-label">{t('set.sounds')}</span>
      <div class="seg">
        <button class="seg-btn" class:active={prefs.sound} onclick={() => prefs = { ...prefs, sound: true }}>{t('set.soundsOn')}</button>
        <button class="seg-btn" class:active={!prefs.sound} onclick={() => prefs = { ...prefs, sound: false }}>{t('set.soundsOff')}</button>
      </div>
    </div>
  </section>

  <!-- Sécurité -->
  <section class="card set-card">
    <header class="set-head">
      <h2 class="section-label">{t('set.security')}</h2>
      <p class="set-sub">{t('set.securitySub')}</p>
    </header>

    <div class="set-row">
      <span class="set-label">{t('set.autoLock')}</span>
      <div class="seg">
        {#each [0, 5, 15, 30, 60] as m}
          <button class="seg-btn" class:active={prefs.lockMinutes === m} onclick={() => setLockMinutes(m)}>
            {m === 0 ? t('set.autoLockNever') : m + " min"}
          </button>
        {/each}
      </div>
    </div>

    <div class="set-row">
      <span class="set-label">{t('set.confirmThreshold')}</span>
      <input class="input set-input" type="number" min="0" step="1"
        bind:value={prefs.confirmThreshold} placeholder="100" />
      <span class="set-suffix">QUANTA</span>
    </div>
  </section>

  <!-- Identité réseau -->
  <section class="card set-card">
    <header class="set-head">
      <h2 class="section-label">{t('set.nodeShare')}</h2>
      <p class="set-sub">{t('set.nodeShareSub')}</p>
    </header>
    <div class="ticket-box">
      <code class="ticket-val">{nodeTicket}</code>
      <button class="btn btn-ghost btn-sm" onclick={copyTicket} disabled={!nodeTicket || nodeTicket === t('set.offline')}>
        {ticketCopied ? t('set.copied') : t('set.copy')}
      </button>
    </div>
  </section>

  <!-- Connecter un pair -->
  <section class="card set-card">
    <header class="set-head">
      <h2 class="section-label">{t('set.connectPeer')}</h2>
      <p class="set-sub">{t('set.connectPeerSub')}</p>
    </header>
    <div class="connect-box">
      <input class="input connect-input" type="text" bind:value={peerInput}
        placeholder={t('set.connectPlaceholder')} />
      <button class="btn btn-primary" onclick={connectPeer} disabled={!peerInput.trim()}>{t('set.connectBtn')}</button>
    </div>
    {#if connectMsg}
      <div class="connect-msg" class:ok={connectStatus === "ok"} class:err={connectStatus === "error"}>
        {connectMsg}
      </div>
    {/if}
  </section>

  <!-- Économie QUANTA V2 -->
  {#if economy}
  <section class="card set-card">
    <header class="set-head">
      <h2 class="section-label">{t('set.econTitle')}</h2>
      <p class="set-sub">{t('set.econSub')}</p>
    </header>
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
  </section>
  {/if}

  <!-- Mise à jour OTA -->
  <section class="card set-card">
    <header class="set-head">
      <h2 class="section-label">{t('set.update')}</h2>
      <p class="set-sub">{t('set.updateSub')}</p>
    </header>
    <div class="update-box">
      {#if updateStatus === "idle"}
        <button class="btn btn-primary" onclick={checkForUpdate}>{t('set.updateCheck')}</button>
      {:else if updateStatus === "checking"}
        <div class="update-info">⏳ {t('set.updateChecking')}</div>
      {:else if updateStatus === "downloading"}
        <div class="update-info">📦 {t('set.updateDownloading')} v{updateVersion}...</div>
        <div class="ep-bar" style="margin-top:8px">
          <div class="ep-fill" style="width:{downloadProgress}%"></div>
        </div>
        <div class="ep-label">{downloadProgress}%</div>
      {:else if updateStatus === "ready"}
        <div class="update-info update-success">✅ v{updateVersion} {t('set.updateInstalled')}</div>
        <button class="btn btn-primary" onclick={doRelaunch}>{t('set.updateRelaunch')}</button>
      {:else if updateStatus === "latest"}
        <div class="update-info update-success">✅ {t('set.updateLatest')}</div>
      {:else if updateStatus === "error"}
        <div class="update-info update-err">❌ {updateError}</div>
        <button class="btn btn-primary" onclick={checkForUpdate}>{t('set.updateRetry')}</button>
      {/if}
    </div>
  </section>

  <!-- API Développeur -->
  <section class="card set-card">
    <header class="set-head">
      <h2 class="section-label">{t('set.devApi')}</h2>
      <p class="set-sub">{t('set.devApiSub')}</p>
    </header>
    {#if devApi}
      <div class="set-row">
        <span class="set-label">{t('set.devApiEnable')}</span>
        <button
          class="pill-toggle"
          class:active={devApi.enabled}
          onclick={toggleDevApi}
          disabled={devApiBusy}
        >{devApi.enabled ? t('set.devApiOn') : t('set.devApiOff')}</button>
      </div>
      {#if devApi.enabled}
        <div class="set-row">
          <span class="set-label">Endpoint</span>
          <code class="dev-endpoint">http://{devApi.endpoint}</code>
        </div>
        <div class="set-row">
          <span class="set-label">Token</span>
          <code class="dev-token">{devTokenVisible ? devApi.token : "•".repeat(64)}</code>
          <button class="btn btn-ghost btn-sm" onclick={() => (devTokenVisible = !devTokenVisible)}>
            {devTokenVisible ? t('set.devHide') : t('set.devShow')}
          </button>
          <button class="btn btn-ghost btn-sm" onclick={copyDevToken}>{devTokenCopied ? "✓ " + t('set.copied') : t('set.copy')}</button>
          <button class="btn btn-ghost btn-sm btn-danger" onclick={rotateDevToken} disabled={devApiBusy}>{t('set.devRegen')}</button>
        </div>
        <div class="dev-hint">
          {t('set.devQuickTest')} <code>curl -H "Authorization: Bearer &lt;token&gt;" http://{devApi.endpoint}/api/status</code>
        </div>
      {/if}
    {:else}
      <div class="dev-hint muted">{t('set.loading')}</div>
    {/if}
  </section>

  <!-- Charte d'intégrité — confiance -->
  <section style="margin-bottom:12px;">
    <TrustCharter />
  </section>

  <!-- À propos -->
  <section class="card set-card">
    <header class="set-head">
      <h2 class="section-label">{t('set.about')}</h2>
    </header>
    <div class="about">
      <span class="about-line"><b>QUANTA</b> · {t('set.aboutTagline')}</span>
      <span class="about-line">Tauri 2.0 · Svelte 5 · libSQL · Iroh QUIC · Ed25519 · BLAKE3 · AES-256-GCM</span>
      <span class="about-line muted">{t('set.aboutPrivacy')}</span>
    </div>
  </section>
</div>

<style>
  /* Écran calme : colonne étroite, cartes blanches globales (.card), zéro Aurora. */
  .settings-page { max-width: 720px; }

  /* Groupes de réglages — la carte vient du vocabulaire global ; ici, le rythme. */
  .set-card { margin-bottom: 12px; }
  .set-head { margin-bottom: 14px; }
  .set-head h2 { margin-bottom: 3px; }
  .set-sub { font-size: 12px; color: var(--color-text-2); }

  .set-row {
    display: flex; align-items: center; gap: 10px;
    padding: 10px 0;
    border-bottom: 1px solid var(--color-border);
  }
  .set-row:last-child { border-bottom: none; padding-bottom: 0; }
  .set-label { flex: 1; font-size: 13px; color: var(--color-text-1); }
  .set-input { width: 110px; }
  .set-suffix { font-size: 11px; color: var(--color-text-3); font-family: var(--font-mono); }

  /* Contrôle segmenté — puce active blanche surélevée, texte teal (état actif). */
  .seg {
    display: inline-flex;
    background: var(--color-bg-2); border-radius: 10px;
    padding: 2px;
  }
  .seg-btn {
    padding: 5px 12px; font-size: 12px; font-weight: 500;
    border: none; background: transparent; border-radius: 8px;
    color: var(--color-text-2); cursor: pointer; font-family: inherit;
    transition: background var(--dur-fast) var(--ease-out), color var(--dur-fast) var(--ease-out);
  }
  .seg-btn:hover { color: var(--color-text-0); }
  .seg-btn.active {
    background: var(--surface); color: var(--cyan); font-weight: 600;
    box-shadow: var(--shadow-sm);
  }

  /* Pastille on/off (API dev) — teal uniquement à l'état actif. */
  .pill-toggle {
    padding: 5px 14px; font-size: 12px; font-weight: 600;
    font-family: inherit; cursor: pointer; border-radius: 8px;
    border: 1px solid var(--color-border-hover);
    background: var(--surface); color: var(--color-text-2);
    box-shadow: var(--shadow-sm);
    transition: background var(--dur-fast) var(--ease-out), color var(--dur-fast) var(--ease-out), border-color var(--dur-fast) var(--ease-out);
  }
  .pill-toggle:hover { color: var(--color-text-0); }
  .pill-toggle.active {
    background: var(--cyan-dim); border-color: var(--cyan-mid); color: var(--cyan);
  }
  .pill-toggle:disabled { opacity: 0.4; cursor: not-allowed; }

  .ticket-box {
    display: flex; align-items: center; gap: 10px;
    padding: 12px 14px;
    background: var(--color-bg-2); border-radius: 10px;
  }
  .ticket-val {
    flex: 1;
    font-family: var(--font-mono); font-size: 11px;
    color: var(--color-text-1);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }

  /* L'input vient de .input (global) ; ici seulement la métrique mono + le flex. */
  .connect-box { display: flex; align-items: center; gap: 10px; }
  .connect-input { flex: 1; font-family: var(--font-mono); font-size: 12px; }

  .connect-msg {
    margin-top: 8px; padding: 6px 10px;
    border-radius: 8px; font-size: 11px;
    font-family: var(--font-mono);
  }
  .connect-msg.ok { background: rgba(22,163,74,0.08); color: var(--color-green); }
  .connect-msg.err { background: rgba(229,72,77,0.08); color: var(--color-red); }

  .econ-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; }
  .econ-cell {
    padding: 10px 12px;
    background: var(--color-bg-2); border-radius: 10px;
    display: flex; flex-direction: column; gap: 2px;
  }
  .ec-lab { font-size: 10px; color: var(--color-text-3); font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; }
  .ec-val {
    font-size: 18px; font-weight: 700; color: var(--color-text-0);
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums lining-nums; font-feature-settings: 'tnum', 'zero';
  }
  .ec-meta { font-size: 10px; color: var(--color-text-2); }

  .ep-bar {
    width: 100%;
    height: 6px; border-radius: 3px;
    background: var(--color-bg-3); overflow: hidden;
  }
  .ep-fill {
    height: 100%; background: var(--color-accent);
    border-radius: 3px;
    transition: width 0.4s var(--ease-out);
  }
  .ep-label {
    font-size: 10px; color: var(--color-text-3); font-family: var(--font-mono);
    font-variant-numeric: tabular-nums lining-nums;
  }

  .about { display: flex; flex-direction: column; gap: 4px; }
  .about-line { font-size: 12px; color: var(--color-text-1); line-height: 1.6; }
  .about-line.muted { color: var(--color-text-3); font-size: 11px; }

  .update-box { display: flex; flex-direction: column; gap: 8px; align-items: flex-start; }
  .update-info { font-size: 13px; color: var(--color-text-1); }
  .update-success { color: var(--color-green); }
  .update-err { color: var(--color-red); font-size: 11px; font-family: var(--font-mono); }

  @media (max-width: 640px) {
    .econ-grid { grid-template-columns: 1fr 1fr; }
  }

  .dev-endpoint, .dev-token {
    font-family: var(--font-mono);
    font-size: 12px;
    background: var(--color-bg-2);
    border: 1px solid var(--color-border);
    padding: 6px 10px;
    border-radius: 8px;
    color: var(--color-text-1);
    flex: 1;
    min-width: 0;
    overflow-x: auto;
    white-space: nowrap;
  }
  .dev-token { letter-spacing: 1px; }
  /* Encadré d'aide — chrome neutre (le teal reste réservé aux actions/états actifs). */
  .dev-hint {
    margin-top: 8px;
    font-size: 12px;
    color: var(--color-text-2);
    background: var(--color-bg-2);
    border-left: 3px solid var(--color-border-hover);
    padding: 8px 12px;
    border-radius: 8px;
    overflow-x: auto;
  }
  .dev-hint code {
    font-family: var(--font-mono);
    background: transparent;
    color: var(--color-text-1);
  }
  .dev-hint.muted { color: var(--color-text-3); border-color: var(--color-border); }
</style>
