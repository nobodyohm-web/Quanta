// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
import adapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter({
      fallback: "index.html",
    }),
    // A13 (audit 2026-08-13) — CSP : supprimer `'unsafe-inline'` de `script-src`
    // sans casser l'application.
    //
    // La CSP de `tauri.conf.json` portait `script-src 'self' 'unsafe-inline'`, ce
    // qui annule la protection anti-XSS : tout script injecté s'exécute. On ne
    // peut pas simplement retirer le mot-clé — SvelteKit émet un `<script>` inline
    // pour amorcer l'hydratation, et Tauri ne pose de nonce que sur les balises
    // `script[src^='http']`, jamais sur les scripts inline. Retirer le mot-clé à
    // l'aveugle donnerait un écran blanc.
    //
    // La réponse est ici : en mode `hash`, SvelteKit calcule le SHA-256 de CHACUN
    // de ses scripts inline et émet une balise `<meta http-equiv=
    // "content-security-policy">` qui ne les autorise que par empreinte. Les deux
    // politiques (l'en-tête de Tauri et ce meta) s'appliquent en **intersection**,
    // donc la plus stricte gagne : les scripts légitimes de SvelteKit passent par
    // leur hash, et un script injecté — dont l'empreinte n'est évidemment pas dans
    // la liste — est refusé, `'unsafe-inline'` de l'en-tête ou pas.
    //
    // À vérifier au prochain build : ouvrir l'application et confirmer l'absence
    // de violation CSP dans la console du webview.
    csp: {
      mode: "hash",
      directives: {
        "script-src": ["self"],
        "object-src": ["none"],
        "base-uri": ["self"],
        "frame-ancestors": ["none"],
      },
    },
  },
};

export default config;
