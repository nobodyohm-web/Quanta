<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import NavBar from "$lib/NavBar.svelte";
  import Wallet from "$lib/Wallet.svelte";
  import Dashboard from "$lib/Dashboard.svelte";
  import CommandPalette from "$lib/CommandPalette.svelte";
  import Welcome from "$lib/Welcome.svelte";
  import StrengthMeter from "$lib/StrengthMeter.svelte";
  import TopBar from "$lib/TopBar.svelte";
  import HelpModal from "$lib/HelpModal.svelte";
  import Settings from "$lib/Settings.svelte";
  import { getPrefs, applyTheme } from "$lib/prefs";
  import "../app.css";

  type Step = "check" | "welcome" | "create" | "unlock" | "recovery" | "confirm";

  let view = $state("wallet");
  let ready = $state(false);
  let loading = $state(true);
  let step = $state<Step>("check");
  let name = $state("");
  let pass = $state("");
  let err = $state("");
  let pk = $state("");
  let recoveryKey = $state("");
  let confirmInput = $state("");
  let keyCopied = $state(false);
  let cmdOpen = $state(false);
  let helpOpen = $state(false);
  let profilePk = $state<string | null>(null);

  const lastBlock = $derived(recoveryKey ? recoveryKey.split("-").slice(-1)[0] : "");

  $effect(() => { init(); });
  $effect(() => {
    // Apply persisted theme as soon as the app boots
    applyTheme(getPrefs().theme);
  });
  $effect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.metaKey && e.key === "k") { e.preventDefault(); cmdOpen = !cmdOpen; return; }
      if (e.metaKey && e.key === "/") { e.preventDefault(); helpOpen = !helpOpen; return; }
      // Navigation 1-5 / virgule — uniquement quand l'app est prête et hors input
      if (!ready || e.metaKey || e.ctrlKey || e.altKey) return;
      const t = e.target as HTMLElement;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
      const map: Record<string, string> = {
        "1": "wallet", "2": "network", "3": "settings",
      };
      const v = map[e.key];
      if (v) { e.preventDefault(); nav(v); }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  });

  // ─── Auto-lock after inactivity ──────────────────────────────
  $effect(() => {
    if (!ready) return;
    let last = Date.now();
    const onActivity = () => { last = Date.now(); };
    const events = ["mousemove", "keydown", "click", "touchstart"];
    events.forEach(ev => window.addEventListener(ev, onActivity, { passive: true }));
    const interval = setInterval(() => {
      const lockMin = getPrefs().lockMinutes;
      if (lockMin <= 0) return;
      if (Date.now() - last > lockMin * 60_000) {
        ready = false;
        step = "unlock";
        pass = "";
        err = "";
      }
    }, 30_000);
    return () => {
      events.forEach(ev => window.removeEventListener(ev, onActivity));
      clearInterval(interval);
    };
  });

  async function init() {
    try {
      await new Promise(r => setTimeout(r, 400));
      const has = await invoke<boolean>("check_identity");
      step = has ? "unlock" : "welcome";
    } catch { setTimeout(init, 800); return; }
    loading = false;
  }

  async function create() {
    err = "";
    if (!name.trim() || !pass.trim()) { err = "Les deux champs sont requis"; return; }
    if (pass.length < 8) { err = "Minimum 8 caractères"; return; }
    try {
      const id = await invoke<{ public_key_hex: string }>("create_identity", { displayName: name.trim(), password: pass });
      pk = id.public_key_hex;
      recoveryKey = await invoke<string>("get_recovery_key");
      step = "recovery";
    } catch (e) { err = (e as Error)?.toString() || "Erreur"; }
  }

  async function unlock() {
    err = "";
    if (!pass.trim()) { err = "Mot de passe requis"; return; }
    try {
      const id = await invoke<{ public_key_hex: string }>("unlock_identity", { password: pass });
      pk = id.public_key_hex; ready = true;
    } catch { err = "Mot de passe invalide"; }
  }

  async function copyKey() {
    await navigator.clipboard.writeText(recoveryKey);
    keyCopied = true;
    setTimeout(() => keyCopied = false, 2000);
  }

  function goToConfirm() {
    confirmInput = "";
    err = "";
    step = "confirm";
  }

  function finishConfirm() {
    err = "";
    if (confirmInput.trim().toLowerCase() !== lastBlock.toLowerCase()) {
      err = "Les caractères ne correspondent pas. Vérifiez votre sauvegarde.";
      return;
    }
    ready = true;
  }

  function nav(v: string) { view = v; }
  function handleCmd(id: string) { nav(id); }
