// Shared wallet helpers — payment URIs, exact µQTA math, display formatting.
//
// The `quanta:` payment URI follows the BIP-21 shape every wallet user already
// knows from Bitcoin (BlueWallet, Electrum…): `quanta:<recipient>?amount=<QUANTA>`.
// Recipient is either a @username or a 64-hex ML-DSA address. QUANTA lives on
// its own network — the URI *format* is the familiar standard, the coin never
// crosses onto Bitcoin rails.

/** One QUANTA = 1_000_000 µQTA (mirror of the backend MICRO constant). */
export const MICRO = 1_000_000;

/** A parsed payment request: recipient (@user or 64-hex) + optional amount. */
export interface PaymentRequest {
  /** `@username` (with the @) or a 64-hex address, as typed. */
  to: string;
  /** Amount in QUANTA, or null when the URI carries none. */
  amount: number | null;
}

const HEX64 = /^[0-9a-fA-F]{64}$/;
// Public `qta1…` address shape (Bech32m over a 32-byte address): `qta1` + 58
// charset symbols = 62 chars. This is a fast *shape* filter — the authoritative
// checksum validation lives in the backend (`validate_address` / `ledger_transfer`).
const BECH32_ADDR = /^qta1[qpzry9x8gf2tvdw0s3jn54khce6mua7l]{58}$/i;
const USERNAME = /^[a-z][a-z0-9_]{2,19}$/;

/** True when `s` looks like an account address — either the public `qta1…`
 * (Bech32m) form or the canonical 64-hex form. */
export function isAddress(s: string): boolean {
  const t = s.trim();
  return HEX64.test(t) || BECH32_ADDR.test(t);
}

/** True when `s` (with or without @) is a plausible username. */
export function isUsername(s: string): boolean {
  return USERNAME.test(s.trim().replace(/^@/, "").toLowerCase());
}

/**
 * Build a `quanta:` payment URI for a recipient and optional amount.
 * `to` may be `@user`, `user`, or a 64-hex address. Amounts are emitted with
 * up to 6 decimals (µQTA precision), trailing zeros trimmed.
 */
export function formatPaymentUri(to: string, amount?: number | null): string {
  const raw = to.trim();
  const target = isAddress(raw) ? raw.toLowerCase() : "@" + raw.replace(/^@/, "").toLowerCase();
  if (amount != null && isFinite(amount) && amount > 0) {
    const q = (Math.round(amount * MICRO) / MICRO).toFixed(6).replace(/\.?0+$/, "");
    return `quanta:${target}?amount=${q}`;
  }
  return `quanta:${target}`;
}

/**
 * Parse anything a user might paste into the "recipient" field:
 * a full `quanta:` URI, a bare `@username`, or a bare 64-hex address.
 * Returns null when the input matches none of those shapes.
 */
export function parsePaymentUri(raw: string): PaymentRequest | null {
  let s = raw.trim();
  if (!s) return null;
  let amount: number | null = null;
  if (s.toLowerCase().startsWith("quanta:")) {
    s = s.slice("quanta:".length).replace(/^\/\//, "");
    const qm = s.indexOf("?");
    if (qm >= 0) {
      const query = s.slice(qm + 1);
      s = s.slice(0, qm);
      for (const pair of query.split("&")) {
        const [k, v] = pair.split("=");
        if (k === "amount" && v !== undefined) {
          const a = parseFloat(decodeURIComponent(v));
          if (isFinite(a) && a > 0) amount = a;
        }
      }
    }
  }
  s = s.trim();
  if (isAddress(s)) return { to: s.toLowerCase(), amount };
  if (isUsername(s)) return { to: "@" + s.replace(/^@/, "").toLowerCase(), amount };
  return null;
}

/**
 * Exact transfer split, integer µQTA math identical to the ledger:
 * `burn = floor(gross_µQTA / 100)`, `net = gross − burn`. Never floats on the
 * wire — this is only the *preview* of what the chain will do.
 */
export function splitTransfer(amountQ: number): { gross: number; net: number; burn: number } {
  const grossMicro = Math.round(amountQ * MICRO);
  const burnMicro = Math.floor(grossMicro / 100);
  return {
    gross: grossMicro / MICRO,
    net: (grossMicro - burnMicro) / MICRO,
    burn: burnMicro / MICRO,
  };
}

/** Locale-aware QUANTA amount for display (2..6 decimals). */
export function fmtQ(n: number, locale = "fr-FR"): string {
  return n.toLocaleString(locale, { minimumFractionDigits: 2, maximumFractionDigits: 6 });
}

/** Shorten a 64-hex address for lists: `a3f7b2…9c4d`. */
export function shortAddr(s: string): string {
  return s.length > 14 ? s.slice(0, 6) + "…" + s.slice(-4) : s;
}

/**
 * Copy a SENSITIVE string (recovery phrase, private material) and schedule a
 * best-effort clipboard wipe: after `ttlMs`, if the clipboard still holds
 * exactly this text (the user hasn't copied anything else), it is cleared.
 * If the platform forbids reading the clipboard, we leave it untouched
 * rather than risk destroying something the user copied since.
 */
export async function copySensitive(text: string, ttlMs = 45_000): Promise<void> {
  await navigator.clipboard.writeText(text);
  setTimeout(async () => {
    try {
      const cur = await navigator.clipboard.readText();
      if (cur === text) await navigator.clipboard.writeText("");
    } catch {
      /* clipboard read unavailable — do nothing */
    }
  }, ttlMs);
}

/**
 * Human ETA for `blocks` remaining, from the ~2 min/block seal cadence
 * (SEAL_EVERY_N_TICKS × MINE_INTERVAL). Approximate by design — the chain
 * only advances while leaders seal — so it is worded as "≈".
 */
export function blocksToEta(blocks: number): { days: number; hours: number; minutes: number } {
  const mins = blocks * 2;
  return {
    days: Math.floor(mins / 1440),
    hours: Math.floor((mins % 1440) / 60),
    minutes: Math.floor(mins % 60),
  };
}
