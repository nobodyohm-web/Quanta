<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { check } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { getPrefs, setPrefs, applyTheme, type Prefs } from "./prefs";

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
    try { nodeTicket = await invoke<string>("get_node_ticket"); } catch { nodeTicket = "Hors ligne"; }
    try { economy = await invoke("get_economy_stats"); } catch {}
  }

  async function copyTicket() {
    if (!nodeTicket || nodeTicket === "Hors ligne") return;
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
      connectMsg = "Connecté !";
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
</script>

<div class="page settings-page">
  <h1 class="page-title">Réglages</h1>
  <p class="page-sub">Préférences locales · stockées sur cet appareil uniquement</p>

  <!-- Apparence -->
  <section class="set-card">
    <header class="set-head">
      <h2 class="set-title">Apparence</h2>
      <p class="set-sub">Adaptez l'interface à votre environnement.</p>
    </header>
    <div class="set-row">
      <span class="set-label">Thème</span>
      <div class="seg">
        <button class="seg-btn" class:active={prefs.theme === "light"} onclick={() => setTheme("light")}>Clair</button>
        <button class="seg-btn" class:active={prefs.theme === "dark"} onclick={() => setTheme("dark")}>Sombre</button>
        <button class="seg-btn" class:active={prefs.theme === "auto"} onclick={() => setTheme("auto")}>Auto</button>
      </div>
    </div>
  </section>

  <!-- Sécurité -->
  <section class="set-card">
    <header class="set-head">
      <h2 class="set-title">Sécurité</h2>
      <p class="set-sub">Verrouillage automatique et seuils de transfert.</p>
    </header>

    <div class="set-row">
      <span class="set-label">Verrouillage auto</span>
      <div class="seg">
        {#each [0, 5, 15, 30, 60] as m}
          <button class="seg-btn" class:active={prefs.lockMinutes === m} onclick={() => setLockMinutes(m)}>
            {m === 0 ? "Jamais" : m + " min"}
          </button>
        {/each}
      </div>
    </div>

    <div class="set-row">
      <span class="set-label">Confirmation pour transferts > </span>
      <input class="input set-input" type="number" min="0" step="1"
        bind:value={prefs.confirmThreshold} placeholder="100" />
      <span class="set-suffix">QUANTA</span>
    </div>
  </section>

  <!-- Identité réseau -->
  <section class="set-card">
    <header class="set-head">
      <h2 class="set-title">Partage de nœud</h2>
      <p class="set-sub">Donnez ce ticket à un pair pour qu'il vous trouve sur le réseau.</p>
    </header>
    <div class="ticket-box">
      <code class="ticket-val">{nodeTicket}</code>
      <button class="btn btn-sm" onclick={copyTicket} disabled={!nodeTicket || nodeTicket === "Hors ligne"}>
        {ticketCopied ? "Copié" : "Copier"}
      </button>
    </div>
  </section>

  <!-- Connecter un pair -->
  <section class="set-card">
    <header class="set-head">
      <h2 class="set-title">Connecter un pair</h2>
      <p class="set-sub">Collez le ticket d'un autre nœud pour le rejoindre sur le réseau QUANTA.</p>
    </header>
    <div class="connect-box">
      <input class="input connect-input" type="text" bind:value={peerInput}
        placeholder="Coller le ticket du pair ici..." />
      <button class="btn btn-accent" onclick={connectPeer} disabled={!peerInput.trim()}>Connecter</button>
    </div>
    {#if connectMsg}
      <div class="connect-msg" class:ok={connectStatus === "ok"} class:err={connectStatus === "error"}>
        {connectMsg}
      </div>
    {/if}
  </section>

  <!-- Économie QUANTA V2 -->
  {#if economy}
  <section class="set-card">
    <header class="set-head">
      <h2 class="set-title">Économie QUANTA — temps réel</h2>
      <p class="set-sub">Émission fixe 100 QUANTA/h. Pas de halving. Distribution Shapley.</p>
    </header>
    <div class="econ-grid">
      <div class="econ-cell">
        <span class="ec-lab">Supply circulante</span>
        <span class="ec-val">{economy.circulating?.toFixed(2) ?? "0"}</span>
        <span class="ec-meta">QUANTA</span>
      </div>
      <div class="econ-cell">
        <span class="ec-lab">Brûlés</span>
        <span class="ec-val">{economy.total_burned?.toFixed(2) ?? "0"}</span>
        <span class="ec-meta">QUANTA — déflationniste</span>
      </div>
      <div class="econ-cell">
        <span class="ec-lab">Émission</span>
        <span class="ec-val">100</span>
        <span class="ec-meta">QUANTA/h (fixe, pas de halving)</span>
      </div>
      <div class="econ-cell">
        <span class="ec-lab">Votre taux</span>
        <span class="ec-val">{economy.mining_rate_per_hour?.toFixed(4) ?? "0"}</span>
        <span class="ec-meta">QUANTA/h (Shapley × énergie)</span>
      </div>
      <div class="econ-cell">
        <span class="ec-lab">Trust score</span>
        <span class="ec-val">{economy.your_trust_score?.toFixed(0) ?? "0"}</span>
        <span class="ec-meta">influence votre part Shapley</span>
      </div>
      <div class="econ-cell">
        <span class="ec-lab">Plancher énergie</span>
        <span class="ec-val">{economy.atn_floor_eur?.toFixed(4) ?? "0"}</span>
        <span class="ec-meta">EUR par QUANTA</span>
      </div>
    </div>
  </section>
  {/if}

  <!-- Mise à jour OTA -->
  <section class="set-card">
    <header class="set-head">
      <h2 class="set-title">Mise à jour</h2>
      <p class="set-sub">Vérifiez et installez les nouvelles versions automatiquement.</p>
    </header>
    <div class="update-box">
      {#if updateStatus === "idle"}
        <button class="btn btn-accent" onclick={checkForUpdate}>Vérifier les mises à jour</button>
      {:else if updateStatus === "checking"}
        <div class="update-info">⏳ Vérification en cours...</div>
      {:else if updateStatus === "downloading"}
        <div class="update-info">📦 Téléchargement v{updateVersion}...</div>
        <div class="ep-bar" style="margin-top:8px">
          <div class="ep-fill" style="width:{downloadProgress}%"></div>
        </div>
        <div class="ep-label">{downloadProgress}%</div>
      {:else if updateStatus === "ready"}
        <div class="update-info update-success">✅ v{updateVersion} installée !</div>
        <button class="btn btn-accent" onclick={doRelaunch}>Relancer l'application</button>
      {:else if updateStatus === "latest"}
        <div class="update-info update-success">✅ Vous avez la dernière version.</div>
      {:else if updateStatus === "error"}
        <div class="update-info update-err">❌ {updateError}</div>
        <button class="btn btn-accent" onclick={checkForUpdate}>Réessayer</button>
      {/if}
    </div>
  </section>

  <!-- À propos -->
  <section class="set-card">
    <header class="set-head">
      <h2 class="set-title">À propos</h2>
    </header>
    <div class="about">
      <span class="about-line"><b>QUANTA</b> · Torus Protocol v2</span>
      <span class="about-line">Tauri 2.0 · Svelte 5 · libSQL · Iroh QUIC · Ed25519 · BLAKE3 · AES-256-GCM</span>
      <span class="about-line muted">Vos données ne quittent jamais cet appareil sans votre signature.</span>
    </div>
  </section>
</div>

<style>
  .settings-page { max-width: 720px; }

  .set-card {
    background: var(--color-bg-1); border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    padding: 18px 20px; margin-bottom: 12px;
  }
  .set-head { margin-bottom: 14px; }
  .set-title { font-size: 14px; font-weight: 700; }
  .set-sub { font-size: 12px; color: var(--color-text-2); margin-top: 2px; }

  .set-row {
    display: flex; align-items: center; gap: 10px;
    padding: 10px 0;
    border-bottom: 1px solid var(--color-border);
  }
  .set-row:last-child { border-bottom: none; padding-bottom: 0; }
  .set-label { flex: 1; font-size: 13px; color: var(--color-text-1); }
  .set-input { width: 110px; }
  .set-suffix { font-size: 11px; color: var(--color-text-3); font-family: var(--font-mono); }

  .seg {
    display: inline-flex;
    background: var(--color-bg-2); border-radius: var(--radius);
    padding: 2px;
  }
  .seg-btn {
    padding: 5px 12px; font-size: 12px; font-weight: 500;
    border: none; background: transparent; border-radius: 6px;
    color: var(--color-text-2); cursor: pointer; font-family: inherit;
    transition: all 0.12s;
  }
  .seg-btn:hover { color: var(--color-text-0); }
  .seg-btn.active {
    background: var(--color-bg-1); color: var(--color-accent);
    box-shadow: 0 1px 3px rgba(0,0,0,0.06);
  }

  .ticket-box {
    display: flex; align-items: center; gap: 10px;
    padding: 12px 14px;
    background: var(--color-bg-2); border-radius: var(--radius);
  }
  .ticket-val {
    flex: 1;
    font-family: var(--font-mono); font-size: 11px;
    color: var(--color-text-1);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }

  .connect-box {
    display: flex; align-items: center; gap: 10px;
  }
  .connect-input {
    flex: 1;
    font-family: var(--font-mono); font-size: 11px;
    padding: 10px 12px;
    background: var(--color-bg-2); border: 1px solid var(--color-border);
    border-radius: var(--radius); color: var(--color-text-1);
  }
  .connect-input::placeholder { color: var(--color-text-3); }
  .btn-accent {
    padding: 8px 16px; font-size: 12px; font-weight: 600;
    background: var(--color-accent); color: #fff;
    border: none; border-radius: var(--radius); cursor: pointer;
    transition: opacity 0.15s;
  }
  .btn-accent:hover { opacity: 0.85; }
  .btn-accent:disabled { opacity: 0.4; cursor: default; }
  .connect-msg {
    margin-top: 8px; padding: 6px 10px;
    border-radius: var(--radius); font-size: 11px;
    font-family: var(--font-mono);
  }
  .connect-msg.ok { background: rgba(34,197,94,0.12); color: #22c55e; }
  .connect-msg.err { background: rgba(239,68,68,0.12); color: #ef4444; }
  .econ-grid {
    display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px;
    margin-bottom: 12px;
  }
  .econ-cell {
    padding: 10px 12px;
    background: var(--color-bg-2); border-radius: var(--radius);
    display: flex; flex-direction: column; gap: 2px;
  }
  .ec-lab { font-size: 10px; color: var(--color-text-3); font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; }
  .ec-val { font-size: 18px; font-weight: 800; color: var(--color-text-0); font-family: var(--font-mono); }
  .ec-meta { font-size: 10px; color: var(--color-text-2); }

  .econ-progress { display: flex; flex-direction: column; gap: 4px; padding: 4px 0; }
  .ep-bar {
    height: 6px; border-radius: 3px;
    background: var(--color-bg-2); overflow: hidden;
  }
  .ep-fill {
    height: 100%; background: linear-gradient(90deg, var(--color-accent), #a78bfa);
    border-radius: 3px;
    transition: width 0.4s ease;
  }
  .ep-label { font-size: 10px; color: var(--color-text-3); font-family: var(--font-mono); text-align: center; }

  .about { display: flex; flex-direction: column; gap: 4px; }
  .about-line { font-size: 12px; color: var(--color-text-1); line-height: 1.6; }
  .about-line.muted { color: var(--color-text-3); font-size: 11px; }

  .update-box { display: flex; flex-direction: column; gap: 8px; align-items: flex-start; }
  .update-info { font-size: 13px; color: var(--color-text-1); }
  .update-success { color: #22c55e; }
  .update-err { color: #ef4444; font-size: 11px; font-family: var(--font-mono); }

  @media (max-width: 640px) {
    .econ-grid { grid-template-columns: 1fr 1fr; }
  }
</style>
