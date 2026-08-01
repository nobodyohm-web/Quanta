<script lang="ts">
  import { untrack } from "svelte";
  import { t } from "./i18n.svelte";
  import { getPrefs, setPrefs } from "./prefs";
  import { takeSendIntent } from "./intents.svelte";
  import {
    walletOverview as walletStore, recentTxs as recentTxsStore,
    nodeStatus as nodeStatusStore,
  } from "./stores.svelte";
  import WalletSend from "./WalletSend.svelte";
  import WalletReceive from "./WalletReceive.svelte";
  import WalletStake from "./WalletStake.svelte";
  import WalletActivity from "./WalletActivity.svelte";

  // ── Données du portefeuille : stores partagés (le solde reste CHAUD entre les
  //    navigations — plus de « 0 » au retour). nodeStatus/txs sont partagés avec
  //    les autres écrans (un seul sondage app-wide). ──
  $effect(() => walletStore.subscribe());
  $effect(() => recentTxsStore.subscribe());
  $effect(() => nodeStatusStore.subscribe());

  const ov = $derived(walletStore.value);
  const txs = $derived(recentTxsStore.value ?? []);
  const loading = $derived(!walletStore.loaded);

  const peers = $derived(nodeStatusStore.value?.peer_count ?? 0);
  const online = $derived(nodeStatusStore.value?.is_online ?? false);

  let panel = $state<"send" | "receive" | "stake" | null>(null);
  // Pré-remplissage du destinataire, alimenté UNIQUEMENT par l'intent inter-vue
  // (Contacts « Envoyer »). Une ouverture manuelle du panneau le remet à vide.
  let sendInitialTo = $state("");

  // Feedback partagé par les panneaux d'action (envoi/stake) : il vit ICI car il
  // SURVIT à la fermeture du panneau (un envoi réussi ferme le panneau mais laisse
  // la bannière de succès) et s'affiche AU-DESSUS des panneaux.
  let feedback = $state<{ ok: boolean; msg: string } | null>(null);
  function setFeedback(fb: { ok: boolean; msg: string } | null) { feedback = fb; }
  function closePanel() { panel = null; }

  // ── Mode privé : montants floutés jusqu'au survol (regard par-dessus l'épaule).
  let privacy = $state(getPrefs().privacy);
  function togglePrivacy() {
    privacy = !privacy;
    setPrefs({ ...getPrefs(), privacy });
  }

  // ── Solde animé : le montant COMPTE jusqu'à sa nouvelle valeur (ticker).
  // « Solde total » = tout ce qu'on détient sur la chaîne (dépensable + staké
  // + en-déverrouillage) ; la ventilation vit dans « Ton argent » plus bas.
  let shownBalance = $state(0);
  $effect(() => {
    const target = ov ? ov.spendable + ov.staked + ov.unbonding : 0;
    const start = untrack(() => shownBalance);
    if (Math.abs(target - start) < 1e-9) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      shownBalance = target;
      return;
    }
    const t0 = performance.now();
    const dur = 750;
    let raf = 0;
    const step = (now: number) => {
      const p = Math.min(1, (now - t0) / dur);
      const e = 1 - Math.pow(1 - p, 3);
      shownBalance = start + (target - start) * e;
      if (p < 1) raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  });

  // Cross-view send intent (Contacts « Envoyer » → single send engine). The
  // {#key view} wrapper in +page remounts this component on each navigation, so
  // a one-shot mount effect is enough — the Wallet is never kept alive in the
  // background. Open the Send panel pre-filled; WalletSend runs the standard
  // Continue → net/burn preview → Confirm (sign) flow.
  $effect(() => {
    const to = untrack(() => takeSendIntent());
    if (!to) return;
    panel = "send";
    feedback = null;
    sendInitialTo = to;
  });

  function togglePanel(p: "send" | "receive" | "stake") {
    // Ouverture manuelle du panneau d'envoi = destinataire vierge (l'intent, lui,
    // ouvre le panneau sans passer par ici).
    if (p === "send" && panel !== "send") sendInitialTo = "";
    panel = panel === p ? null : p;
    feedback = null;
  }
</script>