</script>

{#if loading}
  <div class="load-screen">
    <div class="load-inner">
      <span class="load-logo">SOVA</span>
      <span class="load-sub">Chargement…</span>
    </div>
  </div>
{:else if !ready && step === "welcome"}
  <Welcome onContinue={() => step = "create"} />

{:else if !ready && step === "recovery"}
  <div class="setup-screen">
    <div class="setup-box">
      <h1 class="setup-title">Clé de récupération</h1>
      <p class="setup-sub">
        Votre clé maître unique. Elle donne accès à tout : contenu, tokens ATN, identité.
        Si vous la perdez, personne ne pourra récupérer votre compte.
      </p>
      <div class="recovery-box">
        <code class="recovery-key">{recoveryKey}</code>
      </div>
      <div class="recovery-warn">
        <span class="rw-icon">!</span>
        Cette clé ne sera plus jamais affichée. Sauvegardez-la avant de continuer.
      </div>
      <div class="setup-form">
        <button class="btn sb" onclick={copyKey}>{keyCopied ? "Copié !" : "Copier la clé"}</button>
        <button class="btn btn-primary sb" onclick={goToConfirm}>J'ai sauvegardé ma clé</button>
      </div>
    </div>
  </div>

{:else if !ready && step === "confirm"}
  <div class="setup-screen">
    <div class="setup-box">
      <h1 class="setup-title">Vérification</h1>
      <p class="setup-sub">
        Pour confirmer votre sauvegarde, retapez le <b>dernier bloc</b> de votre clé de récupération.
      </p>
      <div class="setup-form">
        <div class="fg">
          <label for="confirm-input">Dernier bloc (8 caractères)</label>
          <input class="input mono" id="confirm-input" type="text" autocomplete="off" autocapitalize="off" spellcheck="false"
            bind:value={confirmInput} placeholder="ex: a3f7b2c4"
            onkeydown={(e) => e.key === 'Enter' && finishConfirm()} />
          <span class="fg-hint">Cette étape garantit que vous avez bien sauvegardé la clé complète.</span>
        </div>
        {#if err}<div class="setup-err">{err}</div>{/if}
        <button class="btn btn-primary sb" onclick={finishConfirm}>Confirmer & entrer</button>
        <button class="btn btn-ghost sb" onclick={() => step = "recovery"}>Revoir la clé</button>
      </div>
    </div>
  </div>

{:else if !ready}
  <div class="setup-screen">
    <div class="setup-box">
      <h1 class="setup-title">{step === "create" ? "Créer votre identité" : "Déverrouiller"}</h1>
      <p class="setup-sub">
        {step === "create" ? "Une seule clé. Aucun tiers de confiance." : "Bon retour."}
      </p>
      {#if step === "create"}
        <div class="setup-form">
          <div class="fg">
            <label for="name-input">Nom d'affichage</label>
            <input class="input" id="name-input" type="text" bind:value={name} placeholder="Votre pseudo" maxlength="64" />
          </div>
          <div class="fg">
            <label for="pass-input">Mot de passe</label>
            <input class="input" id="pass-input" type="password" bind:value={pass} placeholder="Au moins 8 caractères" />
            <StrengthMeter password={pass} />
            <span class="fg-hint">Argon2id (64 Mo) · clé chiffrée AES-256-GCM · signature Ed25519</span>
          </div>
          {#if err}<div class="setup-err">{err}</div>{/if}
          <button class="btn btn-primary sb" onclick={create}>Créer mon identité</button>
          <button class="btn btn-ghost sb" onclick={() => step = "unlock"}>J'ai déjà une identité</button>
        </div>
      {:else}
        <div class="setup-form">
          <div class="fg">
            <label for="unlock-pass">Mot de passe</label>
            <input class="input" id="unlock-pass" type="password" bind:value={pass} placeholder="Votre mot de passe"
              onkeydown={(e) => e.key === 'Enter' && unlock()} />
          </div>
          {#if err}<div class="setup-err">{err}</div>{/if}
          <button class="btn btn-primary sb" onclick={unlock}>Déverrouiller</button>
          <button class="btn btn-ghost sb" onclick={() => { step = "welcome"; pass = ""; err = ""; }}>Créer une nouvelle identité</button>
        </div>
      {/if}
    </div>
  </div>
{:else}
  <div class="app">
    <TopBar onHelp={() => helpOpen = true} />
    <main class="main">
      {#if view === "wallet"}<Wallet />
      {:else if view === "network"}<Dashboard />
      {:else if view === "settings"}<Settings />
      {:else}<Wallet />{/if}
    </main>
    <NavBar activeView={view} onNavigate={nav} />
  </div>
  <CommandPalette isOpen={cmdOpen} onClose={() => cmdOpen = false} onCommand={handleCmd} />
  <HelpModal isOpen={helpOpen} onClose={() => helpOpen = false} />
{/if}

<style>
  .load-screen {
    height: 100vh;
    display: flex; align-items: center; justify-content: center;
    background: var(--color-bg-0);
  }
  .load-inner { text-align: center; animation: fadeIn 0.15s ease-out; }
  .load-logo {
    display: block;
    font-size: 24px; font-weight: 700;
    letter-spacing: 0.1em;
    margin-bottom: 8px;
  }
  .load-sub { font-size: 13px; color: var(--color-text-2); }

  .setup-screen {
    height: 100vh;
    display: flex; align-items: center; justify-content: center;
    background: var(--color-bg-0);
    padding: 24px;
  }
  .setup-box {
    width: 100%; max-width: 400px;
    padding: 40px 32px;
    animation: fadeIn 0.15s ease-out;
  }
  .setup-title {
    font-size: 24px; font-weight: 700;
    letter-spacing: -0.03em;
    margin-bottom: 8px;
  }
  .setup-sub {
    font-size: 14px; color: var(--color-text-1);
    line-height: 1.6;
    margin-bottom: 32px;
  }
  .setup-form { display: flex; flex-direction: column; gap: 16px; }
  .fg { display: flex; flex-direction: column; gap: 6px; }
  .fg label {
    font-size: 11px; font-weight: 600;
    color: var(--color-text-2);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .fg-hint {
    font-size: 11px; color: var(--color-text-2);
    line-height: 1.5;
    margin-top: 4px;
  }
  .setup-err {
    font-size: 13px; color: var(--color-red);
    padding: 12px 16px;
    background: rgba(255, 68, 68, 0.06);
    border-radius: var(--radius-sm);
  }
  .sb { width: 100%; }

  .recovery-box {
    padding: 20px;
    margin-bottom: 16px;
    background: var(--color-bg-2);
    border-radius: var(--radius-sm);
    word-break: break-all;
  }
  .recovery-key {
    font-family: var(--font-mono);
    font-size: 13px; font-weight: 500;
    color: var(--color-text-0);
    letter-spacing: 0.04em; line-height: 1.8;
  }
  .recovery-warn {
    display: flex; align-items: flex-start; gap: 10px;
    font-size: 12px; color: var(--color-text-1);
    margin-bottom: 24px; padding: 12px 16px;
    background: var(--color-bg-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    line-height: 1.6;
  }
  .rw-icon {
    width: 18px; height: 18px; min-width: 18px;
    border-radius: 50%;
    background: var(--color-amber); color: #000;
    display: flex; align-items: center; justify-content: center;
    font-size: 11px; font-weight: 700;
  }

  .app {
    display: flex; flex-direction: column;
    height: 100vh; overflow: hidden;
    background: var(--color-bg-0);
  }
  .main {
    flex: 1; overflow: hidden;
    background: var(--color-bg-0);
  }
</style>
