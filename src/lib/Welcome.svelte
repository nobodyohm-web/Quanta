<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import StrengthMeter from "./StrengthMeter.svelte";

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
      err = "Mot de passe : minimum 8 caractères";
      return;
    }
    if (showAdvanced && confirmPass !== pass) {
      err = "Les mots de passe ne correspondent pas";
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
      err = (e as Error)?.toString() || "Erreur lors de la création";
    } finally {
      loading = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter") start();
  }
</script>

<div class="welcome">
  <div class="w-content">
    <span class="w-logo">QUANTA</span>
    <h1 class="w-headline">Le Web sans serveur,<br/>la monnaie sans banque.</h1>
    <p class="w-sub">
      Publiez · cherchez · récompensez. <br/>
      Aucun cloud. Aucun intermédiaire. Vous êtes le serveur.
    </p>

    <div class="form">
      <div class="fg">
        <input
          type="password"
          class="big-input"
          placeholder="Mot de passe (min. 8 caractères)"
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
            placeholder="Confirmer le mot de passe"
            bind:value={confirmPass}
            onkeydown={onKey}
            autocomplete="new-password"
          />
        </div>
        <div class="fg">
          <input
            type="text"
            class="big-input"
            placeholder="Pseudo (optionnel — auto si vide)"
            bind:value={name}
            onkeydown={onKey}
            maxlength="64"
          />
        </div>
      {/if}

      {#if err}<div class="err">{err}</div>{/if}

      <button class="primary" onclick={start} disabled={loading || pass.length < 8}>
        {loading ? "Création…" : "Démarrer en 1 clic"}
      </button>

      <div class="links">
        {#if !showAdvanced}
          <button class="ghost-link" onclick={() => showAdvanced = true}>
            Options avancées (pseudo, confirmation)
          </button>
        {/if}
        <button class="ghost-link" onclick={onSwitchToUnlock}>
          J'ai déjà une identité
        </button>
      </div>
    </div>

    <p class="security-note">
      Identité chiffrée localement (Argon2id + AES-256-GCM, signature Ed25519).<br/>
      Vous pourrez sauvegarder votre clé de récupération à tout moment dans <b>Profil → Sauvegarde</b>.
    </p>
  </div>
</div>

<style>
  .welcome {
    height: 100vh;
    display: flex; align-items: center; justify-content: center;
    background: var(--color-bg-0);
    padding: 24px;
  }
  .w-content {
    text-align: center;
    max-width: 440px;
    width: 100%;
    animation: fadeIn 0.15s ease-out;
  }
  .w-logo {
    display: block;
    font-size: 14px; font-weight: 700;
    letter-spacing: 0.15em;
    color: var(--color-text-2);
    margin-bottom: 24px;
  }
  .w-headline {
    font-size: 28px; font-weight: 700;
    letter-spacing: -0.03em;
    line-height: 1.25;
    margin-bottom: 12px;
  }
  .w-sub {
    font-size: 14px;
    color: var(--color-text-1);
    line-height: 1.6;
    margin-bottom: 32px;
  }

  .form {
    display: flex; flex-direction: column; gap: 12px;
    margin-bottom: 24px;
    text-align: left;
  }
  .fg { display: flex; flex-direction: column; gap: 4px; }
  .big-input {
    padding: 14px 16px;
    background: var(--color-bg-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-0);
    font-size: 15px;
  }
  .big-input:focus { outline: 1px solid var(--color-accent); }

  .err {
    font-size: 13px; color: var(--color-red);
    padding: 8px 12px;
    background: rgba(255, 68, 68, 0.06);
    border-radius: var(--radius-sm);
  }

  .primary {
    padding: 14px 24px;
    background: var(--color-accent);
    color: #000;
    border: none;
    border-radius: var(--radius-sm);
    font-size: 16px; font-weight: 600;
    cursor: pointer;
    transition: opacity 0.15s ease;
  }
  .primary:disabled { opacity: 0.4; cursor: default; }
  .primary:hover:not(:disabled) { opacity: 0.9; }

  .links {
    display: flex; flex-direction: column; gap: 6px;
    margin-top: 8px;
    text-align: center;
  }
  .ghost-link {
    background: none; border: none;
    color: var(--color-text-2);
    font-size: 12px;
    cursor: pointer;
    padding: 4px;
    text-decoration: underline;
    text-decoration-color: transparent;
    transition: text-decoration-color 0.15s ease;
  }
  .ghost-link:hover { text-decoration-color: var(--color-text-2); }

  .security-note {
    font-size: 11px;
    color: var(--color-text-2);
    line-height: 1.6;
    margin: 0;
  }
</style>
