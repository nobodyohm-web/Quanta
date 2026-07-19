<script lang="ts">
  import {
    wasGuardianReload, getPublicKey, checkIdentity, createIdentity, unlockIdentity,
    biometricStatus,
  } from "$lib/api";
  import Sidebar from "$lib/Sidebar.svelte";
  import Wallet from "$lib/Wallet.svelte";
  import Dashboard from "$lib/Dashboard.svelte";
  import Network from "$lib/Network.svelte";
  import Profile from "$lib/Profile.svelte";
  import Contacts from "$lib/Contacts.svelte";
  import CommandPalette from "$lib/CommandPalette.svelte";
  import QuantaMark from "$lib/brand/QuantaMark.svelte";
  import Toasts from "$lib/Toasts.svelte";
  import AuthGate from "$lib/AuthGate.svelte";
  import { t } from "$lib/i18n.svelte";
  import HelpModal from "$lib/HelpModal.svelte";
  import Settings from "$lib/Settings.svelte";
  import Whitepaper from "$lib/Whitepaper.svelte";
  import { getPrefs, applyTheme } from "$lib/prefs";
  import { startDiag, note } from "$lib/diag";
  import { warmAudio } from "$lib/sound";
  // Local fonts — bundled, no CDN (offline-first). Inter for everything,
  // JetBrains Mono reserved for the pro terminal only.
  import "@fontsource-variable/inter";
  import "@fontsource-variable/jetbrains-mono";
  import "../app.css";

  // Sonde de gel : démarrée avant tout (patch d'invoke inclus) — un thread UI
  // bloqué > 600 ms devient un rapport daté avec le contexte des opérations.
  if (typeof window !== "undefined") startDiag();

  // Autopilote de diagnostic (dev uniquement, jamais en release) : une instance
  // jetable — dossier de données séparé — entre seule, droit sur une vue cible,
  // pour reproduire un bug sous la sonde sans interaction.
  const AUTOPILOT =
    import.meta.env.DEV && import.meta.env.VITE_QUANTA_AUTOPILOT === "1";

  let view = $state("wallet");
  let ready = $state(false);
  let loading = $state(true);
  let pk = $state("");
  let cmdOpen = $state(false);
  let helpOpen = $state(false);
  let profilePk = $state<string | null>(null);
  // ML-DSA value address of the unlocked wallet — feeds the live toasts.
  let myAddr = $state("");

  // ── État d'auth transmis à AuthGate (calculé au boot) ──
  // Un vault existe-t-il déjà (déverrouillage) ou faut-il s'inscrire (welcome) ;
  // Touch ID est-il configuré ; et est-ce le PREMIER passage (auto-offre bio une
  // seule fois, jamais après un auto-lock).
  let hasIdentity = $state(false);
  let bioEnabled = $state(false);
  let firstAuth = $state(true);

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
        "1": "wallet", "2": "contacts", "3": "dashboard", "4": "network", "5": "profile",
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
        // Retour à AuthGate : il remonte sur l'écran de déverrouillage (identité
        // existante) ; `firstAuth` est déjà faux → pas d'auto-offre biométrique.
        ready = false;
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
      // Reprise après rechargement GARDIEN uniquement (webview ressuscité) :
      // le vault Rust est resté chaud — pas d'écran de déverrouillage. Un
      // auto-lock volontaire ou un vrai redémarrage ne passent jamais par ici.
      const guardianReload = await wasGuardianReload().catch(() => false);
      if (guardianReload) {
        try {
          const a = await getPublicKey();
          if (a) {
            note("reprise", "session restaurée après rechargement gardien");
            pk = a;
            ready = true;
            loading = false;
            return;
          }
        } catch { /* vault verrouillé → flux normal */ }
      }
      const has = await checkIdentity();
      if (AUTOPILOT) {
        // Un seul essai, jamais de boucle : si le déverrouillage échoue (vault
        // réel ≠ vault jetable), on s'arrête net — pas de brute-force accidentel.
        try {
          const password = "quanta-dev-autopilot";
          const id = has
            ? await unlockIdentity(password)
            : await createIdentity("probe", password);
          pk = id.public_key_hex;
          ready = true;
          view = import.meta.env.VITE_QUANTA_AUTOPILOT_VIEW || "network";
          // Rotation de vues (sonde) : mime la navigation humaine pendant les
          // forges — chaque changement est daté dans l'anneau de la sonde.
          if (import.meta.env.VITE_QUANTA_AUTOPILOT_ROTATE === "1") {
            const cycle = ["network", "wallet", "network", "dashboard", "network", "profile"];
            let ci = 0;
            setInterval(() => { ci = (ci + 1) % cycle.length; view = cycle[ci]; }, 15000);
          }
          // Auto-test de la sonde : gel volontaire de 900 ms, 30 s après le boot
          // — prouve de bout en bout que le watchdog capture un vrai gel.
          if (import.meta.env.VITE_QUANTA_AUTOPILOT_SELFTEST === "1") {
            setTimeout(() => {
              const t0 = performance.now();
              while (performance.now() - t0 < 900) { /* gel volontaire (test) */ }
            }, 30000);
          }
        } catch (e) {
          console.error("[autopilot] arrêt:", e);
        }
        loading = false;
        return;
      }
      // Chemin normal : on transmet l'état à AuthGate (écrans d'auth).
      hasIdentity = has;
      if (has) {
        try {
          const st = await biometricStatus();
          bioEnabled = st.supported && st.enabled;
        } catch { /* biometry optional */ }
      }
    } catch { setTimeout(init, 800); return; }
    loading = false;
  }

  // Identité prête (déverrouillée / créée / @pseudo réglé) → entrer dans l'app.
  function enterApp(unlocked_pk: string) {
    pk = unlocked_pk;
    ready = true;
    firstAuth = false;
  }

  // Load the value address once unlocked — feeds the live toasts layer.
  $effect(() => {
    if (!ready) return;
    getPublicKey().then((a) => { myAddr = a; }).catch(() => {});
    // Pré-chauffe la route audio maintenant (geste utilisateur = déverrouillage)
    // — le réveil d'une interface externe (Universal Audio…) peut bloquer ~1-2 s
    // s'il arrive au moment du carillon de scellement.
    warmAudio();
  });

  function nav(v: string) { view = v; }
  // La vue courante entre dans l'anneau de la sonde (contexte des rapports de gel).
  $effect(() => {
    note("nav", view);
    (window as unknown as { __quantaView?: string }).__quantaView = view;
  });

  // Disjoncteur : `effect_update_depth_exceeded` (boucle d'effets) tue le
  // graphe réactif — la page devient une nature morte (LE « gel » du 19/07,
  // bloc #91). Ici : on capte l'erreur, on remonte TOUT le shell ({#key
  // appGen}) — un clignotement d'une seconde au lieu d'une app morte — et
  // l'événement part dans la forensique.
  let appGen = $state(0);
  let lastBreak = 0;
  $effect(() => {
    const breaker = (e: ErrorEvent) => {
      if (!String(e.message).includes("effect_update_depth")) return;
      const now = Date.now();
      if (now - lastBreak < 5000) return; // jamais en boucle
      lastBreak = now;
      note("disjoncteur", "remontage du shell après boucle d'effets");
      // setTimeout : on laisse le flush fautif s'aborter proprement d'abord.
      setTimeout(() => { appGen += 1; }, 50);
    };
    window.addEventListener("error", breaker);
    return () => window.removeEventListener("error", breaker);
  });

  // Bannière « gel détecté » : la sonde (diag.ts) émet quanta-stall à chaque
  // rapport — l'app MONTRE qu'elle a vu le gel et où lire le contexte.
  let stallFlash = $state("");
  let stallTo: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const h = (e: Event) => {
      stallFlash = String((e as CustomEvent).detail ?? "").slice(0, 80);
      clearTimeout(stallTo);
      stallTo = setTimeout(() => (stallFlash = ""), 8000);
    };
    window.addEventListener("quanta-stall", h);
    return () => { window.removeEventListener("quanta-stall", h); clearTimeout(stallTo); };
  });
  function handleCmd(id: string) { nav(id); }
