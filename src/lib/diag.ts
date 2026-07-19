// Sonde de diagnostic du thread UI — « voir ce qui se passe quand ça gèle ».
//
// Trois capteurs, tous best-effort et sans effet sur l'app :
//   1. Patch global de `__TAURI_INTERNALS__.invoke` : chaque commande est
//      chronométrée (nom, durée, taille de réponse) dans un anneau de contexte.
//   2. Watchdog 150 ms : un trou > 600 ms entre deux battements = thread UI
//      bloqué → rapport complet (gel + anneau) vers le log Rust (`ui_diag`),
//      la console et localStorage (`quanta.lastStall`).
//   3. PerformanceObserver "longtask" (si le moteur le supporte — WKWebView
//      ne l'expose pas toujours ; le watchdog reste le capteur principal).
//
// L'anneau enregistre aussi les événements Tauri du nœud (mined, block-sealed,
// tx-applied, engine…) : un gel corrélé au forge se lit directement.

import { listen } from "@tauri-apps/api/event";
import { lastChimeAgoMs } from "./sound";

type Entry = { t: number; k: string; d: string };

const RING_SIZE = 48;
const ring: Entry[] = [];
let ringPos = 0;
let rawInvoke: ((cmd: string, args?: unknown, opts?: unknown) => Promise<unknown>) | null = null;
let lastReportAt = 0;
let started = false;
// Dernière frame présentée (rAF) — partagé watchdog rendu + forensique.
let lastRaf = 0;

/// Forensique d'un scellement : 3 s d'échantillonnage à 100 ms, écrite SANS
/// condition — chaque bloc produit sa ligne de vérité mesurée.
function runSealForensics(index: number): void {
  const t0 = performance.now();
  let jsWorst = 0;
  let rafWorst = 0;
  let last = t0;
  const iv = setInterval(() => {
    const now = performance.now();
    jsWorst = Math.max(jsWorst, now - last - 100);
    last = now;
    if (lastRaf) rafWorst = Math.max(rafWorst, now - lastRaf);
    if (now - t0 >= 3000) {
      clearInterval(iv);
      const chime = lastChimeAgoMs();
      // Battement de la scène 3D : vivante (âge de sa dernière frame),
      // absente (page sans scène) ou MORTE (montée mais figée) — le composant
      // peut geler seul pendant que le pipeline global reste fluide.
      const sb = (window as unknown as { __quantaSceneBeat?: number }).__quantaSceneBeat;
      const scene =
        sb === undefined ? "jamais-montée"
        : sb === -1 ? "absente"
        : `${Math.round(now - sb)}ms`;
      const recent = ring
        .filter((e) => now - e.t < 3500)
        .map((e) => `${e.k}:${e.d}`)
        .join(" | ");
      const line =
        `FORENSIQUE bloc #${index} : pire_trou_js=${Math.round(jsWorst)}ms ` +
        `pire_trou_rendu=${Math.round(rafWorst)}ms scène=${scene} ` +
        `carillon=${chime >= 0 ? `${chime}ms` : "jamais"} :: ${recent}`.slice(0, 900);
      try { localStorage.setItem("quanta.lastSeal", `${new Date().toISOString()} ${line}`); } catch { /* best-effort */ }
      try { void rawInvoke?.("ui_diag", { msg: line }); } catch { /* best-effort */ }
      note("forensique", `#${index} js=${Math.round(jsWorst)} rendu=${Math.round(rafWorst)}`);
    }
  }, 100);
}

/** Note une opération dans l'anneau de contexte (jamais bloquant, jamais réactif). */
export function note(kind: string, detail: string): void {
  try {
    const e = { t: performance.now(), k: kind, d: detail };
    if (ring.length < RING_SIZE) ring.push(e);
    else { ring[ringPos] = e; ringPos = (ringPos + 1) % RING_SIZE; }
  } catch { /* la sonde ne casse jamais l'app */ }
}

function ringDump(): string {
  const ordered = ring.length < RING_SIZE
    ? ring
    : ring.slice(ringPos).concat(ring.slice(0, ringPos));
  const now = performance.now();
  return ordered
    .map((e) => `-${((now - e.t) / 1000).toFixed(2)}s ${e.k}:${e.d}`)
    .join(" | ");
}

