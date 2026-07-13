<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import StrengthMeter from "./StrengthMeter.svelte";
  import Aurora from "./Aurora.svelte";
  import QuantaMark from "./brand/QuantaMark.svelte";
  import LanguageSelect from "./LanguageSelect.svelte";
  import QuantumField from "./QuantumField.svelte";
  import { t } from "./i18n.svelte";

  let {
    onCreated = (_pk: string) => {},
    onSwitchToUnlock = () => {},
  } = $props<{
    onCreated?: (pk: string) => void;
    onSwitchToUnlock?: () => void;
  }>();

  let pass = $state("");
  let confirmPass = $state("");
  let name = $state("");
  let loading = $state(false);
  let err = $state("");
  let showAdvanced = $state(false);

  /// Génère un pseudo-utilisateur léger : `User-` + 4 chiffres aléatoires.
  /// L'utilisateur peut le changer plus tard dans Settings.
  function suggestName(): string {
    const n = Math.floor(Math.random() * 9000) + 1000;
    return `User-${n}`;
  }

  async function start() {
    err = "";
    if (pass.length < 8) {
      err = t("welcome.errPass");
      return;
    }
    if (showAdvanced && confirmPass !== pass) {
      err = t("welcome.errMismatch");
      return;
    }
    loading = true;
    try {
      const finalName = name.trim() || suggestName();
      const id = await invoke<{ public_key_hex: string }>("create_identity", {
        displayName: finalName,
        password: pass,
      });
      onCreated(id.public_key_hex);
    } catch (e) {
      err = (e as Error)?.toString() || t("welcome.errCreate");
    } finally {
      loading = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter") start();
  }
</script>

<div class="welcome">
  <QuantumField density={1.1} />
  <div class="card">
    <!-- Aurora hero — l'artefact de marque (moment, jamais le chrome) -->
    <div class="hero">
      <Aurora radius={0}>
        <div class="hero-inner">
          <div class="brand">
            <div class="coin-glass"><QuantaMark size={28} tone="white" /></div>
            <span class="wordmark">QUANTA</span>
          </div>
          <h1 class="headline">{@html t("welcome.headline")}</h1>
          <p class="sub">{@html t("welcome.sub")}</p>
        </div>
      </Aurora>
    </div>

    <!-- Panneau identité (chrome clair & sobre) -->
    <div class="panel">
    <div class="form">
      <div class="fg">
        <input
          type="password"
          class="big-input"
          placeholder={t("welcome.password")}
          bind:value={pass}
          onkeydown={onKey}
          autocomplete="new-password"
        />
        <StrengthMeter password={pass} />
      </div>

      {#if showAdvanced}
        <div class="fg">
          <input
            type="password"
            class="big-input"
            placeholder={t("welcome.confirm")}
            bind:value={confirmPass}
            onkeydown={onKey}
            autocomplete="new-password"
          />
        </div>
        <div class="fg">
          <input
            type="text"
            class="big-input"
            placeholder={t("welcome.pseudo")}
            bind:value={name}
            onkeydown={onKey}
            maxlength="64"
          />
        </div>
      {/if}

      {#if err}<div class="err">{err}</div>{/if}

      <button class="primary" onclick={start} disabled={loading || pass.length < 8}>
        {loading ? t("welcome.creating") : t("welcome.start")}
      </button>

      <div class="links">
        {#if !showAdvanced}
          <button class="ghost-link" onclick={() => showAdvanced = true}>
            {t("welcome.advanced")}
          </button>
        {/if}
        <button class="ghost-link" onclick={onSwitchToUnlock}>
          {t("welcome.haveIdentity")}
        </button>
      </div>
    </div>

      <p class="security-note">{@html t("welcome.securityNote")}</p>
      <div class="lang-row"><LanguageSelect /></div>
    </div>
  </div>
</div>

<style>
  .welcome {
    height: 100vh; position: relative;
    display: flex; align-items: center; justify-content: center;
    background: var(--color-bg-2);
    padding: 24px;
  }
  .card {
    position: relative; z-index: 1;
    width: 100%; max-width: 460px;
    background: var(--color-bg-0);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    box-shadow: var(--shadow-lg);
    overflow: hidden;
    animation: welcomeRise 0.4s cubic-bezier(0.22, 0.61, 0.36, 1);
  }
  @keyframes welcomeRise {
    from { opacity: 0; transform: translateY(12px) scale(0.99); }
    to   { opacity: 1; transform: none; }
  }

  /* ── Aurora hero (moment de marque) ── */
  .hero { position: relative; }
  .hero-inner { padding: 30px 32px 34px; }
  .lang-row { display: flex; justify-content: center; margin-top: 16px; }

  .brand { display: flex; align-items: center; gap: 12px; margin-bottom: 22px; }
  .coin-glass {
    width: 44px; height: 44px; border-radius: 13px;
    background: rgba(255,255,255,0.18);
    border: 1px solid rgba(255,255,255,0.4);
    display: flex; align-items: center; justify-content: center;
    font-size: 23px; font-weight: 800; color: #fff;
    backdrop-filter: blur(6px);
    box-shadow: 0 4px 16px rgba(0,0,0,0.12);
  }
  .wordmark { font-size: 22px; font-weight: 800; letter-spacing: 0.14em; color: #fff; }
  .headline {
    font-size: 27px; font-weight: 800;
    letter-spacing: -0.02em; line-height: 1.1;
    color: #fff; margin-bottom: 12px;
  }
  .sub {
    font-size: 14.5px; line-height: 1.55;
    color: rgba(255,255,255,0.92);
    max-width: 380px;
  }

  /* ── Panneau identité (chrome clair) ── */
  .panel { padding: 26px 32px 28px; }
  .form {
    display: flex; flex-direction: column; gap: 12px;
    margin-bottom: 18px; text-align: left;
  }
  .fg { display: flex; flex-direction: column; gap: 6px; }
  .big-input {
    padding: 13px 15px;
    background: var(--color-bg-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius);
    color: var(--color-text-0);
    font-size: 15px;
    transition: border-color 0.15s ease, box-shadow 0.15s ease;
  }
  .big-input:focus {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: 0 0 0 3px var(--color-accent-dim);
  }

  .err {
    font-size: 13px; color: var(--color-red);
    padding: 8px 12px;
    background: rgba(229, 72, 77, 0.07);
    border-radius: var(--radius-sm);
  }

  .primary {
    padding: 14px 24px;
    background: var(--color-accent);
    color: #fff; border: none;
    border-radius: var(--radius);
    font-size: 15px; font-weight: 700;
    cursor: pointer;
    box-shadow: 0 6px 18px rgba(11, 165, 160, 0.26);
    transition: transform 0.12s ease, box-shadow 0.15s ease, background 0.15s ease;
  }
  .primary:disabled { opacity: 0.4; cursor: default; box-shadow: none; }
  .primary:not(:disabled):hover { background: var(--color-accent-hover); box-shadow: 0 8px 22px rgba(11, 165, 160, 0.32); }
  .primary:not(:disabled):active { transform: translateY(1px); }

  .links {
    display: flex; flex-direction: column; gap: 6px;
    margin-top: 10px; text-align: center;
  }
  .ghost-link {
    background: none; border: none;
    color: var(--color-text-2);
    font-size: 12px; cursor: pointer; padding: 4px;
    text-decoration: underline; text-decoration-color: transparent;
    transition: text-decoration-color 0.15s ease, color 0.15s ease;
  }
  .ghost-link:hover { color: var(--color-text-1); text-decoration-color: var(--color-text-3); }

  .security-note {
    font-size: 11px; color: var(--color-text-3);
    line-height: 1.6; text-align: center; margin: 14px 0 0;
  }
</style>
