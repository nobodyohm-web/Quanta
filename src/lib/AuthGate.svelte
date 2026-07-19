<script lang="ts">
  import {
    unlockIdentity, unlockBiometric, getMyUsername, isUsernameAvailable,
    claimUsername as apiClaimUsername,
  } from "./api";
  import Welcome from "./Welcome.svelte";
  import LanguageSelect from "./LanguageSelect.svelte";
  import QuantaMark from "./brand/QuantaMark.svelte";
  import { t } from "./i18n.svelte";
  import { translateError } from "./errors";

  // Reachable steps: identity creation lives entirely in Welcome.svelte (step
  // "welcome"); this gate only unlocks an existing vault ("unlock") or prompts
  // for a @pseudo ("username") after the identity is available.
  type Step = "welcome" | "unlock" | "username";

  let {
    hasIdentity,
    biometricEnabled = false,
    autoBiometric = false,
    onReady,
  }: {
    /** Un vault existe déjà → écran de déverrouillage ; sinon → inscription. */
    hasIdentity: boolean;
    /** Touch ID configuré (bouton biométrique sur l'écran de déverrouillage). */
    biometricEnabled?: boolean;
    /** Proposer Touch ID automatiquement au montage (vrai UNIQUEMENT au 1ᵉʳ boot,
     *  jamais après un auto-lock — l'utilisateur clique alors le bouton). */
    autoBiometric?: boolean;
    /** Identité prête (déverrouillée / créée / @pseudo réglé) → entrer dans l'app. */
    onReady: (pk: string) => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Intentionnel : `hasIdentity` ne fixe que l'écran INITIAL (déverrouiller vs
  // s'inscrire) ; les transitions ensuite sont internes. Le prop ne change jamais
  // du vivant du composant (calculé au boot ; AuthGate est remonté à l'auto-lock).
  let step = $state<Step>(hasIdentity ? "unlock" : "welcome");
  let pass = $state("");
  let err = $state("");
  let pk = $state("");
  let bioBusy = $state(false);
  let bioAutoTried = false;

  // ─── Pseudo unique (@handle) — adresse de wallet lisible ──────
  let usernameInput = $state("");
  let usernameStatus = $state<"" | "checking" | "available" | "taken" | "invalid">("");
  let usernameErr = $state("");
  let claimingUsername = $state(false);
  let usernameTimer: ReturnType<typeof setTimeout> | null = null;

  async function unlockBio() {
    if (bioBusy) return;
    bioBusy = true; err = "";
    try {
      const id = await unlockBiometric();
      pk = id.public_key_hex;
      await proceedAfterAuth();
    } catch (e) {
      // Cancel/backoff → stay on the password form, show the reason quietly.
      // A refused/cancelled Touch ID (`err.unlockRefused`, or the raw "Touch ID
      // refusé" from the Keychain) stays silent; a real reason (e.g. the
      // brute-force backoff `err.rateLimited:n`) is shown, translated.
      const raw = String(e).replace(/^Error:\s*/, "");
      if (raw !== "err.unlockRefused" && !raw.includes("refusé")) err = translateError(e);
    } finally { bioBusy = false; }
  }

  // Auto-offer Touch ID once at app start (1Password-style). Never re-fires
  // after an auto-lock — `autoBiometric` is false on re-entry, so the user
  // explicitly clicks the button then.
  $effect(() => {
    if (step === "unlock" && biometricEnabled && !bioAutoTried && autoBiometric) {
      bioAutoTried = true;
      unlockBio();
    }
  });

  async function unlock() {
    err = "";
    if (!pass.trim()) { err = "Mot de passe requis"; return; }
    try {
      const id = await unlockIdentity(pass);
      pk = id.public_key_hex;
      await proceedAfterAuth();
    } catch (e) { err = translateError(e, t("err.wrongPassword")); }
  }

  // Une fois l'identité prête : si l'utilisateur n'a pas encore de @pseudo,
  // on l'invite à en choisir un (se retrouver facilement). Sinon, on entre.
  async function proceedAfterAuth() {
    try {
      const u = await getMyUsername();
      if (u) { onReady(pk); }
      else { usernameInput = ""; usernameStatus = ""; usernameErr = ""; step = "username"; }
    } catch {
      onReady(pk); // fail-open : ne jamais bloquer l'accès au wallet
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
        const ok = await isUsernameAvailable(u);
        usernameStatus = ok ? "available" : "taken";
      } catch { usernameStatus = "invalid"; }
    }, 300);
  }

  async function claimUsername() {
    const u = usernameInput.trim().replace(/^@/, "").toLowerCase();
    if (!localValidUsername(u)) { usernameStatus = "invalid"; return; }
    claimingUsername = true; usernameErr = "";
    try {
      await apiClaimUsername(u);
      onReady(pk);
    } catch (e) {
      usernameErr = translateError(e);
    } finally {
      claimingUsername = false;
    }
  }

  function skipUsername() { onReady(pk); }
</script>

{#if step === "welcome"}
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

{:else if step === "username"}
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

{:else}
  <!-- Unlock screen. Identity creation lives entirely in Welcome.svelte (step
       "welcome"); this screen only ever unlocks an existing vault. "Nouvelle
       identité" routes back to the secure onboarding. -->
  <div class="setup-screen">
    <div class="setup-box card">
      <div class="setup-brand">
        <QuantaMark size={32} tone="ink" />
        <div class="setup-brand-txt">
          <div class="setup-wordmark">QUANTA</div>
          <div class="setup-tag">{t('auth.unlock.tag')}</div>
        </div>
      </div>
      <h1 class="setup-title">{t('auth.unlock.title')}</h1>
      <p class="setup-sub">{t('auth.unlock.sub')}</p>
      <div class="setup-form">
        {#if biometricEnabled}
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
        <button class="new-id-cta" onclick={() => { step = "welcome"; pass = ""; err = ""; }}>
          <span>{t('auth.newIdentity')}</span>
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12h14M13 6l6 6-6 6"/></svg>
        </button>
      </div>
      <div class="lang-row"><LanguageSelect /></div>
    </div>
  </div>
{/if}

<style>
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

  /* « Créer une nouvelle identité » — chemin clair vers l'inscription refaite */
  .new-id-cta {
    display: flex; align-items: center; justify-content: center; gap: 8px;
    width: 100%; padding: 12px; margin-top: 2px;
    background: var(--cyan-dim); border: 1px solid var(--cyan-mid);
    border-radius: 11px; color: var(--color-accent-hover);
    font-family: inherit; font-size: 14px; font-weight: 600; cursor: pointer;
    transition: background var(--dur-fast) ease, border-color var(--dur-fast) ease;
  }
  .new-id-cta:hover { background: rgba(11,165,160,0.16); border-color: var(--color-accent); }
</style>
