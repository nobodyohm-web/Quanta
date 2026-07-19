// ═══════════════════════════════════════════════════════════════════════════
//  stores.svelte.ts — état partagé + UN sondage par donnée.
//
//  Problème résolu : avant, `get_node_status` (entre autres) était re-fetché
//  indépendamment par Wallet, Réseau, Minage… chacun avec son propre
//  `setInterval`. Ici, chaque donnée vivante a UN store runes au niveau module
//  (survit aux démontages : les vues sont dans `{#key view}` donc remontées à
//  chaque navigation — le store garde la donnée CHAUDE, fini le « 0 » au retour
//  sur un écran) et UN `setInterval` unique, démarré/arrêté par comptage de
//  références (refcount) : le sondage ne tourne QUE tant qu'au moins un écran
//  monté est abonné.
//
//  Contrat par store : `.value` (T | null), `.loaded` (latch au 1ᵉʳ succès —
//  pilote l'affichage « — »), `.error` (dernier tick en échec — pilote la ligne
//  d'erreur discrète), `.refresh()` (re-fetch impératif après une action), et
//  `.subscribe()` (renvoie la fonction de désabonnement — à retourner tel quel
//  depuis un `$effect` de composant).
//
//  Sûreté « 0 perte » : en cas d'échec d'un tick, la dernière valeur bonne est
//  CONSERVÉE (jamais écrasée) — l'écran garde ses chiffres + montre l'erreur,
//  exactement comme les refresh locaux le faisaient avant.
// ═══════════════════════════════════════════════════════════════════════════
import {
  getNodeStatus,
  getChainOverview,
  getFinalityStatus,
  getWalletOverview,
  getRecentTxs,
  getEconomyStats,
  getMyReputation,
  getPeerMetrics,
  getMyUsername,
  getMyConnectionCode,
  type NodeStatus,
  type ChainOverview,
  type FinalityStatus,
  type WalletOverview,
  type LedgerTx,
  type EconomyStats,
  type Reputation,
  type PeerMetric,
} from "./api";

// ─── Cadences de sondage (une seule source, nommées + commentées) ───────────
/** Chaîne vive : hauteur, blocs récents, plancher de finalité (l'écran Réseau
 *  animait déjà la chaîne à ce rythme). */
export const POLL_CHAIN_MS = 1_500;
/** Statut du nœud, émission, réputation — chiffres à évolution lente. */
export const POLL_STATUS_MS = 5_000;
/** Portefeuille : solde on-chain + transactions récentes. */
export const POLL_WALLET_MS = 15_000;
/** Identité : @pseudo + code de connexion (quasi statiques). */
export const POLL_IDENTITY_MS = 10_000;

// ─── Rattrapage au retour de fond ───────────────────────────────────────────
// macOS gèle les timers JS d'une fenêtre occluse (le backend Rust, lui, continue
// grâce à `prevent_app_nap`). Au retour, sans ceci, l'écran garderait des
// chiffres périmés jusqu'au prochain tick (jusqu'à 15 s pour le wallet) : on
// re-fetch immédiatement chaque store actif dès que la fenêtre redevient visible.
const activeTicks = new Set<() => void>();
if (typeof document !== "undefined") {
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") {
      for (const tick of activeTicks) tick();
    }
  });
}

/** Un store sondé : `.value` chaud, refcount → UN interval, dernière valeur
 *  bonne conservée en cas d'échec. */
export interface PolledStore<T> {
  readonly value: T | null;
  readonly loaded: boolean;
  readonly error: boolean;
  refresh(): Promise<void>;
  subscribe(): () => void;
}

function makePolledStore<T>(fetcher: () => Promise<T>, intervalMs: number): PolledStore<T> {
  let value = $state<T | null>(null);
  let loaded = $state(false);
  let error = $state(false);
  let refs = 0;
  let iv: ReturnType<typeof setInterval> | null = null;
  // Référence stable pour le registre de rattrapage visibilitychange.
  const catchUp = (): void => void tick();

  async function tick(): Promise<void> {
    try {
      value = await fetcher(); // succès → on remplace la valeur
      loaded = true;
      error = false;
    } catch {
      // Échec : on GARDE la dernière valeur bonne (0 perte), on signale l'erreur.
      error = true;
    }
  }

  return {
    get value() {
      return value;
    },
    get loaded() {
      return loaded;
    },
    get error() {
      return error;
    },
    refresh: tick,
    subscribe() {
      refs += 1;
      if (refs === 1) {
        void tick(); // premier abonné → charge tout de suite puis lance l'interval
        iv = setInterval(() => void tick(), intervalMs);
        activeTicks.add(catchUp);
      }
      let released = false;
      return () => {
        if (released) return; // idempotent : un cleanup ne décrémente qu'une fois
        released = true;
        refs -= 1;
        if (refs === 0 && iv) {
          clearInterval(iv);
          iv = null;
          activeTicks.delete(catchUp);
        }
      };
    },
  };
}

// ─── Les stores (un par donnée) ─────────────────────────────────────────────

/** Statut du nœud — Wallet · Réseau · Minage (4 consommateurs avant, 1 sondage). */
export const nodeStatus: PolledStore<NodeStatus> = makePolledStore(getNodeStatus, POLL_STATUS_MS);

/** Résumé de chaîne (hauteur, blocs, offre) — Réseau (limit 22) · Minage. */
export const chainOverview: PolledStore<ChainOverview> = makePolledStore(
  () => getChainOverview(22),
  POLL_CHAIN_MS,
);

/** Finalité Casper-FFG vivante — Réseau · Minage. */
export const finalityStatus: PolledStore<FinalityStatus> = makePolledStore(
  getFinalityStatus,
  POLL_CHAIN_MS,
);

/** Métriques par pair (NET-9/10) — Réseau. */
export const peerMetrics: PolledStore<PeerMetric[]> = makePolledStore(getPeerMetrics, POLL_STATUS_MS);

/** Vérité on-chain du portefeuille — Wallet. */
export const walletOverview: PolledStore<WalletOverview> = makePolledStore(
  getWalletOverview,
  POLL_WALLET_MS,
);

/** Transactions récentes — Wallet. */
export const recentTxs: PolledStore<LedgerTx[]> = makePolledStore(getRecentTxs, POLL_WALLET_MS);

/** Émission réelle décroissante — Minage. */
export const economyStats: PolledStore<EconomyStats> = makePolledStore(
  getEconomyStats,
  POLL_STATUS_MS,
);

/** Contribution + identité brute — Minage · Profil. */
export const myReputation: PolledStore<Reputation> = makePolledStore(getMyReputation, POLL_STATUS_MS);

/** @pseudo — Wallet · Proches · Sidebar · Profil (4 sondages avant, 1 maintenant). */
export const myUsername: PolledStore<string | null> = makePolledStore(getMyUsername, POLL_IDENTITY_MS);

/** Code de connexion — Wallet · Proches · Profil. */
export const myConnectionCode: PolledStore<string> = makePolledStore(
  getMyConnectionCode,
  POLL_IDENTITY_MS,
);