function sizeOf(r: unknown): string {
  if (typeof r === "string") return ` ${r.length}c`;
  if (Array.isArray(r)) return ` ${r.length}el`;
  return "";
}

/** Rapport de gel : contexte complet vers Rust + console + localStorage. */
function report(source: string, ms: number): void {
  const now = performance.now();
  if (now - lastReportAt < 3000) return; // anti-spam : 1 rapport / 3 s max
  lastReportAt = now;
  const view = (window as unknown as { __quantaView?: string }).__quantaView ?? "?";
  const msg = `GEL ${Math.round(ms)} ms (${source}) vue=${view} :: ${ringDump()}`;
  console.error("[diag]", msg);
  try { localStorage.setItem("quanta.lastStall", `${new Date().toISOString()} ${msg}`); } catch { /* best-effort */ }
  try { void rawInvoke?.("ui_diag", { msg }); } catch { /* best-effort */ }
  // Signal visible : le shell affiche une bannière « gel détecté et enregistré »
  // — l'app prouve qu'elle a vu le gel, l'utilisateur sait où lire le rapport.
  try { window.dispatchEvent(new CustomEvent("quanta-stall", { detail: `${Math.round(ms)} ms (${source})` })); } catch { /* best-effort */ }
}

/** Dernier gel enregistré (affichable dans le terminal du moteur). */
export function lastStall(): string | null {
  try { return localStorage.getItem("quanta.lastStall"); } catch { return null; }
}

