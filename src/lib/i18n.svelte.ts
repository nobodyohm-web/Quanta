// Lightweight i18n for Quanta — a Svelte 5 runes module (.svelte.ts).
//
// The active locale is reactive ($state) and persisted via prefs, so any
// component that calls t() re-renders when the language changes. t() falls back
// to French, then to the key itself, so a missing translation never throws.
//
// Migration strategy: move hardcoded strings into the dictionaries below, one
// surface at a time. The onboarding (Welcome) is the first migrated surface.

import { getPrefs, setPrefs, type Locale } from "./prefs";

// French is the source of truth for the key set. `satisfies` keeps the literal
// key union so t() and the EN dictionary are compile-time checked.
const fr = {
  "loading": "Chargement…",
  "lang.fr": "Français",
  "lang.en": "English",
  "welcome.headline": "Le Web sans serveur,<br/>la monnaie sans banque.",
  "welcome.sub": "Publiez · cherchez · récompensez. <br/> Aucun cloud. Aucun intermédiaire. Vous êtes le serveur.",
  "welcome.password": "Mot de passe (min. 8 caractères)",
  "welcome.confirm": "Confirmer le mot de passe",
  "welcome.pseudo": "Pseudo (optionnel — auto si vide)",
  "welcome.start": "Démarrer en 1 clic",
  "welcome.creating": "Création…",
  "welcome.advanced": "Options avancées (pseudo, confirmation)",
  "welcome.haveIdentity": "J'ai déjà une identité",
  "welcome.securityNote": "Identité chiffrée localement (Argon2id + AES-256-GCM, signature Ed25519).<br/>À l'étape suivante, vous sauvegarderez votre <b>clé de récupération</b> — l'unique moyen de restaurer votre compte.",
  "welcome.errPass": "Mot de passe : minimum 8 caractères",
  "welcome.errMismatch": "Les mots de passe ne correspondent pas",
  "welcome.errCreate": "Erreur lors de la création",
} satisfies Record<string, string>;

export type TKey = keyof typeof fr;

const en: Record<TKey, string> = {
  "loading": "Loading…",
  "lang.fr": "Français",
  "lang.en": "English",
  "welcome.headline": "The web without a server,<br/>money without a bank.",
  "welcome.sub": "Publish · search · earn. <br/> No cloud. No intermediary. You are the server.",
  "welcome.password": "Password (min. 8 characters)",
  "welcome.confirm": "Confirm password",
  "welcome.pseudo": "Username (optional — auto if empty)",
  "welcome.start": "Start in 1 click",
  "welcome.creating": "Creating…",
  "welcome.advanced": "Advanced options (username, confirmation)",
  "welcome.haveIdentity": "I already have an identity",
  "welcome.securityNote": "Identity encrypted locally (Argon2id + AES-256-GCM, Ed25519 signature).<br/>In the next step you'll save your <b>recovery key</b> — the only way to restore your account.",
  "welcome.errPass": "Password: minimum 8 characters",
  "welcome.errMismatch": "Passwords do not match",
  "welcome.errCreate": "Error while creating the identity",
};

const DICTS: Record<Locale, Record<TKey, string>> = { fr, en };

let current = $state<Locale>(getPrefs().locale);

/** The active locale (reactive). */
export function locale(): Locale {
  return current;
}

/** Switch language and persist the choice. */
export function setLocale(l: Locale): void {
  current = l;
  setPrefs({ ...getPrefs(), locale: l });
}

/** Translate a key for the active locale, falling back to FR then the key. */
export function t(key: TKey): string {
  return DICTS[current][key] ?? fr[key] ?? key;
}
