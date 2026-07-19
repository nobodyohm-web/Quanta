<script lang="ts">
  import { t } from "./i18n.svelte";
  import { getSecurityAudit, type SecurityAudit } from "./api";

  let { isOpen, onClose }: { isOpen: boolean; onClose: () => void } = $props();
  let tab = $state<"start" | "economy" | "security" | "shortcuts">("start");
  let audit = $state<SecurityAudit | null>(null);

  $effect(() => {
    if (isOpen && !audit) {
      getSecurityAudit().then(a => audit = a).catch(() => {});
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
  class="modal-overlay help-overlay"
  onclick={onClose}
  onkeydown={(e) => e.key === 'Escape' && onClose()}
  role="button"
  tabindex="-1"
  aria-label={t('help.overlay_aria')}
>
  <div
    class="modal help-modal"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    aria-labelledby="help-title"
    tabindex="-1"
  >
    <div class="help-head">
      <h2 class="help-title" id="help-title">{t('help.title')}</h2>
      <button class="help-close" onclick={onClose} aria-label={t('help.close_aria')}>×</button>
    </div>

    <div class="help-tabs filter-tabs">
      <button class="filter-tab" class:active={tab === "start"} onclick={() => tab = "start"}>{t('help.tab_start')}</button>
      <button class="filter-tab" class:active={tab === "economy"} onclick={() => tab = "economy"}>{t('help.tab_economy')}</button>
      <button class="filter-tab" class:active={tab === "security"} onclick={() => tab = "security"}>{t('help.tab_security')}</button>
      <button class="filter-tab" class:active={tab === "shortcuts"} onclick={() => tab = "shortcuts"}>{t('help.tab_shortcuts')}</button>
    </div>

    <div class="help-body">
      {#if tab === "start"}
        <h3 class="section-label">{t('help.start_h3')}</h3>
        <ol class="help-ol">
          <li>{@html t('help.start_li1')}</li>
          <li>{@html t('help.start_li2')}</li>
          <li>{@html t('help.start_li3')}</li>
          <li>{@html t('help.start_li4')}</li>
          <li>{@html t('help.start_li5')}</li>
        </ol>
        <p class="help-tip">{t('help.start_tip')}</p>

      {:else if tab === "economy"}
        <h3 class="section-label">{t('help.eco_h3')}</h3>
        <p class="help-p">{@html t('help.eco_intro')}</p>

        <table class="help-table">
          <thead><tr><th>{t('help.eco_th_mechanism')}</th><th>{t('help.eco_th_detail')}</th></tr></thead>
          <tbody>
            <tr><td>{t('help.eco_cap_label')}</td><td>{t('help.eco_cap_detail')}</td></tr>
            <tr><td>{t('help.eco_emission_label')}</td><td>{@html t('help.eco_emission_detail')}</td></tr>
            <tr><td>{t('help.eco_distribution_label')}</td><td>{t('help.eco_distribution_detail')}</td></tr>
            <tr><td>{t('help.eco_burn_label')}</td><td>{t('help.eco_burn_detail')}</td></tr>
            <tr><td>{t('help.eco_cost_label')}</td><td>{@html t('help.eco_cost_detail')}</td></tr>
          </tbody>
        </table>

        <p class="help-p">{t('help.eco_outro')}</p>

      {:else if tab === "security"}
        <h3 class="section-label">{t('help.sec_h3')}</h3>
        <p class="help-p">{t('help.sec_intro')}</p>
        {#if audit}
          <div class="help-grid">
            <div class="help-cell">
              <div class="hc-label">{t('help.sec_signature')}</div>
              <div class="hc-value">{audit.signing?.name}</div>
              <div class="hc-meta">{audit.signing?.standard}</div>
            </div>
            <div class="help-cell">
              <div class="hc-label">{t('help.sec_encryption')}</div>
              <div class="hc-value">{audit.symmetric?.name}</div>
              <div class="hc-meta">{audit.symmetric?.standard}</div>
            </div>
            <div class="help-cell">
              <div class="hc-label">{t('help.sec_derivation')}</div>
              <div class="hc-value">{audit.kdf?.name}</div>
              <div class="hc-meta">{audit.kdf?.standard}</div>
            </div>
            <div class="help-cell">
              <div class="hc-label">{t('help.sec_hashing')}</div>
              <div class="hc-value">{audit.hashing?.name}</div>
              <div class="hc-meta">{audit.hashing?.standard}</div>
            </div>
          </div>
          <div class="help-grade">
            <span class="hg-label">{t('help.sec_grade')}</span>
            <span class="hg-value">{audit.grade}</span>
          </div>
        {/if}
        <p class="help-tip">
          {@html t('help.sec_tip')}
        </p>

      {:else if tab === "shortcuts"}
        <h3 class="section-label">{t('help.sc_h3')}</h3>
        <div class="help-keys">
          <div class="hk-row"><kbd>⌘</kbd> <kbd>K</kbd> <span>{t('help.sc_palette')}</span></div>
          <div class="hk-row"><kbd>⌘</kbd> <kbd>/</kbd> <span>{t('help.sc_help')}</span></div>
          <div class="hk-row"><kbd>Esc</kbd> <span>{t('help.sc_close')}</span></div>
          <div class="hk-row"><kbd>↵</kbd> <span>{t('help.sc_submit')}</span></div>
        </div>
        <p class="help-tip">{t('help.sc_tip')}</p>
      {/if}
    </div>
  </div>
</div>
{/if}

<style>
  /* Voile + carte = vocabulaire .modal* global ; ici uniquement les
     écarts nécessaires (stacking, gabarit colonne, radius 20). */
  .help-overlay { z-index: 200; padding: 24px; }
  .help-modal {
    width: 640px; max-width: 100%; max-height: 80vh;
    padding: 0; border-radius: 20px;
    display: flex; flex-direction: column;
    overflow: hidden;
  }
  .help-head {
    display: flex; align-items: center; justify-content: space-between;
    padding: 16px 24px;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }
  .help-title { font-size: 16px; font-weight: 700; letter-spacing: -0.01em; }
  .help-close {
    width: 28px; height: 28px; border-radius: 8px;
    border: none; background: transparent;
    font-size: 22px; line-height: 1; color: var(--color-text-2);
    cursor: pointer;
    transition: background var(--dur-fast) ease, color var(--dur-fast) ease;
  }
  .help-close:hover { background: var(--color-bg-2); color: var(--color-text-0); }

  /* Onglets = .filter-tab global ; le rail garde sa hairline + scroll. */
  .help-tabs {
    padding: 8px 16px;
    border-bottom: 1px solid var(--color-border);
    overflow-x: auto;
    flex-shrink: 0;
  }

  .help-body {
    padding: 20px 24px; overflow-y: auto; flex: 1;
  }
  .help-p { font-size: 13px; line-height: 1.65; color: var(--color-text-1); margin-bottom: 12px; }
  .help-ol { padding-left: 22px; font-size: 13px; line-height: 1.8; color: var(--color-text-1); }
  .help-ol li { margin-bottom: 4px; }
  .help-tip {
    font-size: 12px; line-height: 1.55; color: var(--color-text-1);
    padding: 10px 14px; margin-top: 14px;
    background: var(--color-bg-2);
    border-radius: 10px;
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
    color: var(--color-text-3); font-weight: 600;
    text-transform: uppercase; letter-spacing: 0.08em; font-size: 10px;
  }
  .help-table td { color: var(--color-text-1); font-variant-numeric: tabular-nums lining-nums; }

  .help-grid {
    display: grid; grid-template-columns: repeat(2, 1fr); gap: 8px;
    margin-bottom: 12px;
  }
  .help-cell {
    padding: 10px 12px;
    background: var(--color-bg-2);
    border-radius: 10px;
  }
  .hc-label { font-size: 10px; color: var(--color-text-3); font-weight: 600; text-transform: uppercase; letter-spacing: 0.08em; margin-bottom: 4px; }
  .hc-value { font-size: 13px; font-weight: 700; color: var(--color-text-0); }
  .hc-meta { font-size: 10px; color: var(--color-text-2); font-family: var(--font-mono); }
  .help-grade {
    display: flex; align-items: center; justify-content: space-between;
    padding: 12px 16px; background: var(--color-bg-2);
    border-radius: 10px; margin-top: 8px;
  }
  .hg-label { font-size: 12px; color: var(--color-text-1); font-weight: 600; }
  .hg-value { font-size: 14px; color: var(--cyan); font-weight: 800; font-family: var(--font-mono); font-variant-numeric: tabular-nums lining-nums; }

  .help-keys { display: flex; flex-direction: column; gap: 4px; }
  .hk-row {
    display: flex; align-items: center; gap: 6px;
    padding: 8px 10px; border-radius: 8px;
    transition: background var(--dur-fast) ease;
  }
  .hk-row:hover { background: var(--color-bg-2); }
  .hk-row span { margin-left: 12px; font-size: 13px; color: var(--color-text-1); }
  kbd {
    display: inline-flex; align-items: center; justify-content: center;
    min-width: 24px; height: 24px; padding: 0 7px;
    background: var(--surface);
    border: 1px solid var(--color-border-hover);
    border-bottom-width: 2px;
    border-radius: 8px;
    box-shadow: var(--shadow-sm);
    font-family: var(--font-mono); font-size: 11px; font-weight: 600;
    color: var(--color-text-1);
  }
  /* Cible le <code> injecté via {@html} (le scoping Svelte ne le voit pas). */
  .help-modal :global(code) {
    font-family: var(--font-mono); font-size: 11px;
    padding: 1px 5px; background: var(--color-bg-2);
    border-radius: 6px;
  }
</style>
