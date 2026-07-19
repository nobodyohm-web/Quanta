<script lang="ts">
  import { t } from "./i18n.svelte";
  import { blocksToEta, TICKER } from "./quanta";
  import { ledgerStake, ledgerUnstake } from "./api";
  import { walletOverview as walletStore, recentTxs as recentTxsStore } from "./stores.svelte";

  type Feedback = { ok: boolean; msg: string };
  let { onFeedback }: { onFeedback: (fb: Feedback | null) => void } = $props();

  // ── Vérité on-chain du portefeuille : store partagé (chaud entre navigations). ──
  $effect(() => walletStore.subscribe());
  const ov = $derived(walletStore.value);

  /** Re-fetch impératif du portefeuille après une action signée. */
  function refreshWallet() {
    return Promise.all([walletStore.refresh(), recentTxsStore.refresh()]);
  }

  let stakeAmount = $state("");
  let stakeBusy = $state(false);
  let unstakeAmount = $state("");
  let unstakeBusy = $state(false);

  // ── Staking on-chain (ONCHAIN-STAKE-1) — le VRAI enjeu de consensus ──
  async function stake() {
    const amt = parseFloat(stakeAmount);
    if (!isFinite(amt) || amt <= 0) {
      onFeedback({ ok: false, msg: t("wallet.err.invalidAmount") });
      return;
    }
    stakeBusy = true; onFeedback(null);
    try {
      await ledgerStake(amt);
      onFeedback({ ok: true, msg: amt.toFixed(2) + " QUANTA " + t("wallet.ok.staked") });
      stakeAmount = "";
      await refreshWallet();
    } catch (e: unknown) {
      onFeedback({ ok: false, msg: e instanceof Error ? e.message : String(e) });
    } finally { stakeBusy = false; }
  }

  async function unstake() {
    const amt = parseFloat(unstakeAmount);
    if (!isFinite(amt) || amt <= 0) {
      onFeedback({ ok: false, msg: t("wallet.err.invalidAmount") });
      return;
    }
    unstakeBusy = true; onFeedback(null);
    try {
      await ledgerUnstake(amt);
      onFeedback({ ok: true, msg: amt.toFixed(2) + " QUANTA " + t("wallet.ok.unstaked") });
      unstakeAmount = "";
      await refreshWallet();
    } catch (e: unknown) {
      onFeedback({ ok: false, msg: e instanceof Error ? e.message : String(e) });
    } finally { unstakeBusy = false; }
  }

  function etaLabel(blocks: number): string {
    const { days, hours, minutes } = blocksToEta(blocks);
    if (days > 0) return `≈ ${days} ${t("time.d")} ${hours} ${t("time.h")}`;
    if (hours > 0) return `≈ ${hours} ${t("time.h")} ${minutes} ${t("time.min")}`;
    return `≈ ${Math.max(1, minutes)} ${t("time.min")}`;
  }
</script>

