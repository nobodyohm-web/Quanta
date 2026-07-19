<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Sidebar from "$lib/Sidebar.svelte";
  import Wallet from "$lib/Wallet.svelte";
  import Dashboard from "$lib/Dashboard.svelte";
  import Network from "$lib/Network.svelte";
  import Profile from "$lib/Profile.svelte";
  import Explorer from "$lib/Explorer.svelte";
  import Contacts from "$lib/Contacts.svelte";
  import CommandPalette from "$lib/CommandPalette.svelte";
  import QuantaMark from "$lib/brand/QuantaMark.svelte";
  import Toasts from "$lib/Toasts.svelte";
  import Welcome from "$lib/Welcome.svelte";
  import LanguageSelect from "$lib/LanguageSelect.svelte";
  import { t } from "$lib/i18n.svelte";
  import StrengthMeter from "$lib/StrengthMeter.svelte";
  import HelpModal from "$lib/HelpModal.svelte";
  import Settings from "$lib/Settings.svelte";
  import { getPrefs, applyTheme } from "$lib/prefs";
  import "../app.css";

  // Reachable steps after the onboarding consolidation: identity creation lives
  // entirely in Welcome.svelte (step "welcome"); this page only boots ("check"),
  // unlocks an existing vault ("unlock"), or prompts for a @pseudo ("username").
  type Step = "check" | "welcome" | "unlock" | "username";

  let view = $state("wallet");
  let ready = $state(false);
  let loading = $state(true);
  let step = $state<Step>("check");
  let pass = $state("");
  let err = $state("");
  let pk = $state("");
  let cmdOpen = $state(false);
  let helpOpen = $state(false);
  let profilePk = $state<string | null>(null);
  // Touch ID quick unlock (macOS) — the OS gates the Keychain KEK by biometry.
  let bioEnabled = $state(false);
  let bioBusy = $state(false);
  let bioAutoTried = false;
  // ML-DSA value address of the unlocked wallet — feeds the live toasts.
  let myAddr = $state("");

  // ─── Pseudo unique (@handle) — adresse de wallet lisible ──────
  let usernameInput = $state("");
  let usernameStatus = $state<"" | "checking" | "available" | "taken" | "invalid">("");
  let usernameErr = $state("");
  let claimingUsername = $state(false);
  let usernameTimer: ReturnType<typeof setTimeout> | null = null;

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
      const tgt = e.target as HTMLElement;
      if (tgt && (tgt.tagName === "INPUT" || tgt.tagName === "TEXTAREA" || tgt.isContentEditable)) return;
      const map: Record<string, string> = {
        "1": "dashboard", "2": "wallet", "3": "network", "4": "profile", "5": "explorer",
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
      if (has) {
        try {
          const st = await invoke<{ supported: boolean; enabled: boolean }>("biometric_status");
          bioEnabled = st.supported && st.enabled;
        } catch { /* biometry optional */ }
      }
    } catch { setTimeout(init, 800); return; }
    loading = false;
  }

  async function unlockBio() {
    if (bioBusy) return;
    bioBusy = true; err = "";
    try {
      const id = await invoke<{ public_key_hex: string }>("unlock_biometric");
      pk = id.public_key_hex;
      await proceedAfterAuth();
    } catch (e) {
      // Cancel/backoff → stay on the password form, show the reason quietly.
      const msg = String(e);
      if (!msg.includes("refusé")) err = msg.replace(/^Error: /, "");
    } finally { bioBusy = false; }
  }

  // Auto-offer Touch ID once at app start (1Password-style). Never re-fires
  // after an auto-lock — the user explicitly clicks the button then.
  $effect(() => {
    if (!loading && step === "unlock" && bioEnabled && !bioAutoTried && !ready) {
      bioAutoTried = true;
      unlockBio();
    }
  });

  // Load the value address once unlocked — feeds the live toasts layer.
  $effect(() => {
    if (!ready) return;
    invoke<string>("get_public_key").then((a) => { myAddr = a; }).catch(() => {});
  });

  async function unlock() {
    err = "";
    if (!pass.trim()) { err = "Mot de passe requis"; return; }
    try {
      const id = await invoke<{ public_key_hex: string }>("unlock_identity", { password: pass });
      pk = id.public_key_hex;
      await proceedAfterAuth();
    } catch { err = "Mot de passe invalide"; }
  }

  // Une fois l'identité prête : si l'utilisateur n'a pas encore de @pseudo,
  // on l'invite à en choisir un (se retrouver facilement). Sinon, on entre.
  async function proceedAfterAuth() {
    try {
      const u = await invoke<string | null>("get_my_username");
      if (u) { ready = true; }
      else { usernameInput = ""; usernameStatus = ""; usernameErr = ""; step = "username"; }
    } catch {
      ready = true; // fail-open : ne jamais bloquer l'accès au wallet
    }
  }

  function localValidUsername(u: string): boolean {
    return (
      u.length >= 3 && u.length <= 20 &&
      /^[a-z][a-z0-9_]*$/.test(u) &&
      !u.endsWith("_") && !u.includes("__")
    );
  }

  function onUsernameInput() {
    usernameErr = "";
    const u = usernameInput.trim().replace(/^@/, "").toLowerCase();
    if (usernameTimer) clearTimeout(usernameTimer);
    if (!u) { usernameStatus = ""; return; }
    if (!localValidUsername(u)) { usernameStatus = "invalid"; return; }
    usernameStatus = "checking";
    usernameTimer = setTimeout(async () => {
      try {
        const ok = await invoke<boolean>("is_username_available", { username: u });
        usernameStatus = ok ? "available" : "taken";
      } catch { usernameStatus = "invalid"; }
    }, 300);
  }

  async function claimUsername() {
    const u = usernameInput.trim().replace(/^@/, "").toLowerCase();
    if (!localValidUsername(u)) { usernameStatus = "invalid"; return; }
    claimingUsername = true; usernameErr = "";
    try {
      await invoke("claim_username", { username: u });
      ready = true;
    } catch (e) {
      usernameErr = String(e);
    } finally {
      claimingUsername = false;
    }
  }

  function skipUsername() { ready = true; }

  function nav(v: string) { view = v; }
  function handleCmd(id: string) { nav(id); }