/** Démarre la sonde (idempotent). À appeler une fois au boot du shell. */
export function startDiag(): void {
  if (started) return;
  started = true;

  // ── 1. Patch global d'invoke : toute commande passe par ici.
  // @tauri-apps/api résout `window.__TAURI_INTERNALS__.invoke` À L'APPEL, donc
  // remplacer cette propriété suffit. Si l'objet est gelé (Object.freeze),
  // l'affectation jette en mode strict → repli : clone + remplacement de la
  // propriété window entière. Le ping de démarrage rapporte l'état du patch.
  let patchState = "off";
  try {
    type Internals = Record<string, unknown> & {
      invoke: NonNullable<typeof rawInvoke>;
      __diagWrapped?: boolean;
    };
    const w = window as unknown as { __TAURI_INTERNALS__?: Internals };
    const internals = w.__TAURI_INTERNALS__;
    if (internals?.invoke && !internals.__diagWrapped) {
      const orig = internals.invoke.bind(internals) as NonNullable<typeof rawInvoke>;
      rawInvoke = orig;
      const wrapped = ((cmd: string, args?: unknown, opts?: unknown) => {
        const t0 = performance.now();
        const p = orig(cmd, args, opts);
        (p as Promise<unknown>).then(
          (r) => { if (cmd !== "ui_diag") note("cmd", `${cmd} ${Math.round(performance.now() - t0)}ms${sizeOf(r)}`); },
          () => note("cmd✗", `${cmd} ${Math.round(performance.now() - t0)}ms`),
        );
        return p;
      }) as Internals["invoke"];
      try {
        internals.invoke = wrapped;
        internals.__diagWrapped = true;
        patchState = "direct";
      } catch {
        const clone: Internals = Object.assign(
          Object.create(Object.getPrototypeOf(internals) as object) as Internals,
          internals,
          { invoke: wrapped, __diagWrapped: true },
        );
        w.__TAURI_INTERNALS__ = clone;
        patchState = "clone";
      }
    } else if (internals?.__diagWrapped) {
      patchState = "déjà";
    }
  } catch { /* sans patch, watchdog + événements restent actifs */ }

  // Ping de démarrage : confirme (côté log Rust) que la sonde est active,
  // sur quelle vue, et si le patch d'invoke intercepte (direct/clone/off).
  setTimeout(() => {
    const view = (window as unknown as { __quantaView?: string }).__quantaView ?? "?";
    try { void rawInvoke?.("ui_diag", { msg: `sonde active vue=${view} patch=${patchState}` }); } catch { /* best-effort */ }
  }, 4000);

  // ── 2. Watchdog : trou entre deux battements = thread bloqué.
  // Un seul long blocage (>600 ms) OU une TEMPÊTE de micro-blocages (somme
  // des retards >800 ms sur 3 s glissantes — chacun sous le seuil, l'ensemble
  // très visible) sont rapportés : c'est la signature du « ça freeze » perçu.
  let beat = performance.now();
  let overrun = 0;
  let stormStart = performance.now();
  setInterval(() => {
    const n = performance.now();
    const gap = n - beat;
    beat = n;
    // Fenêtre cachée/occluse : macOS ralentit les timers à ~1 s — des trous
    // NORMAUX, pas des gels. On se tait (sinon : spam « GEL 1000 ms » dans le
    // journal de preuve pendant toute absence — observé le 19/07 au soir).
    if (document.hidden || document.visibilityState !== "visible") {
      overrun = 0;
      stormStart = n;
      return;
    }
    if (gap > 600) report("watchdog", gap);
    else if (gap - 150 > 40) overrun += gap - 150;
    if (n - stormStart >= 3000) {
      if (overrun > 800) report("tempête-jank", overrun);
      overrun = 0;
      stormStart = n;
    }
  }, 150);

  // ── 2b. Battement de cœur vers Rust : le gardien backend détecte un webview
  // MORT (WebContent tué → gel permanent, plus aucun JS) et le recharge.
  setInterval(() => { try { void rawInvoke?.("ui_beat"); } catch { /* best-effort */ } }, 5000);

  // ── 2c. rAF-liveness : le fil JS peut battre pendant que les FRAMES sont
  // figées (compositeur/GPU/occlusion). Si aucune frame n'est présentée
  // pendant > 3 s alors que la page se dit visible → gel de RENDU, rapporté
  // une fois par épisode (le rAF repartant réarme).
  lastRaf = performance.now();
  let rafReported = false;
  const rafBeat = () => { lastRaf = performance.now(); rafReported = false; requestAnimationFrame(rafBeat); };
  try { requestAnimationFrame(rafBeat); } catch { /* best-effort */ }
  setInterval(() => {
    if (document.hidden) { lastRaf = performance.now(); return; }
    const gap = performance.now() - lastRaf;
    if (gap > 1200 && !rafReported) {
      rafReported = true;
      report("rendu-rAF", gap);
    }
  }, 500);

  // ── 5. Forensique par scellement — la demande d'Alex : « des logs pour
  // comprendre ». À CHAQUE bloc scellé, fenêtre de 3 s échantillonnée à
  // 100 ms, TOUJOURS écrite (aucun seuil) : pire trou JS, pire trou de rendu,
  // battement de la scène 3D, délai depuis le carillon, anneau récent.
  listen<{ index?: number }>("quanta://block-sealed", (e) => {
    try { runSealForensics(e.payload?.index ?? -1); } catch { /* best-effort */ }
  }).catch(() => { /* best-effort */ });

  // ── 6. Erreurs globales : une exception dans un callback rAF/timer tue sa
  // boucle EN SILENCE (aucun boundary ne la voit) — ici elle entre dans
  // l'anneau ET déclenche un rapport. Plus jamais de mort silencieuse.
  window.addEventListener("error", (e) => {
    const stack = (e.error as Error | undefined)?.stack ?? "";
    note("js-erreur", `${String(e.message).slice(0, 100)} @${String(e.filename).split("/").pop()}:${e.lineno} ${stack.slice(0, 300)}`);
    report("js-erreur", 0);
  });
  window.addEventListener("unhandledrejection", (e) => {
    note("promesse-rejetée", String(e.reason).slice(0, 100));
  });

  // ── 3. Longtasks (moteurs qui l'exposent) ──
  try {
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) {
        note("longtask", `${Math.round(e.duration)}ms`);
        if (e.duration > 300) report("longtask", e.duration);
      }
    }).observe({ entryTypes: ["longtask"] });
  } catch { /* WKWebView : non supporté — le watchdog couvre */ }

  // ── 4. Événements du nœud, datés dans l'anneau (corrélation gel ↔ forge) ──
  for (const ev of [
    "quanta://mined",
    "quanta://block-sealed",
    "quanta://tx-applied",
    "quanta://chain-sync-progress",
    "quanta://engine",
  ]) {
    listen(ev, (e) => {
      try { note("evt", `${ev.slice(9)} ${JSON.stringify(e.payload).slice(0, 90)}`); } catch { /* best-effort */ }
    }).catch(() => { /* best-effort */ });
  }
}
