<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import StrengthMeter from "./StrengthMeter.svelte";
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
  <div class="wrap">
    <!-- Moment de marque : l'anneau et le quantum, rail Aurora (card-hero) -->
    <div class="card card-hero hero">
      <div class="brand">
        <QuantaMark size={40} tone="aurora" />
        <span class="wordmark">QUANTA</span>
      </div>
      <h1 class="headline">{@html t("welcome.headline")}</h1>
      <p class="sub">{@html t("welcome.sub")}</p>
    </div>

    <!-- Étape identité : carte blanche calme -->
    <div class="card panel">
      <div class="form">
        <div class="fg">
          <input
            type="password"
            class="input input-lg"
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
              class="input input-lg"
              placeholder={t("welcome.confirm")}
              bind:value={confirmPass}
              onkeydown={onKey}
              autocomplete="new-password"
            />
          </div>
          <div class="fg">
            <input
              type="text"
              class="input input-lg"
              placeholder={t("welcome.pseudo")}
              bind:value={name}
              onkeydown={onKey}
              maxlength="64"
            />
          </div>
        {/if}

        {#if err}<div class="err">{err}</div>{/if}

        <button class="btn btn-primary cta" onclick={start} disabled={loading || pass.length < 8}>
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
    </div>

    <p class="security-note">{@html t("welcome.securityNote")}</p>
    <div class="lang-row"><LanguageSelect /></div>
  </div>
</div>

<style>
  .welcome {
    height: 100vh; position: relative;
    display: flex;
    background: var(--canvas);
    padding: 24px;
    overflow-y: auto;
  }
  /* margin:auto (et non align/justify center) : centre le contenu sans le
     tronquer en haut quand il dépasse la hauteur de la fenêtre. */
  .wrap {
    position: relative; z-index: 1;
    width: 100%; max-width: 440px;
    margin: auto;
    display: flex; flex-direction: column; gap: 12px;
    animation: welcomeRise var(--dur-med) var(--ease-out);
  }
  @keyframes welcomeRise {
    from { opacity: 0; transform: translateY(12px); }
    to   { opacity: 1; transform: none; }
  }
  @media (prefers-reduced-motion: reduce) { .wrap { animation: none; } }

  /* ── Moment de marque (la seule card-hero de l'écran) ── */
  .hero { padding: 28px 30px 26px; }
  .brand { display: flex; align-items: center; gap: 12px; margin-bottom: 18px; }
  .wordmark {
    font-size: 15px; font-weight: 800; letter-spacing: 0.14em;
    color: var(--color-text-0);
  }
  .headline {
    font-size: 27px; font-weight: 700; letter-spacing: -0.025em;
    line-height: 1.15; color: var(--color-text-0);
    margin-bottom: 10px;
  }
  .sub { font-size: 14px; line-height: 1.55; color: var(--color-text-2); }

  /* ── Étape identité ── */
  .panel { padding: 24px 30px 22px; }
  .form { display: flex; flex-direction: column; gap: 12px; text-align: left; }
  .fg { display: flex; flex-direction: column; gap: 6px; }
  /* Modificateur local du .input global : gabarit confortable d'onboarding. */
  .input-lg { padding: 13px 15px; font-size: 15px; }

  .err {
    font-size: 13px; color: var(--color-red);
    padding: 8px 12px;
    background: rgba(229, 72, 77, 0.07);
    border-radius: 8px;
  }

  /* Modificateur local du .btn-primary global : CTA pleine largeur. */
  .cta { width: 100%; padding: 13px 24px; font-size: 14.5px; margin-top: 2px; }

  .links {
    display: flex; flex-direction: column; gap: 6px;
    margin-top: 8px; text-align: center;
  }
  .ghost-link {
    background: none; border: none;
    color: var(--color-text-2);
    font-size: 12px; cursor: pointer; padding: 4px;
    text-decoration: underline; text-decoration-color: transparent;
    transition: text-decoration-color var(--dur-fast) ease, color var(--dur-fast) ease;
  }
  .ghost-link:hover { color: var(--color-text-1); text-decoration-color: var(--color-text-3); }

  .security-note {
    font-size: 11px; color: var(--color-text-3);
    line-height: 1.6; text-align: center; margin: 4px 12px 0;
  }
  .lang-row { display: flex; justify-content: center; margin-top: 2px; }
</style>
