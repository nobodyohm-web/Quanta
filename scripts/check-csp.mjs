#!/usr/bin/env node
// A13 (audit 2026-08-13) — garde-fou de la CSP après build.
//
// La CSP en mode `hash` de SvelteKit ne hashe QUE les scripts qu'il génère
// lui-même. Un `<script>` inline ajouté à `src/app.html` — c'est exactement ce
// qui est arrivé avec l'anti-flash de thème — se retrouve donc refusé par
// l'intersection des deux politiques (en-tête Tauri × meta SvelteKit), sans que
// rien n'échoue au build : le symptôme n'apparaît qu'à l'exécution, dans la
// console du webview, sous la forme d'un thème qui ne s'applique plus.
//
// Ce script relit le bundle produit et vérifie que CHAQUE script inline du
// document est couvert par un hash de la politique. Il échoue sinon. C'est la
// seule façon de garder A13 vrai après coup : la propriété n'est pas dans le
// code source, elle est dans le fichier produit.
import { createHash } from "node:crypto";
import { readFileSync, existsSync } from "node:fs";

const INDEX = "build/index.html";

if (!existsSync(INDEX)) {
  console.error(`✗ ${INDEX} absent — lancer \`npm run build\` d'abord.`);
  process.exit(1);
}

const html = readFileSync(INDEX, "utf8");
const meta = html.match(/<meta http-equiv="content-security-policy" content="([^"]*)"/i);

if (!meta) {
  console.error("✗ aucune balise meta CSP dans le bundle : le mode `hash` de svelte.config.js ne s'applique plus.");
  process.exit(1);
}

const csp = meta[1];
if (csp.includes("unsafe-inline")) {
  console.error("✗ la CSP produite contient 'unsafe-inline' — A13 est annulé.");
  process.exit(1);
}

let blocked = 0;
for (const m of html.matchAll(/<script([^>]*)>([\s\S]*?)<\/script>/gi)) {
  const [, attrs, body] = m;
  if (/\ssrc=/.test(attrs)) continue; // couvert par 'self'
  const digest = "sha256-" + createHash("sha256").update(body, "utf8").digest("base64");
  if (!csp.includes(digest)) {
    console.error(`✗ script inline non autorisé par la CSP (${digest}) :`);
    console.error(body.slice(0, 200).trim());
    blocked++;
  }
}

if (blocked > 0) {
  console.error(
    `\n${blocked} script(s) inline seraient bloqués à l'exécution.\n` +
      "Corriger en déplaçant le code dans `static/` et en le chargeant par `<script src>` : " +
      "`'self'` est déjà dans les deux politiques et ne se périme pas, contrairement à un hash écrit à la main."
  );
  process.exit(1);
}

console.log(`✓ CSP : tous les scripts du bundle passent (${csp})`);
