// Generated micro-sounds — zero assets, zero network. A shared AudioContext
// synthesizes three tiny, tasteful cues (Apple-like: felt more than heard):
//   forge()   — a soft two-partial chime when a mining reward lands
//   seal()    — a low felt "thunk" + shimmer when a block is sealed
//   receive() — a brighter ascending ding when money arrives
// Gated by the `sound` preference; volumes stay well under attention level.

import { getPrefs } from "./prefs";

let ctx: AudioContext | null = null;
let keepalive: ReturnType<typeof setInterval> | null = null;
let lastChime = 0;

/** Il y a combien de ms le dernier carillon a-t-il joué ? (-1 = jamais).
 *  Nourrit la forensique de scellement : corrélation carillon ⇄ gel. */
export function lastChimeAgoMs(): number {
  return lastChime ? Math.round(performance.now() - lastChime) : -1;
}

/** Pré-chauffe la route audio (à appeler après le déverrouillage, dans un
 *  contexte de geste utilisateur). Le réveil d'une interface audio externe
 *  (ex. Universal Audio) peut bloquer la sortie ~1-2 s — mieux vaut le payer
 *  une fois au boot qu'à chaque scellement. */
export function warmAudio(): void {
  void audio();
}

function audio(): AudioContext | null {
  if (!getPrefs().sound) return null;
  try {
    if (!ctx) {
      ctx = new AudioContext();
      // Maintien de route : un échantillon à gain nul toutes les 25 s empêche
      // macOS/le driver d'endormir la sortie entre deux carillons (2 min) —
      // sinon chaque réveil de route peut se percevoir comme un gel au bloc.
      if (!keepalive) {
        keepalive = setInterval(() => {
          try {
            if (!ctx || ctx.state !== "running" || !getPrefs().sound) return;
            const b = ctx.createBuffer(1, 1, ctx.sampleRate);
            const s = ctx.createBufferSource();
            s.buffer = b;
            const g = ctx.createGain();
            g.gain.value = 0;
            s.connect(g).connect(ctx.destination);
            s.start();
          } catch { /* best-effort */ }
        }, 25000);
      }
    }
    if (ctx.state === "suspended") void ctx.resume();
    return ctx;
  } catch {
    return null;
  }
}

/** One decaying sine partial. */
function partial(
  ac: AudioContext,
  freq: number,
  at: number,
  dur: number,
  peak: number,
  type: OscillatorType = "sine",
) {
  const osc = ac.createOscillator();
  const gain = ac.createGain();
  osc.type = type;
  osc.frequency.value = freq;
  gain.gain.setValueAtTime(0, at);
  gain.gain.linearRampToValueAtTime(peak, at + 0.012);
  gain.gain.exponentialRampToValueAtTime(0.0001, at + dur);
  osc.connect(gain).connect(ac.destination);
  osc.start(at);
  osc.stop(at + dur + 0.05);
}

/** Mining reward — soft major-third chime (E6+G#6), barely-there. */
export function forge(): void {
  const ac = audio();
  if (!ac) return;
  lastChime = performance.now();
  const t = ac.currentTime;
  partial(ac, 1318.5, t, 0.55, 0.05);
  partial(ac, 1661.2, t + 0.04, 0.5, 0.035);
}

/** Block sealed — low felt thunk + a faint fifth shimmer above. */
export function seal(): void {
  const ac = audio();
  if (!ac) return;
  lastChime = performance.now();
  const t = ac.currentTime;
  partial(ac, 174.6, t, 0.4, 0.09, "triangle");
  partial(ac, 261.6, t + 0.06, 0.45, 0.03);
  partial(ac, 1046.5, t + 0.1, 0.35, 0.02);
}

/** Money received — bright ascending pair (C6→E6), the happy one. */
export function receive(): void {
  const ac = audio();
  if (!ac) return;
  lastChime = performance.now();
  const t = ac.currentTime;
  partial(ac, 1046.5, t, 0.4, 0.06);
  partial(ac, 1318.5, t + 0.09, 0.5, 0.06);
  partial(ac, 2093.0, t + 0.09, 0.35, 0.02);
}