</script>

{#if loading}
  <div class="load-screen">
    <div class="load-inner">
      <div class="load-mark"><QuantaMark size={46} tone="aurora" /></div>
      <span class="load-logo">QUANTA</span>
      <span class="load-sub">{t('loading')}</span>
    </div>
  </div>
{:else if !ready && step === "welcome"}
  <Welcome
    onCreated={async (created_pk) => {
      pk = created_pk;
      // Welcome.svelte already ran the full secure flow — create → 24-word
      // BIP39 backup → verify (and, on the create path, claimed the @pseudo).
      // So go straight in; proceedAfterAuth only prompts for a handle if the
      // restore path left the account without one.
      await proceedAfterAuth();
    }}
    onSwitchToUnlock={() => step = "unlock"}
  />

{:else if !ready && step === "username"}
  <div class="setup-screen">
    <div class="setup-box card">
      <h1 class="setup-title">{t('su.uname.title')}</h1>
      <p class="setup-sub">{@html t('su.uname.intro')}</p>
      <div class="setup-form">
        <div class="fg">
          <label for="username-input">{t('su.uname.label')}</label>
          <div style="display:flex;align-items:center;gap:8px;">
            <span style="font-size:18px;font-weight:700;color:var(--color-text-2);">@</span>
            <input class="input" id="username-input" type="text" autocomplete="off"
              autocapitalize="off" spellcheck="false" maxlength="20" style="flex:1;"
              bind:value={usernameInput} oninput={onUsernameInput} placeholder="alex"
              onkeydown={(e) => e.key === 'Enter' && usernameStatus === 'available' && claimUsername()} />
          </div>
          {#if usernameStatus === "checking"}
            <span class="fg-hint">{t('su.uname.checking')}</span>
          {:else if usernameStatus === "available"}
            <span class="fg-hint" style="color:var(--color-green);">✓ @{usernameInput.trim().replace(/^@/, "").toLowerCase()} {t('su.uname.avail')}</span>
          {:else if usernameStatus === "taken"}
            <span class="fg-hint" style="color:var(--color-red);">{t('su.uname.taken')}</span>
          {:else if usernameStatus === "invalid"}
            <span class="fg-hint" style="color:var(--color-red);">{t('su.uname.invalid')}</span>
          {:else}
            <span class="fg-hint">{t('su.uname.rule')}</span>
          {/if}
        </div>
        {#if usernameErr}<div class="setup-err">{usernameErr}</div>{/if}
        <button class="btn btn-primary sb" onclick={claimUsername}
          disabled={claimingUsername || usernameStatus !== "available"}>
          {claimingUsername ? t('su.uname.reserving') : t('su.uname.reserve')}
        </button>
        <button class="btn btn-ghost sb" onclick={skipUsername}>{t('su.uname.later')}</button>
      </div>
    </div>
  </div>

{:else if !ready}
  <!-- Unlock screen. Identity creation lives entirely in Welcome.svelte (step
       "welcome"); this screen only ever unlocks an existing vault. "Nouvelle
       identité" routes back to the secure onboarding. -->
  <div class="setup-screen">
    <div class="setup-box card">
      <div class="setup-brand">
        <QuantaMark size={32} tone="aurora" />
        <div class="setup-brand-txt">
          <div class="setup-wordmark">QUANTA</div>
          <div class="setup-tag">{t('auth.unlock.tag')}</div>
        </div>
      </div>
      <h1 class="setup-title">{t('auth.unlock.title')}</h1>
      <p class="setup-sub">{t('auth.unlock.sub')}</p>
      <div class="setup-form">
        {#if bioEnabled}
          <button class="bio-btn" onclick={unlockBio} disabled={bioBusy}>
            <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true">
              <path d="M12 11v3.5M8.5 9.5a3.5 3.5 0 017 0v4a3.5 3.5 0 01-.6 2"/>
              <path d="M5.5 8a6.5 6.5 0 0113 0v5a6.5 6.5 0 01-1.4 4"/>
              <path d="M8.6 18.2A3.5 3.5 0 018.5 17v-2"/>
            </svg>
            {bioBusy ? t('auth.bioBusy') : t('auth.bioUnlock')}
          </button>
          <div class="bio-sep"><span>{t('auth.bioOr')}</span></div>
        {/if}
        <div class="fg">
          <label for="unlock-pass">{t('auth.password')}</label>
          <input class="input" id="unlock-pass" type="password" bind:value={pass} placeholder={t('auth.passwordPh')}
            onkeydown={(e) => e.key === 'Enter' && unlock()} />
        </div>
        {#if err}<div class="setup-err">{err}</div>{/if}
        <button class="btn btn-primary sb" onclick={unlock}>{t('auth.unlockBtn')}</button>
        <button class="btn btn-ghost sb" onclick={() => { step = "welcome"; pass = ""; err = ""; }}>{t('auth.newIdentity')}</button>
      </div>
      <div class="lang-row"><LanguageSelect /></div>
    </div>
  </div>
{:else}
  <div class="app-shell">
    <Sidebar activeView={view} onNavigate={nav} />
    <main class="main-content">
      {#key view}
        <div class="view-anim">
          {#if view === "wallet"}<Wallet />
          {:else if view === "contacts"}<Contacts />
          {:else if view === "dashboard"}<Dashboard />
          {:else if view === "network"}<Network />
          {:else if view === "explorer"}<Explorer />
          {:else if view === "profile"}<Profile />
          {:else if view === "settings"}<Settings />
          {:else}<Wallet />{/if}
        </div>
      {/key}
    </main>
  </div>
  <CommandPalette isOpen={cmdOpen} onClose={() => cmdOpen = false} onCommand={handleCmd} />
  <HelpModal isOpen={helpOpen} onClose={() => helpOpen = false} />
  <Toasts myAddress={myAddr} />
{/if}

<style>
  .load-screen {
    height: 100vh;
    display: flex; align-items: center; justify-content: center;
    background: var(--canvas);
  }
  .load-inner { text-align: center; animation: fadeIn 0.15s ease-out; }
  .load-mark {
    display: flex; justify-content: center; margin-bottom: 14px;
    animation: markBreathe 2.2s ease-in-out infinite;
  }
  @keyframes markBreathe {
    0%, 100% { transform: scale(1); opacity: 1; }
    50% { transform: scale(1.05); opacity: 0.85; }
  }
  @media (prefers-reduced-motion: reduce) { .load-mark { animation: none; } }
  .load-logo {
    display: block;
    font-size: 24px; font-weight: 700;
    letter-spacing: 0.1em;
    margin-bottom: 8px;
  }
  .load-sub { font-size: 13px; color: var(--color-text-2); }

  /* L'entrée — niveau banque : carte blanche nette sur fond clair, zéro
     particule, zéro rail aurora. La typo et le vide portent le moment. */
  .setup-screen {
    height: 100vh; position: relative;
    display: flex; align-items: center; justify-content: center;
    background: var(--canvas);
    padding: 24px;
  }
  .setup-box {
    position: relative; z-index: 1;
    width: 100%; max-width: 410px;
    padding: 36px 36px 30px;
    animation: welcomeRise 0.4s var(--ease-out);
  }
  @keyframes welcomeRise {
    from { opacity: 0; transform: translateY(12px) scale(0.99); }
    to   { opacity: 1; transform: none; }
  }
  @media (prefers-reduced-motion: reduce) { .setup-box { animation: none; } }
  .setup-brand {
    display: flex; align-items: center; gap: 12px;
    margin-bottom: 30px;
  }
  .setup-brand-txt { min-width: 0; }
  .setup-wordmark {
    font-size: 16px; font-weight: 800; letter-spacing: 0.12em;
    color: var(--color-text-0);
  }
  .setup-tag { font-size: 12px; color: var(--color-text-2); margin-top: 1px; }
  .lang-row { display: flex; justify-content: center; margin-top: 18px; }
  .setup-title {
    font-size: 29px; font-weight: 700;
    letter-spacing: -0.03em; line-height: 1.1;
    margin-bottom: 10px; color: var(--color-text-0);
  }
  .setup-sub {
    font-size: 15px; color: var(--color-text-2);
    line-height: 1.55;
    margin-bottom: 28px;
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

  /* Touch ID — the fast path, visually first-class on the unlock card. */
  .bio-btn {
    display: flex; align-items: center; justify-content: center; gap: 10px;
    width: 100%; padding: 13px;
    background: var(--surface);
    border: 1px solid var(--color-border-hover);
    border-radius: 10px;
    box-shadow: var(--shadow-sm);
    color: var(--color-text-0);
    font-family: inherit; font-size: 14px; font-weight: 600;
    cursor: pointer;
    transition: border-color var(--dur-fast) ease, background var(--dur-fast) ease, transform 0.12s var(--ease-out);
  }
  .bio-btn:hover:not(:disabled) { border-color: var(--color-accent); background: var(--cyan-dim); }
  .bio-btn:active:not(:disabled) { transform: scale(0.99); }
  .bio-btn:disabled { opacity: 0.55; cursor: default; }
  .bio-btn svg { color: var(--color-accent); }
  .bio-sep {
    display: flex; align-items: center; gap: 12px;
    color: var(--color-text-3); font-size: 11px;
    text-transform: uppercase; letter-spacing: 0.08em;
  }
  .bio-sep::before, .bio-sep::after {
    content: ""; flex: 1; height: 1px; background: var(--color-border);
  }

  /* Layout handled by app.css .app-shell and .main-content */

  /* Transition de page soyeuse — fondu + glissé + micro-échelle, easing maître. */
  .view-anim {
    animation: viewIn 0.34s cubic-bezier(0.22, 0.61, 0.36, 1);
    will-change: opacity, transform;
  }
  @keyframes viewIn {
    from { opacity: 0; transform: translateY(10px) scale(0.992); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }
  @media (prefers-reduced-motion: reduce) {
    .view-anim { animation: none; }
  }
</style>