<div class="page">

  <!-- ── Hero : le solde total — LE moment de l'écran, la typo seule ── -->
  <div class="card w-hero">
    {#if loading}
      <div class="skeleton sk-label"></div>
      <div class="skeleton sk-bal"></div>
      <div class="skeleton sk-sub"></div>
    {:else}
      <div class="w-hero-top">
        <span class="w-hero-label">{t('wallet.totalBalance')}</span>
        <button class="w-eye" onclick={togglePrivacy}
          aria-label={t('wallet.privacyToggle')} title={t('wallet.privacyToggle')}>
          {#if privacy}
            <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M3 3l18 18M10.6 10.7a2.8 2.8 0 003.9 3.9M6.6 6.7C4.3 8.1 2.7 10.2 2 12c1.6 4 5.4 7 10 7 1.9 0 3.7-.5 5.2-1.4M12 5c4.6 0 8.4 3 10 7-.4 1.1-1.1 2.2-2 3.2"/></svg>
          {:else}
            <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M2 12c1.6-4 5.4-7 10-7s8.4 3 10 7c-1.6 4-5.4 7-10 7S3.6 16 2 12z"/><circle cx="12" cy="12" r="3"/></svg>
          {/if}
        </button>
      </div>
      <div class="w-balance-row" class:amt-private={privacy}>
        <span class="w-balance">{shownBalance.toFixed(2)}</span>
        <span class="w-cur">QUANTA</span>
      </div>
      <div class="w-hero-sub" class:amt-private={privacy}>
        <span>{(ov?.spendable ?? 0).toFixed(2)} {t('wallet.available')}</span>
        <span class="w-hero-sub-dot">·</span>
        <span>{(ov?.staked ?? 0).toFixed(2)} {t('wallet.stakedShort')}</span>
        <span class="w-hero-sub-dot">·</span>
        <span>{(ov?.unbonding ?? 0).toFixed(2)} {t('wallet.unbondingShort')}</span>
      </div>
    {/if}
  </div>

  <!-- ── Actions ── -->
  <div class="w-actions">
    <button class="w-btn" class:w-active={panel === "send"} onclick={() => togglePanel("send")}>
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M22 2L11 13M22 2l-7 20-4-9-9-4 20-7z"/>
      </svg>
      <span>{t('wallet.send')}</span>
    </button>
    <button class="w-btn" class:w-active={panel === "receive"} onclick={() => togglePanel("receive")}>
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M7 10l5 5 5-5M12 15V3"/>
      </svg>
      <span>{t('wallet.receive')}</span>
    </button>
    <button class="w-btn" class:w-active={panel === "stake"} onclick={() => togglePanel("stake")}>
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <rect x="4" y="10" width="16" height="10" rx="2"/><path d="M8 10V7a4 4 0 018 0v3"/>
      </svg>
      <span>{t('wallet.stake')}</span>
    </button>
    <button class="w-btn w-btn-soon" disabled aria-disabled="true" title={t('wallet.soon')}>
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M7 8h11M15 5l3 3-3 3M17 16H6M9 13l-3 3 3 3"/>
      </svg>
      <span>{t('wallet.exchange')}</span>
      <span class="w-soon">{t('wallet.soon')}</span>
    </button>
  </div>

  <!-- ── Feedback ── -->
  {#if feedback}
    <div class="w-fb" class:w-fb-ok={feedback.ok} class:w-fb-err={!feedback.ok} role="status">
      {feedback.msg}
    </div>
  {/if}

  <!-- ── Panneaux d'action (sous-composants) — le feedback vit au niveau Wallet ── -->
  {#if panel === "send"}
    <WalletSend initialTo={sendInitialTo} onFeedback={setFeedback} onDone={closePanel} />
  {/if}
  {#if panel === "receive"}
    <WalletReceive />
  {/if}
  {#if panel === "stake"}
    <WalletStake onFeedback={setFeedback} />
  {/if}

  <!-- ── Ton argent — la ventilation à trois compartiments (chaîne) ── -->
  <div class="w-section">
    <div class="section-label">{t('wallet.yourMoney')}</div>
    {#if loading}
      <div class="w-info-list"><div class="skeleton sk-row"></div><div class="skeleton sk-row"></div></div>
    {:else}
      <div class="w-grid">
        <div class="card w-cell">
          <div class="w-cell-k">{t('wallet.available')}</div>
          <div class="w-cell-v mono" class:amt-private={privacy}>{(ov?.spendable ?? 0).toFixed(2)}</div>
          <div class="w-cell-s">{t('wallet.availableSub')}</div>
        </div>
        <div class="card w-cell c-teal">
          <div class="w-cell-k">{t('wallet.inStaking')}</div>
          <div class="w-cell-v mono" class:amt-private={privacy}>{(ov?.staked ?? 0).toFixed(2)}</div>
          <div class="w-cell-s">{t('wallet.inStakingSub')}</div>
        </div>
        <div class="card w-cell c-amber">
          <div class="w-cell-k">{t('wallet.unbonding')}</div>
          <div class="w-cell-v mono" class:amt-private={privacy}>{(ov?.unbonding ?? 0).toFixed(2)}</div>
          <div class="w-cell-s">{t('wallet.unbondingSub')}</div>
        </div>
        <div class="card w-cell c-green">
          <div class="w-cell-k">{t('wallet.forged')}</div>
          <div class="w-cell-v mono" class:amt-private={privacy}>+{(ov?.earned ?? 0).toFixed(2)}</div>
          <div class="w-cell-s">{t('wallet.forgedSub')}</div>
        </div>
      </div>
      <div class="w-status-foot">
        <span class="w-dot" class:online></span>
        <span>{online ? `${t('wallet.connected')} · ${peers} ${peers === 1 ? t('wallet.peer') : t('wallet.peers')}` : t('wallet.offline')}</span>
        <span class="w-sep">·</span>
        <span>{txs.length} {t('wallet.recentTx')}</span>
      </div>
    {/if}
  </div>

  <!-- ── Activité ── -->
  <WalletActivity />

</div>

<style>
  /* ── Hero — le solde total, la typo seule (niveau banque : Trade Republic) ── */
  .w-hero { padding: var(--space-10) var(--space-6) var(--space-8); margin-bottom: var(--space-4); }
  .w-hero-top {
    display: flex; align-items: center; justify-content: space-between;
    margin-bottom: var(--space-2);
  }
  .w-hero-label {
    font-size: var(--text-xs); font-weight: 600; letter-spacing: 0.08em; color: var(--color-text-2);
    text-transform: uppercase;
  }
  .w-balance-row { display: flex; align-items: baseline; gap: var(--space-3); flex-wrap: wrap; }
  .w-balance {
    font-size: clamp(46px, 6.4vw, 56px); font-weight: 700; letter-spacing: -0.03em;
    line-height: 1; color: var(--color-text-0);
    font-variant-numeric: tabular-nums lining-nums;
  }
  .w-cur {
    font-size: 17px; font-weight: 600; letter-spacing: 0.02em; color: var(--color-accent-hover);
  }
  .w-sep { opacity: 0.5; }
  /* Sous-ligne : la ventilation en un coup d'œil — dépensable · staké · en déverrouillage */
  .w-hero-sub {
    display: flex; flex-wrap: wrap; align-items: baseline; gap: var(--space-2);
    margin-top: var(--space-3);
    font-size: var(--text-base); color: var(--color-text-2);
    font-variant-numeric: tabular-nums lining-nums;
  }
  .w-hero-sub-dot { color: var(--color-text-3); }
  .w-eye {
    display: flex; align-items: center; justify-content: center;
    width: 30px; height: 30px; border-radius: 8px;
    background: none; border: none; cursor: pointer;
    color: var(--color-text-3);
    transition: color 0.15s ease, background 0.15s ease;
  }
  .w-eye:hover { color: var(--color-text-1); background: var(--color-bg-2); }
  /* Mode privé : flouté au repos, révélé au survol — le regard du propriétaire. */
  .amt-private { filter: blur(10px); transition: filter 0.2s ease; }
  .amt-private:hover { filter: none; }

  /* Skeletons — base (shimmer discret sur gris chaud) + tailles */
  .skeleton {
    background: linear-gradient(90deg, var(--color-bg-2) 25%, var(--color-bg-3) 50%, var(--color-bg-2) 75%);
    background-size: 200% 100%;
    animation: sk-shimmer 1.4s ease infinite;
  }
  @keyframes sk-shimmer { from { background-position: 200% 0; } to { background-position: -200% 0; } }
  @media (prefers-reduced-motion: reduce) { .skeleton { animation: none; } }
  .sk-label { width: 96px; height: 15px; border-radius: 5px; margin-bottom: var(--space-3); }
  .sk-bal  { width: 240px; height: 60px; border-radius: var(--radius-sm); }
  .sk-sub  { width: 200px; height: 14px; border-radius: 5px; margin-top: var(--space-3); }
  .sk-row  { width: 100%; height: 44px; border-radius: var(--radius-sm); margin-bottom: 6px; }

  /* Actions — trois tuiles blanches flottantes ; l'état actif porte le teal */
  .w-actions {
    display: flex; gap: var(--space-3); margin-bottom: var(--space-3);
  }
  .w-btn {
    flex: 1; display: flex; flex-direction: column; align-items: center; gap: 6px;
    padding: var(--space-3) var(--space-2); min-height: 44px;
    background: var(--surface);
    border: 1px solid var(--color-border); border-radius: var(--radius);
    box-shadow: var(--shadow-sm);
    color: var(--color-text-1);
    font-family: inherit; font-size: var(--text-sm); font-weight: 600;
    cursor: pointer;
    transition: background var(--dur-fast) ease, border-color var(--dur-fast) ease,
      color var(--dur-fast) ease, transform var(--dur-fast) var(--ease-out),
      box-shadow var(--dur-fast) ease;
  }
  .w-btn:hover { border-color: var(--color-border-hover); color: var(--color-text-0); transform: translateY(-1px); box-shadow: var(--shadow); }
  .w-btn.w-active { border-color: var(--cyan); color: var(--cyan); background: var(--cyan-dim); transform: none; box-shadow: var(--shadow-sm); }
  .w-btn:disabled { cursor: default; }
  .w-btn-soon { opacity: 0.7; }
  .w-btn-soon:hover { transform: none; border-color: var(--color-border); color: var(--color-text-1); box-shadow: var(--shadow-sm); }
  .w-soon {
    font-size: 9px; font-weight: 700; letter-spacing: 0.05em; text-transform: uppercase;
    color: var(--color-accent-hover); line-height: 1;
  }

  /* Feedback */
  .w-fb {
    margin: 0 0 var(--space-3);
    padding: 10px 14px; border-radius: 10px;
    font-size: var(--text-base); animation: fadeIn 0.15s ease-out;
  }
  .w-fb-ok  { background: var(--cyan-dim); color: var(--teal-700); border: 1px solid var(--cyan-mid); }
  .w-fb-err { background: var(--color-bg-2); color: var(--color-text-0); border: 1px solid var(--color-border); border-left: 3px solid var(--color-text-0); font-weight: 600; }

  /* Vue d'ensemble — cartes blanches globales (.card) ; la couleur ne vit
     que dans le MONTANT (teal = bondé, ambre = déverrouillage, vert = forgé) */
  .w-grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: var(--space-3); }
  @media (min-width: 660px) { .w-grid { grid-template-columns: repeat(4, 1fr); } }
  .w-cell { padding: var(--space-4) 18px; }
  .w-cell-k {
    font-size: var(--text-xs); color: var(--color-text-3); text-transform: uppercase;
    letter-spacing: 0.04em; font-weight: 600; margin-bottom: var(--space-2);
  }
  .w-cell-v { font-size: 22px; font-weight: 700; color: var(--color-text-0); line-height: 1; }
  .w-cell-s { font-size: var(--text-xs); color: var(--color-text-2); margin-top: 7px; line-height: 1.4; }
  .w-status-foot {
    display: flex; flex-wrap: wrap; gap: 6px; align-items: center;
    margin-top: var(--space-3); font-size: var(--text-xs); color: var(--color-text-3);
  }
  .w-dot { width: 7px; height: 7px; border-radius: 50%; display: inline-block; background: var(--color-text-3); }
  .w-dot.online { background: var(--cyan); }
  /* Discipline couleur (niveau banque) : un seul accent. Le teal marque le
     bondé (l'état actif) ; déverrouillage et forgé restent en encre. */
  .w-cell.c-teal  .w-cell-v { color: var(--cyan); }
  .w-cell.c-amber .w-cell-v { color: var(--color-text-0); }
  .w-cell.c-green .w-cell-v { color: var(--color-text-0); }

  /* Sections — le canevas respire entre les cartes */
  .w-section { margin: 20px 0 var(--space-3); }
</style>
