// ═══════════════════════════════════════════════════════════════════════════
//  errors.ts — translate a backend error into the active locale.
//
//  Tauri commands return `Result<T, String>`; user-facing failures now carry a
//  STABLE machine code (`err.<camelCase>`, with an optional `:param`) produced by
//  the Rust `CmdError` enum (src-tauri/src/commands/error.rs). This helper maps
//  such a code to `t('err.<code>')`, interpolating the `{n}` placeholder with the
//  parameter when present (e.g. `err.rateLimited:37` → "…try again in 37 s").
//
//  Anything that is NOT a recognized code — unexpected/technical errors such as
//  a poisoned lock, IO, serde, or internal corruption — falls through untouched:
//  the raw message, or the caller's own fallback string if one is supplied. This
//  keeps the machine surface (rpc.rs, low-level failures) readable while every
//  translated, user-facing failure reads in the user's language.
// ═══════════════════════════════════════════════════════════════════════════
import { t, type TKey } from "./i18n.svelte";

/** `err.camelCase` with an optional `:param` payload (e.g. `err.rateLimited:37`). */
const ERR_CODE = /^(err\.[a-zA-Z]+)(?::(.+))?$/;

/**
 * Turn a caught backend error into a localized, user-facing string.
 *
 * Tauri rejects an `invoke(...)` with the command's error value directly, so `e`
 * is usually the raw `String` (occasionally an `Error`); both are handled.
 *
 * @param e        the value thrown / rejected by an `invoke(...)` call.
 * @param fallback message shown when `e` is NOT a recognized `err.*` code;
 *                 defaults to the raw error text (passthrough for the unexpected).
 */
export function translateError(e: unknown, fallback?: string): string {
  const raw = (e instanceof Error ? e.message : String(e)).replace(/^Error:\s*/, "");
  const m = ERR_CODE.exec(raw);
  if (!m) return fallback ?? raw;
  const [, key, param] = m;
  const translated = t(key as TKey);
  // t() returns the key itself when it is absent from every dictionary — in that
  // (shouldn't-happen) case prefer the caller's fallback, else the raw text.
  if (translated === key) return fallback ?? raw;
  return param !== undefined ? translated.replace(/\{n\}/g, param) : translated;
}