</script>

{#if loading}
  <div class="load-screen">
    <div class="load-inner">
      <div class="load-mark"><QuantaMark size={46} tone="ink" /></div>
      <span class="load-logo">QUANTA</span>
      <span class="load-sub">{t('loading')}</span>
    </div>
  </div>
{:else if !ready}
  <AuthGate {hasIdentity} biometricEnabled={bioEnabled} autoBiometric={firstAuth} onReady={enterApp} />
{:else}
  {#key appGen}
  <div class="app-shell">
    <Sidebar activeView={view} onNavigate={nav} />
    <main class="main-content">
      {#key view}
        <div class="view-anim">
          <!-- Garde-fou : si un écran plante, IL s'affiche en erreur — la
               navigation, elle, ne peut plus jamais geler. -->
          <svelte:boundary onerror={(e) => console.error("[view]", e)}>
            {#if view === "wallet"}<Wallet />
            {:else if view === "contacts"}<Contacts onNavigate={nav} />
            {:else if view === "dashboard"}<Dashboard />
            {:else if view === "network"}<Network />
            {:else if view === "profile"}<Profile />
            {:else if view === "whitepaper"}<Whitepaper />
            {:else if view === "settings"}<Settings />
            {:else}<Wallet />{/if}
            {#snippet failed(error, reset)}
              <div class="page">
                <div class="card view-fail">
                  <div class="vf-t">Cet écran a rencontré une erreur</div>
                  <div class="vf-d">{String(error).slice(0, 300)}</div>
                  <button class="btn btn-primary" onclick={reset}>Réessayer</button>
                </div>
              </div>
            {/snippet}
          </svelte:boundary>
        </div>
      {/key}
    </main>
  </div>
  <CommandPalette isOpen={cmdOpen} onClose={() => cmdOpen = false} onCommand={handleCmd} />
  <HelpModal isOpen={helpOpen} onClose={() => helpOpen = false} />
  <Toasts myAddress={myAddr} />
  {#if stallFlash}
    <div class="stall-banner" role="status">⚠ {t('diag.stall')} · {stallFlash}</div>
  {/if}
  {/key}
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

  /* Layout handled by app.css .app-shell and .main-content */

  /* Bannière de gel — preuve visible que la sonde a capturé l'événement. */
  .stall-banner {
    position: fixed; bottom: 16px; left: 50%; transform: translateX(-50%);
    z-index: 300;
    padding: 9px 16px;
    background: var(--surface);
    border: 1px solid #f0b429;
    border-radius: 100px;
    box-shadow: var(--shadow-lg);
    font-size: 12.5px; font-weight: 600; color: var(--color-text-0);
    animation: toast-like-in 0.25s ease-out;
  }
  @keyframes toast-like-in {
    from { opacity: 0; transform: translateX(-50%) translateY(8px); }
    to   { opacity: 1; transform: translateX(-50%) translateY(0); }
  }
  @media (prefers-reduced-motion: reduce) { .stall-banner { animation: none; } }

  /* Écran en erreur (boundary) — sobre, actionnable. */
  .view-fail { max-width: 480px; display: flex; flex-direction: column; gap: 12px; }
  .vf-t { font-size: 16px; font-weight: 700; color: var(--color-text-0); }
  .vf-d { font-size: 12px; color: var(--color-text-2); word-break: break-word; }

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