<div class="card w-panel">
  <div class="section-label">{t('wallet.stake.title')}</div>

  <!-- Pourquoi staker — le rôle réel dans le protocole (pas de rendement) -->
  <div class="stk-why-title">{t('stk.why.title')}</div>
  <div class="stk-fn-grid">
    <div class="stk-fn-card">
      <svg class="stk-fn-ic" viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 2l8 4.5v9L12 20l-8-4.5v-9L12 2z"/></svg>
      <div class="stk-fn-title">{t('stk.fn.seal.title')}</div>
      <div class="stk-fn-desc">{t('stk.fn.seal.desc')}</div>
    </div>
    <div class="stk-fn-card">
      <svg class="stk-fn-ic" viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M8 12.5l2.5 2.5L16 9.5"/></svg>
      <div class="stk-fn-title">{t('stk.fn.vote.title')}</div>
      <div class="stk-fn-desc">{t('stk.fn.vote.desc')}</div>
    </div>
    <div class="stk-fn-card">
      <svg class="stk-fn-ic" viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3l7 3.2v5.3c0 4.6-3 7.7-7 8.8-4-1.1-7-4.2-7-8.8V6.2L12 3z"/></svg>
      <div class="stk-fn-title">{t('stk.fn.bond.title')}</div>
      <div class="stk-fn-desc">{t('stk.fn.bond.desc')}</div>
    </div>
  </div>

  <div class="stk-honesty">
    <div class="stk-honesty-title">{t('stk.honesty.title')}</div>
    <div class="stk-honesty-body">{t('stk.honesty.body')}</div>
  </div>

  <div class="w-staked-row">
    <span>{t('wallet.stake.bonded')}</span>
    <span class="mono">
      {(ov?.staked ?? 0).toFixed(2)} {TICKER}
      {#if (ov?.pending_stake ?? 0) > 0}
        <span class="stk-pending">+{(ov?.pending_stake ?? 0).toFixed(2)} {t('wallet.stake.pending')}</span>
      {/if}
    </span>
  </div>

  {#if ov && (ov.staked >= ov.min_validator_stake)}
    <div class="stk-validator ok">✓ {t('wallet.stake.validatorOk')}</div>
  {:else if ov}
    <div class="stk-validator">{t('wallet.stake.validatorHint1')} {ov.min_validator_stake.toFixed(0)} {t('wallet.stake.validatorHint2')}</div>
  {/if}

  {#if ov && (ov.unbonding_entries.length > 0 || ov.pending_unstake > 0)}
    <div class="section-label stk-unbond-label">{t('wallet.stake.unbondingTitle')}</div>
    <div class="stk-unbond-list">
      {#each ov.unbonding_entries as e, i (i + ':' + e.unlock_height)}
        <div class="stk-unbond-row">
          <span class="mono">{e.amount.toFixed(2)} {TICKER}</span>
          <span class="stk-eta">{etaLabel(e.blocks_remaining)} · {e.blocks_remaining.toLocaleString()} {t('wallet.stake.blocks')}</span>
        </div>
      {/each}
      {#if ov.pending_unstake > 0}
        <div class="stk-unbond-row">
          <span class="mono">{ov.pending_unstake.toFixed(2)} {TICKER}</span>
          <span class="stk-eta">{t('wallet.stake.pending')}</span>
        </div>
      {/if}
    </div>
  {/if}

  <div class="stk-forms">
    <div class="w-form stk-form">
      <div class="w-field">
        <label for="w-stake-amt">{t('wallet.stake.amountLabel')}</label>
        <input id="w-stake-amt" class="input" type="number"
          min="0.01" step="0.01" placeholder="0.00" bind:value={stakeAmount}
          onkeydown={(e) => e.key === "Enter" && stake()} />
      </div>
      <button class="btn btn-primary" onclick={stake} disabled={stakeBusy}>
        {stakeBusy ? t('wallet.stake.staking') : t('wallet.stake.stakeBtn')}
      </button>
    </div>
    <div class="w-form stk-form">
      <div class="w-field">
        <label for="w-unstake-amt">{t('wallet.stake.unstakeLabel')}</label>
        <input id="w-unstake-amt" class="input" type="number"
          min="0.01" step="0.01" placeholder="0.00" bind:value={unstakeAmount}
          onkeydown={(e) => e.key === "Enter" && unstake()} />
      </div>
      <button class="btn btn-ghost" onclick={unstake} disabled={unstakeBusy || !ov || ov.staked <= 0}>
        {unstakeBusy ? t('wallet.stake.unstaking') : t('wallet.stake.unstakeBtn')}
      </button>
    </div>
  </div>

  <div class="stk-warn stk-warn-amber">
    <span class="stk-warn-ic">!</span>
    <span>{t('wallet.stake.warn')}</span>
  </div>
</div>

<style>
  /* Panels — cartes blanches globales (.card), seul l'agencement reste local */
  .w-panel { margin-bottom: var(--space-3); animation: fadeIn 0.15s ease-out; }
  .w-panel .section-label { margin-bottom: var(--space-4); }

  .w-form  { display: flex; flex-direction: column; gap: var(--space-4); }
  .w-field { display: flex; flex-direction: column; gap: 6px; }
  .w-field label { font-size: var(--text-sm); font-weight: 500; color: var(--color-text-1); }

  /* Titre de la section unbonding du panel Staking */
  .stk-unbond-label { margin-top: var(--space-4); }

  /* Staking — sobre : le teal marque le bondé, l'ambre le déverrouillage */
  .w-staked-row {
    display: flex; justify-content: space-between; align-items: center;
    padding: var(--space-3) 0; border-bottom: 1px solid var(--color-border);
    font-size: var(--text-base); color: var(--color-text-1);
  }
  .w-staked-row .mono { color: var(--cyan); font-weight: 600; }
  .stk-pending { font-size: var(--text-xs); color: var(--color-text-2); margin-left: 6px; font-weight: 600; }
  .stk-validator {
    margin-top: 10px; font-size: var(--text-sm); color: var(--color-text-2);
    padding: var(--space-2) var(--space-3); background: var(--color-bg-2); border-radius: 8px; line-height: 1.5;
  }
  .stk-validator.ok { color: var(--teal-700); font-weight: 600; background: var(--cyan-dim); }
  .stk-unbond-list { display: flex; flex-direction: column; }
  .stk-unbond-row {
    display: flex; justify-content: space-between; align-items: center;
    padding: 9px 0; border-bottom: 1px solid var(--color-border);
    font-size: var(--text-base);
  }
  .stk-unbond-row:last-child { border-bottom: none; }
  .stk-eta { font-size: var(--text-sm); color: var(--color-text-2); }
  .stk-forms {
    display: grid; grid-template-columns: 1fr 1fr; gap: var(--space-4);
    margin-top: var(--space-5);
  }
  @media (max-width: 640px) { .stk-forms { grid-template-columns: 1fr; } }
  .stk-form { align-self: end; }
  .stk-warn {
    display: flex; align-items: flex-start; gap: 10px;
    margin-top: var(--space-5); padding: var(--space-3) 14px;
    background: var(--color-bg-2); border: 1px solid var(--color-border-hover);
    border-radius: var(--radius-sm);
    font-size: var(--text-sm); color: var(--color-text-1); line-height: 1.55;
  }
  .stk-warn-ic {
    width: 18px; height: 18px; min-width: 18px; border-radius: 50%;
    background: var(--color-text-0); color: #fff;
    display: flex; align-items: center; justify-content: center;
    font-size: var(--text-xs); font-weight: 700;
  }
  .stk-warn-amber { border-color: rgba(232,129,12,0.32); background: rgba(232,129,12,0.06); }
  .stk-warn-amber .stk-warn-ic { background: var(--color-amber); }

  /* Pourquoi staker — 3 fonctions réelles du protocole, compactes */
  .stk-why-title {
    font-size: var(--text-xs); font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase;
    color: var(--color-text-3); margin-bottom: 10px;
  }
  .stk-fn-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: var(--space-3); margin-bottom: var(--space-5); }
  @media (max-width: 640px) { .stk-fn-grid { grid-template-columns: 1fr; } }
  .stk-fn-card { padding: var(--space-3); background: var(--color-bg-1); border: 1px solid var(--color-border); border-radius: var(--radius-sm); }
  .stk-fn-ic { color: var(--teal-700); margin-bottom: 6px; }
  .stk-fn-title { font-size: var(--text-sm); font-weight: 600; color: var(--color-text-0); margin-bottom: 3px; }
  .stk-fn-desc { font-size: var(--text-xs); color: var(--color-text-2); line-height: 1.45; }

  /* Honnêteté — pas d'intérêt aujourd'hui, pièces déplacées jamais brûlées */
  .stk-honesty {
    padding: 10px var(--space-3); background: var(--color-bg-2); border-radius: var(--radius-sm);
    margin-bottom: var(--space-4);
  }
  .stk-honesty-title {
    font-size: var(--text-xs); font-weight: 700; text-transform: uppercase; letter-spacing: 0.03em;
    color: var(--color-text-2); margin-bottom: var(--space-1);
  }
  .stk-honesty-body { font-size: var(--text-sm); color: var(--color-text-1); line-height: 1.5; }
</style>
