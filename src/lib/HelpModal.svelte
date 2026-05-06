<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let { isOpen, onClose }: { isOpen: boolean; onClose: () => void } = $props();
  let tab = $state<"start" | "economy" | "security" | "shortcuts">("start");
  let audit = $state<any>(null);

  $effect(() => {
    if (isOpen && !audit) {
      invoke("get_security_audit").then(a => audit = a).catch(() => {});
    }
    if (isOpen) {
      const handler = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
      window.addEventListener("keydown", handler);
      return () => window.removeEventListener("keydown", handler);
    }
  });
</script>

{#if isOpen}
<div
  class="help-overlay"
  onclick={onClose}
  onkeydown={(e) => e.key === 'Escape' && onClose()}
  role="button"
  tabindex="-1"
  aria-label="Fermer l'aide"
>
  <div
    class="help-modal card"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    aria-labelledby="help-title"
    tabindex="-1"
  >
    <div class="help-head">
      <h2 class="help-title" id="help-title">Aide & sécurité</h2>
      <button class="help-close" onclick={onClose} aria-label="Fermer">×</button>
    </div>

    <div class="help-tabs">
      <button class="help-tab" class:active={tab === "start"} onclick={() => tab = "start"}>Démarrage</button>
      <button class="help-tab" class:active={tab === "economy"} onclick={() => tab = "economy"}>Économie QUANTA</button>
      <button class="help-tab" class:active={tab === "security"} onclick={() => tab = "security"}>Sécurité</button>
      <button class="help-tab" class:active={tab === "shortcuts"} onclick={() => tab = "shortcuts"}>Raccourcis</button>
    </div>

    <div class="help-body">
      {#if tab === "start"}
        <h3 class="help-h3">Premiers pas</h3>
        <ol class="help-ol">
          <li><b>Créez du contenu</b> — chaque site publié génère du QUANTA et booste votre confiance.</li>
          <li><b>Publiez en P2P</b> — un clic, et votre site devient un subspace synchronisé via Iroh QUIC.</li>
          <li><b>Restez connecté</b> — le simple fait d'être en ligne mine du QUANTA (énergie réelle convertie, Shapley).</li>
          <li><b>Recevez des interactions</b> — chaque action est une transaction signée sur votre chaîne QUANTA.</li>
        </ol>
        <p class="help-tip">Astuce : utilisez ⌘+K pour naviguer instantanément entre les vues.</p>

      {:else if tab === "economy"}
        <h3 class="help-h3">Comment le QUANTA prend de la valeur</h3>
        <p class="help-p">Chaque QUANTA est adossé à une dépense énergétique réelle. Le prix de l'énergie de votre pays fixe une valeur plancher. Au-delà, c'est l'utilité du réseau qui détermine le prix.</p>

        <table class="help-table">
          <thead><tr><th>Mécanisme</th><th>Détail</th></tr></thead>
          <tbody>
            <tr><td>Émission</td><td>100 QUANTA/h (fixe, pas de halving)</td></tr>
            <tr><td>Distribution</td><td>Shapley Value (énergie 30% + travail 35% + validation 20% + uptime 15%)</td></tr>
            <tr><td>Burn</td><td>1% de chaque transfert détruit (déflationniste)</td></tr>
            <tr><td>Plancher</td><td>Indexé sur le coût énergie réel du réseau</td></tr>
          </tbody>
        </table>

        <p class="help-p">Toutes les transactions sont scellées dans des blocs immuables et propagées via gossip.</p>

      {:else if tab === "security"}
        <h3 class="help-h3">Garanties cryptographiques</h3>
        <p class="help-p">QUANTA n'utilise que des primitives cryptographiques auditées. Aucun protocole maison.</p>
        {#if audit}
          <div class="help-grid">
            <div class="help-cell">
              <div class="hc-label">Signature</div>
              <div class="hc-value">{audit.signing?.name}</div>
              <div class="hc-meta">{audit.signing?.standard}</div>
            </div>
            <div class="help-cell">
              <div class="hc-label">Chiffrement</div>
              <div class="hc-value">{audit.symmetric?.name}</div>
              <div class="hc-meta">{audit.symmetric?.standard}</div>
            </div>
            <div class="help-cell">
              <div class="hc-label">Dérivation</div>
              <div class="hc-value">{audit.kdf?.name}</div>
              <div class="hc-meta">{audit.kdf?.standard}</div>
            </div>
            <div class="help-cell">
              <div class="hc-label">Hashing</div>
              <div class="hc-value">{audit.hashing?.name}</div>
              <div class="hc-meta">{audit.hashing?.standard}</div>
            </div>
          </div>
          <div class="help-grade">
            <span class="hg-label">Grade global :</span>
            <span class="hg-value">{audit.grade}</span>
          </div>
        {/if}
        <p class="help-tip">
          Votre clé privée ne quitte jamais cet appareil. Elle est chiffrée au repos avec une clé dérivée de votre mot de passe via Argon2id (64 Mo de mémoire, 3 itérations). En mémoire vive, les secrets sont effacés via <code>zeroize</code>.
        </p>

      {:else if tab === "shortcuts"}
        <h3 class="help-h3">Raccourcis clavier</h3>
        <div class="help-keys">
          <div class="hk-row"><kbd>⌘</kbd> <kbd>K</kbd> <span>Ouvrir la palette</span></div>
          <div class="hk-row"><kbd>⌘</kbd> <kbd>/</kbd> <span>Cette aide</span></div>
          <div class="hk-row"><kbd>Esc</kbd> <span>Fermer une fenêtre</span></div>
          <div class="hk-row"><kbd>↵</kbd> <span>Valider un formulaire</span></div>
        </div>
        <p class="help-tip">Vous pouvez glisser-déposer du Markdown depuis n'importe où dans l'éditeur.</p>
      {/if}
    </div>
  </div>
</div>
{/if}

<style>
  .help-overlay {
    position: fixed; inset: 0; z-index: 200;
    display: flex; align-items: center; justify-content: center;
    background: rgba(0,0,0,0.18); backdrop-filter: blur(4px);
    animation: fadeIn 0.15s ease-out;
    padding: 24px;
  }
  .help-modal {
    width: 640px; max-width: 100%; max-height: 80vh;
    display: flex; flex-direction: column;
    overflow: hidden;
    box-shadow: 0 24px 48px rgba(0,0,0,0.12);
  }
  .help-head {
    display: flex; align-items: center; justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--color-border);
  }
  .help-title { font-size: 16px; font-weight: 700; }
  .help-close {
    width: 28px; height: 28px; border-radius: 6px;
    border: none; background: transparent;
    font-size: 22px; line-height: 1; color: var(--color-text-2);
    cursor: pointer;
  }
  .help-close:hover { background: var(--color-bg-2); color: var(--color-text-0); }

  .help-tabs {
    display: flex; gap: 2px; padding: 6px 12px;
    border-bottom: 1px solid var(--color-border);
    overflow-x: auto;
  }
  .help-tab {
    padding: 6px 12px; font-size: 12px; font-weight: 500;
    border: none; background: transparent; border-radius: 6px;
    color: var(--color-text-2); cursor: pointer; white-space: nowrap;
    font-family: inherit;
  }
  .help-tab:hover { color: var(--color-text-0); background: var(--color-bg-2); }
  .help-tab.active { color: var(--color-accent); background: var(--color-accent-dim); font-weight: 700; }

  .help-body {
    padding: 20px; overflow-y: auto; flex: 1;
  }
  .help-h3 { font-size: 14px; font-weight: 700; margin-bottom: 10px; }
  .help-p { font-size: 13px; line-height: 1.65; color: var(--color-text-1); margin-bottom: 12px; }
  .help-ol { padding-left: 22px; font-size: 13px; line-height: 1.8; color: var(--color-text-1); }
  .help-ol li { margin-bottom: 4px; }
  .help-tip {
    font-size: 12px; color: var(--color-text-1);
    padding: 10px 12px; margin-top: 12px;
    background: var(--color-accent-dim);
    border-radius: var(--radius);
    border-left: 2px solid var(--color-accent);
  }

  .help-table {
    width: 100%; border-collapse: collapse; margin: 12px 0;
    font-size: 12px;
  }
  .help-table th, .help-table td {
    padding: 7px 10px; text-align: left;
    border-bottom: 1px solid var(--color-border);
  }
  .help-table th {
    color: var(--color-text-3); font-weight: 700;
    text-transform: uppercase; letter-spacing: 0.06em; font-size: 10px;
  }
  .help-table td { color: var(--color-text-1); }

  .help-grid {
    display: grid; grid-template-columns: repeat(2, 1fr); gap: 8px;
    margin-bottom: 12px;
  }
  .help-cell {
    padding: 10px 12px;
    background: var(--color-bg-2);
    border-radius: var(--radius);
  }
  .hc-label { font-size: 10px; color: var(--color-text-3); font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; margin-bottom: 4px; }
  .hc-value { font-size: 13px; font-weight: 700; color: var(--color-text-0); }
  .hc-meta { font-size: 10px; color: var(--color-text-2); font-family: var(--font-mono); }
  .help-grade {
    display: flex; align-items: center; justify-content: space-between;
    padding: 12px 16px; background: var(--color-accent-dim);
    border-radius: var(--radius); margin-top: 8px;
  }
  .hg-label { font-size: 12px; color: var(--color-text-1); font-weight: 600; }
  .hg-value { font-size: 14px; color: var(--color-accent); font-weight: 800; font-family: var(--font-mono); }

  .help-keys { display: flex; flex-direction: column; gap: 4px; }
  .hk-row {
    display: flex; align-items: center; gap: 6px;
    padding: 8px 10px; border-radius: var(--radius);
  }
  .hk-row:hover { background: var(--color-bg-2); }
  .hk-row span { margin-left: 12px; font-size: 13px; color: var(--color-text-1); }
  kbd {
    display: inline-flex; align-items: center; justify-content: center;
    min-width: 22px; height: 22px; padding: 0 6px;
    background: var(--color-bg-1);
    border: 1px solid var(--color-border-hover);
    border-bottom-width: 2px;
    border-radius: 4px;
    font-family: var(--font-mono); font-size: 11px; font-weight: 700;
    color: var(--color-text-1);
  }
  code {
    font-family: var(--font-mono); font-size: 11px;
    padding: 1px 5px; background: var(--color-bg-1);
    border-radius: 3px;
  }
</style>
