<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  // ─── Types ──────────────────────────────────────────────────────────────
  type BlockType =
    | "heading" | "paragraph" | "quote" | "list" | "code"
    | "image" | "gallery" | "video"
    | "columns" | "spacer" | "divider"
    | "hero" | "cards" | "feature" | "faq" | "callout"
    | "navbar" | "footer" | "button"
    | "embed";

  type ThemeId = "minimal" | "dark" | "ocean" | "warm" | "glass";

  interface Block {
    id: string;
    type: BlockType;
    // Bag of type-specific data (text, items, src, href, level, etc.)
    data: Record<string, any>;
  }

  // ─── State ──────────────────────────────────────────────────────────────
  let title = $state("Mon site Torus");
  let domain = $state("");
  let theme = $state<ThemeId>("dark");
  let blocks = $state<Block[]>([]);
  let manualTags = $state<string[]>([]);
  let tagInput = $state("");
  let showCode = $state(false);
  let showMeta = $state(false);
  let metaLang = $state("fr");
  let metaCategory = $state("personnel");
  let metaDescription = $state("");
  let publishing = $state(false);
  let publishMsg = $state("");
  let previewFrame: HTMLIFrameElement | undefined = $state();
  let dragId = $state<string | null>(null);
  let pickerOpenAt = $state<number | null>(null); // index where + was clicked
  let pickerCategory = $state<"texte" | "media" | "structure" | "sections" | "nav" | "avance">("texte");

  // ─── Block defaults ─────────────────────────────────────────────────────
  function uid(): string {
    if (typeof crypto !== "undefined" && (crypto as any).randomUUID) {
      return (crypto as any).randomUUID();
    }
    return Math.random().toString(36).slice(2) + Date.now().toString(36);
  }

  function defaultData(type: BlockType): Record<string, any> {
    switch (type) {
      case "heading": return { level: 2, text: "Un titre accrocheur" };
      case "paragraph": return { text: "Écris ton paragraphe ici. Clique pour éditer le texte directement." };
      case "quote": return { text: "Une citation marquante.", author: "Auteur·rice" };
      case "list": return { ordered: false, items: ["Premier item", "Deuxième item", "Troisième item"] };
      case "code": return { lang: "javascript", code: "console.log('Hello Torus');" };
      case "image": return { src: "", caption: "", alt: "" };
      case "gallery": return { images: [] as string[] };
      case "video": return { url: "" };
      case "columns": return {
        cols: 2,
        children: [
          { id: uid(), type: "paragraph", data: { text: "Colonne gauche." } },
          { id: uid(), type: "paragraph", data: { text: "Colonne droite." } },
        ] as Block[],
      };
      case "spacer": return { size: "M" };
      case "divider": return {};
      case "hero": return {
        title: "Une promesse forte",
        subtitle: "Le sous-titre qui explique pourquoi en une phrase.",
        ctaText: "Découvrir",
        ctaHref: "#",
      };
      case "cards": return {
        items: [
          { emoji: "✨", title: "Premier atout", desc: "Décris ici un point fort." },
          { emoji: "🚀", title: "Deuxième atout", desc: "Mets en avant une force." },
          { emoji: "🌍", title: "Troisième atout", desc: "Souligne ce qui change." },
        ],
      };
      case "feature": return {
        icon: "🛡️",
        title: "Une fonctionnalité clé",
        desc: "Quelques lignes pour expliquer la valeur que tu apportes.",
        side: "left",
      };
      case "faq": return {
        items: [
          { q: "Comment ça marche ?", a: "Réponds ici de façon claire et concise." },
          { q: "Combien ça coûte ?", a: "Précise tes tarifs ou ta gratuité." },
        ],
      };
      case "callout": return {
        emoji: "💡",
        text: "Un message à mettre en avant. Idéal pour une astuce ou une note importante.",
        color: "cyan",
      };
      case "navbar": return {
        logo: title,
        links: [
          { label: "Accueil", href: "#" },
          { label: "À propos", href: "#about" },
          { label: "Contact", href: "#contact" },
        ],
      };
      case "footer": return {
        copyright: `© ${new Date().getFullYear()} ${title}`,
        links: [
          { label: "Mentions légales", href: "#" },
          { label: "Contact", href: "#" },
        ],
      };
      case "button": return { text: "Cliquer ici", href: "#", color: "accent" };
      case "embed": return { html: "<!-- HTML brut sanitisé : pas de <script> -->" };
    }
  }

  function makeBlock(type: BlockType): Block {
    return { id: uid(), type, data: defaultData(type) };
  }

  // ─── Block ops ──────────────────────────────────────────────────────────
  function insertBlock(type: BlockType, atIndex: number) {
    const b = makeBlock(type);
    const copy = blocks.slice();
    copy.splice(atIndex, 0, b);
    blocks = copy;
    pickerOpenAt = null;
  }

  function deleteBlock(id: string) {
    blocks = blocks.filter((b) => b.id !== id);
  }

  function moveBlock(id: string, direction: -1 | 1) {
    const idx = blocks.findIndex((b) => b.id === id);
    if (idx < 0) return;
    const target = idx + direction;
    if (target < 0 || target >= blocks.length) return;
    const copy = blocks.slice();
    [copy[idx], copy[target]] = [copy[target], copy[idx]];
    blocks = copy;
  }

  function duplicateBlock(id: string) {
    const idx = blocks.findIndex((b) => b.id === id);
    if (idx < 0) return;
    const orig = blocks[idx];
    const copy = blocks.slice();
    copy.splice(idx + 1, 0, { ...orig, id: uid(), data: JSON.parse(JSON.stringify(orig.data)) });
    blocks = copy;
  }

  function updateBlock(id: string, patch: Record<string, any>) {
    blocks = blocks.map((b) => (b.id === id ? { ...b, data: { ...b.data, ...patch } } : b));
  }

  function setBlockData(id: string, key: string, value: any) {
    updateBlock(id, { [key]: value });
  }

  // ─── Drag & drop ────────────────────────────────────────────────────────
  function onDragStart(e: DragEvent, id: string) {
    dragId = id;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", id);
    }
  }

  function onDragOver(e: DragEvent) {
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  }

  function onDrop(e: DragEvent, targetId: string) {
    e.preventDefault();
    const sourceId = dragId;
    dragId = null;
    if (!sourceId || sourceId === targetId) return;
    const src = blocks.findIndex((b) => b.id === sourceId);
    const tgt = blocks.findIndex((b) => b.id === targetId);
    if (src < 0 || tgt < 0) return;
    const copy = blocks.slice();
    const [moved] = copy.splice(src, 1);
    copy.splice(tgt, 0, moved);
    blocks = copy;
  }

  // ─── Image picker ───────────────────────────────────────────────────────
  function pickImage(blockId: string, key: string = "src") {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "image/*";
    input.onchange = () => {
      const f = input.files?.[0];
      if (!f) return;
      const reader = new FileReader();
      reader.onload = () => {
        const dataUrl = reader.result as string;
        if (key === "gallery") {
          const b = blocks.find((x) => x.id === blockId);
          if (b) {
            const arr = (b.data.images as string[]) || [];
            updateBlock(blockId, { images: [...arr, dataUrl] });
          }
        } else {
          updateBlock(blockId, { [key]: dataUrl });
        }
      };
      reader.readAsDataURL(f);
    };
    input.click();
  }

  function removeGalleryImage(blockId: string, idx: number) {
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const arr = (b.data.images as string[]).slice();
    arr.splice(idx, 1);
    updateBlock(blockId, { images: arr });
  }

  // ─── Tags ───────────────────────────────────────────────────────────────
  function normaliseTag(raw: string): string {
    const lower = raw.trim().toLowerCase();
    return lower.replace(/[^a-z0-9-]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 30);
  }

  function addTag(raw: string) {
    const clean = normaliseTag(raw);
    if (!clean) return;
    if (manualTags.includes(clean)) return;
    if (manualTags.length >= 10) return;
    manualTags = [...manualTags, clean];
  }

  function removeTag(t: string) {
    manualTags = manualTags.filter((x) => x !== t);
  }

  function onTagKey(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      if (tagInput.trim()) {
        addTag(tagInput);
        tagInput = "";
      }
    } else if (e.key === "Backspace" && !tagInput && manualTags.length) {
      manualTags = manualTags.slice(0, -1);
    }
  }

  // ─── Suggested tags (auto) ──────────────────────────────────────────────
  const STOPWORDS = new Set([
    "le", "la", "les", "de", "des", "du", "un", "une", "et", "ou", "à", "au",
    "aux", "ce", "ces", "cette", "que", "qui", "pour", "par", "sur", "dans",
    "en", "se", "sa", "son", "ses", "est", "sont", "pas", "ne", "plus", "avec",
    "the", "a", "an", "of", "to", "in", "and", "or", "is", "are", "for", "on",
    "at", "by", "with", "this", "that", "as", "be", "it", "its", "from",
  ]);

  function tokenise(text: string): string[] {
    return text
      .toLowerCase()
      .split(/[^\p{L}\p{N}]+/u)
      .filter((t) => t.length >= 2 && !STOPWORDS.has(t));
  }

  function gatherText(b: Block): string {
    const d = b.data;
    const parts: string[] = [];
    const push = (v: any) => {
      if (typeof v === "string") parts.push(v);
      else if (Array.isArray(v)) v.forEach(push);
      else if (v && typeof v === "object") Object.values(v).forEach(push);
    };
    push(d);
    return parts.join(" ");
  }

  const suggestedTags = $derived.by(() => {
    const text = blocks.map(gatherText).join(" ") + " " + title;
    const tokens = tokenise(text);
    const counts = new Map<string, number>();
    for (const t of tokens) counts.set(t, (counts.get(t) ?? 0) + 1);
    const ranked = Array.from(counts.entries())
      .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
      .map(([t]) => t)
      .filter((t) => !manualTags.includes(t))
      .slice(0, 5);
    return ranked;
  });

  // ─── Themes ─────────────────────────────────────────────────────────────
  const THEMES: Record<ThemeId, { label: string; preview: string; vars: string }> = {
    minimal: {
      label: "Minimal",
      preview: "linear-gradient(135deg,#ffffff,#f4f4f5)",
      vars: `
        --bg:#ffffff;--surface:#f8f8f9;--text:#0f172a;--muted:#64748b;
        --accent:#0f172a;--accent-fg:#ffffff;--border:#e4e4e7;--radius:12px;
        --shadow:0 1px 2px rgba(0,0,0,.06);
        --font:'Inter',-apple-system,system-ui,sans-serif;`,
    },
    dark: {
      label: "Dark Pro",
      preview: "linear-gradient(135deg,#0a0a0a,#1a1a1a)",
      vars: `
        --bg:#0a0a0a;--surface:#141417;--text:#e8e8f0;--muted:#a0a0b0;
        --accent:#00E5CC;--accent-fg:#0a0a0a;--border:rgba(255,255,255,.08);--radius:14px;
        --shadow:0 4px 24px rgba(0,0,0,.4);
        --font:'Inter',-apple-system,system-ui,sans-serif;`,
    },
    ocean: {
      label: "Ocean",
      preview: "linear-gradient(135deg,#0f172a,#06b6d4)",
      vars: `
        --bg:#0f172a;--surface:#1e293b;--text:#e2e8f0;--muted:#94a3b8;
        --accent:#06b6d4;--accent-fg:#0f172a;--border:rgba(255,255,255,.08);--radius:14px;
        --shadow:0 4px 24px rgba(6,182,212,.15);
        --font:'Inter',-apple-system,system-ui,sans-serif;`,
    },
    warm: {
      label: "Warm",
      preview: "linear-gradient(135deg,#fefce8,#c2410c)",
      vars: `
        --bg:#fefce8;--surface:#fff7ed;--text:#1c1917;--muted:#78716c;
        --accent:#c2410c;--accent-fg:#fefce8;--border:#e7e5e4;--radius:18px;
        --shadow:0 2px 8px rgba(194,65,12,.15);
        --font:'Inter',-apple-system,system-ui,sans-serif;`,
    },
    glass: {
      label: "Glass",
      preview: "linear-gradient(135deg,rgba(0,229,204,.4),rgba(139,92,246,.4))",
      vars: `
        --bg:linear-gradient(135deg,#1e1b4b,#5b21b6);--surface:rgba(255,255,255,.08);
        --text:#f8fafc;--muted:#cbd5e1;--accent:#a78bfa;--accent-fg:#1e1b4b;
        --border:rgba(255,255,255,.18);--radius:20px;
        --shadow:0 8px 32px rgba(0,0,0,.4);
        --font:'Inter',-apple-system,system-ui,sans-serif;
        --glass:1;`,
    },
  };

  // ─── Templates ──────────────────────────────────────────────────────────
  function applyTemplate(name: "landing" | "blog" | "portfolio" | "boutique" | "perso") {
    if (blocks.length && !confirm("Remplacer le contenu actuel par ce template ?")) return;
    switch (name) {
      case "landing":
        blocks = [
          makeBlock("navbar"),
          { ...makeBlock("hero"), data: { title: "Votre produit, simplement.", subtitle: "Une promesse claire en une phrase qui donne envie d'en savoir plus.", ctaText: "Commencer", ctaHref: "#" } },
          { ...makeBlock("cards"), data: { items: [
            { emoji: "⚡", title: "Rapide", desc: "Optimisé pour la vitesse." },
            { emoji: "🔒", title: "Sécurisé", desc: "Chiffrement de bout en bout." },
            { emoji: "🌐", title: "P2P", desc: "Sans serveur central." },
          ] } },
          makeBlock("feature"),
          makeBlock("button"),
          makeBlock("footer"),
        ];
        break;
      case "blog":
        blocks = [
          makeBlock("navbar"),
          { ...makeBlock("heading"), data: { level: 1, text: "Le titre de mon article" } },
          { ...makeBlock("paragraph"), data: { text: "Le chapô — une introduction qui résume l'article et donne envie de poursuivre." } },
          { ...makeBlock("image"), data: { src: "", caption: "Légende de l'image", alt: "Image principale" } },
          { ...makeBlock("paragraph"), data: { text: "Le développement de l'article. Tu peux ajouter autant de paragraphes que nécessaire pour développer ton argumentaire." } },
          { ...makeBlock("quote"), data: { text: "Une citation forte qui appuie ton propos.", author: "— Source" } },
          { ...makeBlock("paragraph"), data: { text: "Conclusion : récapitule les points clés et invite ton lectorat à réagir." } },
          makeBlock("footer"),
        ];
        break;
      case "portfolio":
        blocks = [
          { ...makeBlock("hero"), data: { title: "Bonjour, je suis Alex", subtitle: "Designer & développeur·se passionné·e par le web souverain.", ctaText: "Voir mon travail", ctaHref: "#works" } },
          { ...makeBlock("gallery"), data: { images: [] } },
          { ...makeBlock("paragraph"), data: { text: "Quelques mots sur moi : mon parcours, mes valeurs et ce qui me passionne dans mon métier." } },
          makeBlock("footer"),
        ];
        break;
      case "boutique":
        blocks = [
          makeBlock("navbar"),
          { ...makeBlock("hero"), data: { title: "Notre boutique", subtitle: "Des produits choisis avec soin pour celles et ceux qui aiment le beau.", ctaText: "Voir les produits", ctaHref: "#produits" } },
          { ...makeBlock("cards"), data: { items: [
            { emoji: "👟", title: "Sneakers", desc: "Modèle phare de la saison.\n89 €" },
            { emoji: "🎒", title: "Sac", desc: "En toile recyclée.\n45 €" },
            { emoji: "🧢", title: "Casquette", desc: "Brodée à la main.\n29 €" },
            { emoji: "👕", title: "T-shirt", desc: "Coton bio certifié.\n35 €" },
            { emoji: "🧦", title: "Chaussettes", desc: "Confort intégral.\n12 €" },
            { emoji: "🪪", title: "Carte cadeau", desc: "À partir de 20 €." },
          ] } },
          makeBlock("faq"),
          makeBlock("footer"),
        ];
        break;
      case "perso":
        blocks = [
          { ...makeBlock("hero"), data: { title: "Salut, moi c'est ✨", subtitle: "Petite page perso qui me ressemble.", ctaText: "Me contacter", ctaHref: "mailto:hello@example.com" } },
          { ...makeBlock("paragraph"), data: { text: "J'aime le code, le café, les balades en forêt et les soirées jeux de société. Bienvenue dans mon coin de Web." } },
          { ...makeBlock("list"), data: { ordered: false, items: [
            "Développement web",
            "Photographie argentique",
            "Cuisine végé",
            "Musique électronique",
          ] } },
          makeBlock("footer"),
        ];
        break;
    }
  }

  // ─── HTML generation ────────────────────────────────────────────────────
  function escape(s: string): string {
    return String(s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#39;");
  }

  function escapeAttr(s: string): string {
    return escape(s);
  }

  function sanitiseEmbed(html: string): string {
    const tag = "scr" + "ipt";
    const re = new RegExp("<" + tag + "[\\s\\S]*?<\\/" + tag + ">", "gi");
    let out = html.replace(re, "");
    out = out.replace(/\son\w+\s*=\s*"[^"]*"/gi, "");
    out = out.replace(/\son\w+\s*=\s*'[^']*'/gi, "");
    out = out.replace(/\son\w+\s*=\s*[^\s>]+/gi, "");
    out = out.replace(/javascript:/gi, "");
    return out;
  }

  function blockHtml(b: Block): string {
    const d = b.data;
    switch (b.type) {
      case "heading": {
        const lvl = Math.max(1, Math.min(3, d.level ?? 2));
        return `<h${lvl} class="b-heading">${escape(d.text ?? "")}</h${lvl}>`;
      }
      case "paragraph":
        return `<p class="b-para">${(d.text ?? "")
          .split("\n")
          .map((l: string) => escape(l))
          .join("<br>")}</p>`;
      case "quote":
        return `<blockquote class="b-quote"><p>${escape(d.text ?? "")}</p><cite>${escape(d.author ?? "")}</cite></blockquote>`;
      case "list": {
        const tag = d.ordered ? "ol" : "ul";
        const items = (d.items ?? []).map((it: string) => `<li>${escape(it)}</li>`).join("");
        return `<${tag} class="b-list">${items}</${tag}>`;
      }
      case "code":
        return `<pre class="b-code"><code>${escape(d.code ?? "")}</code></pre>`;
      case "image":
        if (!d.src) return `<div class="b-image-placeholder">Image vide</div>`;
        return `<figure class="b-image"><img src="${d.src}" alt="${escapeAttr(d.alt ?? "")}"/>${d.caption ? `<figcaption>${escape(d.caption)}</figcaption>` : ""}</figure>`;
      case "gallery": {
        const imgs = (d.images ?? []) as string[];
        if (!imgs.length) return `<div class="b-gallery-placeholder">Galerie vide</div>`;
        return `<div class="b-gallery">${imgs.map((src) => `<img src="${src}" alt=""/>`).join("")}</div>`;
      }
      case "video": {
        const url: string = d.url ?? "";
        if (!url) return `<div class="b-video-placeholder">Aucune URL</div>`;
        // Very simple: assume direct mp4 or generic iframe URL
        const isMp4 = /\.(mp4|webm|ogg)(\?|$)/i.test(url);
        if (isMp4) return `<video class="b-video" controls src="${url}"></video>`;
        return `<div class="b-video"><iframe src="${url}" loading="lazy" allowfullscreen></iframe></div>`;
      }
      case "columns": {
        const children = (d.children ?? []) as Block[];
        const inner = children.map(blockHtml).join("");
        return `<div class="b-columns" data-cols="${d.cols ?? 2}">${inner}</div>`;
      }
      case "spacer": {
        const sz = { S: 24, M: 56, L: 96 }[d.size as "S" | "M" | "L"] ?? 56;
        return `<div class="b-spacer" style="height:${sz}px"></div>`;
      }
      case "divider":
        return `<hr class="b-divider"/>`;
      case "hero":
        return `<section class="b-hero"><h1>${escape(d.title ?? "")}</h1><p>${escape(d.subtitle ?? "")}</p>${d.ctaText ? `<a class="b-cta" href="${escapeAttr(d.ctaHref ?? "#")}">${escape(d.ctaText)}</a>` : ""}</section>`;
      case "cards": {
        const items = (d.items ?? []) as Array<{ emoji: string; title: string; desc: string }>;
        const inner = items
          .map(
            (it) =>
              `<article class="b-card"><div class="b-card-emoji">${escape(it.emoji ?? "")}</div><h3>${escape(it.title ?? "")}</h3><p>${escape(it.desc ?? "")}</p></article>`,
          )
          .join("");
        return `<div class="b-cards" data-cols="${Math.min(4, Math.max(2, items.length))}">${inner}</div>`;
      }
      case "feature":
        return `<section class="b-feature" data-side="${escapeAttr(d.side ?? "left")}"><div class="b-feature-icon">${escape(d.icon ?? "")}</div><div class="b-feature-body"><h2>${escape(d.title ?? "")}</h2><p>${escape(d.desc ?? "")}</p></div></section>`;
      case "faq": {
        const items = (d.items ?? []) as Array<{ q: string; a: string }>;
        return `<dl class="b-faq">${items
          .map(
            (it) =>
              `<details><summary>${escape(it.q ?? "")}</summary><div class="b-faq-a">${escape(it.a ?? "")}</div></details>`,
          )
          .join("")}</dl>`;
      }
      case "callout":
        return `<aside class="b-callout" data-color="${escapeAttr(d.color ?? "cyan")}"><span class="b-callout-emoji">${escape(d.emoji ?? "")}</span><p>${escape(d.text ?? "")}</p></aside>`;
      case "navbar": {
        const links = (d.links ?? []) as Array<{ label: string; href: string }>;
        return `<nav class="b-navbar"><div class="b-navbar-logo">${escape(d.logo ?? "")}</div><ul>${links
          .map((l) => `<li><a href="${escapeAttr(l.href)}">${escape(l.label)}</a></li>`)
          .join("")}</ul></nav>`;
      }
      case "footer": {
        const links = (d.links ?? []) as Array<{ label: string; href: string }>;
        return `<footer class="b-footer"><div>${escape(d.copyright ?? "")}</div><ul>${links
          .map((l) => `<li><a href="${escapeAttr(l.href)}">${escape(l.label)}</a></li>`)
          .join("")}</ul></footer>`;
      }
      case "button":
        return `<div class="b-button-wrap"><a class="b-button" data-color="${escapeAttr(d.color ?? "accent")}" href="${escapeAttr(d.href ?? "#")}">${escape(d.text ?? "")}</a></div>`;
      case "embed":
        return `<div class="b-embed">${sanitiseEmbed(d.html ?? "")}</div>`;
    }
  }

  function blocksToHtml(bs: Block[], themeId: ThemeId): string {
    const t = THEMES[themeId];
    const body = bs.map(blockHtml).join("\n");
    const themeCss = t.vars;
    return `<!DOCTYPE html>
<html lang="${escapeAttr(metaLang)}">
<head>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width,initial-scale=1"/>
  <meta name="description" content="${escapeAttr(metaDescription)}"/>
  <title>${escape(title)}</title>
  <style>
:root { ${themeCss.replace(/\s+/g, " ")} }
* { box-sizing: border-box; margin: 0; padding: 0; }
html, body { background: var(--bg); color: var(--text); font-family: var(--font); line-height: 1.65; }
body { padding: 0; min-height: 100vh; }
main { max-width: 980px; margin: 0 auto; padding: 24px 20px 80px; }
a { color: var(--accent); text-decoration: none; }
a:hover { opacity: .85; }
h1, h2, h3 { line-height: 1.2; margin: 12px 0; letter-spacing: -.02em; }
h1 { font-size: 44px; font-weight: 700; }
h2 { font-size: 28px; font-weight: 600; }
h3 { font-size: 20px; font-weight: 600; }
p { margin: 12px 0; }

.b-heading { margin: 24px 0 12px; }
.b-para { color: var(--text); }
.b-quote { border-left: 3px solid var(--accent); padding: 8px 0 8px 20px; margin: 24px 0; color: var(--muted); font-style: italic; }
.b-quote cite { display: block; margin-top: 8px; font-style: normal; color: var(--muted); font-size: 14px; }
.b-list { margin: 16px 0 16px 20px; }
.b-list li { margin: 6px 0; }
.b-code { background: var(--surface); border: 1px solid var(--border); padding: 14px 18px; border-radius: var(--radius); overflow-x: auto; font-family: ui-monospace, 'SF Mono', monospace; font-size: 13.5px; }
.b-image { margin: 24px 0; }
.b-image img { width: 100%; border-radius: var(--radius); display: block; }
.b-image figcaption { color: var(--muted); font-size: 13px; margin-top: 8px; text-align: center; }
.b-image-placeholder, .b-gallery-placeholder, .b-video-placeholder { padding: 40px; text-align: center; background: var(--surface); border: 1px dashed var(--border); border-radius: var(--radius); color: var(--muted); margin: 16px 0; }
.b-gallery { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; margin: 24px 0; }
.b-gallery img { width: 100%; aspect-ratio: 1; object-fit: cover; border-radius: 8px; }
.b-video { margin: 24px 0; aspect-ratio: 16/9; }
.b-video iframe, .b-video video { width: 100%; height: 100%; border: 0; border-radius: var(--radius); }
.b-columns { display: grid; gap: 24px; margin: 32px 0; }
.b-columns[data-cols='2'] { grid-template-columns: 1fr 1fr; }
.b-columns[data-cols='3'] { grid-template-columns: 1fr 1fr 1fr; }
.b-spacer {}
.b-divider { border: 0; border-top: 1px solid var(--border); margin: 32px 0; }
.b-hero { text-align: center; padding: 80px 24px; background: var(--surface); border-radius: var(--radius); margin: 24px 0; ${themeId === "glass" ? "backdrop-filter: blur(20px);" : ""} }
.b-hero h1 { font-size: 56px; margin-bottom: 12px; }
.b-hero p { color: var(--muted); font-size: 18px; max-width: 620px; margin: 12px auto; }
.b-cta { display: inline-block; margin-top: 24px; background: var(--accent); color: var(--accent-fg); padding: 14px 28px; border-radius: 999px; font-weight: 600; }
.b-cards { display: grid; gap: 20px; margin: 32px 0; }
.b-cards[data-cols='2'] { grid-template-columns: repeat(2, 1fr); }
.b-cards[data-cols='3'] { grid-template-columns: repeat(3, 1fr); }
.b-cards[data-cols='4'] { grid-template-columns: repeat(4, 1fr); }
.b-card { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); padding: 24px; ${themeId === "glass" ? "backdrop-filter: blur(20px);" : ""} }
.b-card-emoji { font-size: 32px; margin-bottom: 12px; }
.b-card h3 { margin-bottom: 6px; }
.b-card p { color: var(--muted); white-space: pre-line; }
.b-feature { display: grid; grid-template-columns: 80px 1fr; gap: 24px; align-items: center; padding: 32px; background: var(--surface); border-radius: var(--radius); margin: 24px 0; }
.b-feature[data-side='right'] { grid-template-columns: 1fr 80px; direction: rtl; }
.b-feature[data-side='right'] .b-feature-body { direction: ltr; }
.b-feature-icon { font-size: 48px; text-align: center; }
.b-feature-body p { color: var(--muted); margin-top: 6px; }
.b-faq { margin: 32px 0; }
.b-faq details { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); margin: 8px 0; padding: 12px 16px; }
.b-faq summary { cursor: pointer; font-weight: 600; }
.b-faq-a { margin-top: 10px; color: var(--muted); }
.b-callout { display: flex; gap: 12px; align-items: flex-start; padding: 16px 20px; background: var(--surface); border-left: 4px solid var(--accent); border-radius: var(--radius); margin: 24px 0; }
.b-callout-emoji { font-size: 22px; }
.b-navbar { display: flex; justify-content: space-between; align-items: center; padding: 16px 24px; background: var(--surface); border-radius: var(--radius); margin-bottom: 16px; }
.b-navbar-logo { font-weight: 700; font-size: 18px; }
.b-navbar ul { display: flex; gap: 20px; list-style: none; }
.b-footer { margin-top: 60px; padding: 28px; background: var(--surface); border-radius: var(--radius); display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 16px; }
.b-footer ul { display: flex; gap: 16px; list-style: none; }
.b-button-wrap { text-align: center; margin: 24px 0; }
.b-button { display: inline-block; background: var(--accent); color: var(--accent-fg); padding: 12px 24px; border-radius: 999px; font-weight: 600; }
.b-button[data-color='ghost'] { background: transparent; color: var(--accent); border: 1px solid var(--accent); }
.b-embed { margin: 24px 0; }
@media (max-width: 720px) {
  .b-columns[data-cols='2'], .b-columns[data-cols='3'],
  .b-cards[data-cols='2'], .b-cards[data-cols='3'], .b-cards[data-cols='4'] { grid-template-columns: 1fr; }
  .b-gallery { grid-template-columns: repeat(2, 1fr); }
  .b-hero h1 { font-size: 36px; }
}
  </style>
</head>
<body>
  <main>
${body}
  </main>
</body>
</html>`;
  }

  const generatedHtml = $derived(blocksToHtml(blocks, theme));

  // ─── Preview iframe sync ────────────────────────────────────────────────
  $effect(() => {
    if (!previewFrame) return;
    const html = generatedHtml;
    queueMicrotask(() => {
      try {
        previewFrame!.srcdoc = html;
      } catch {
        // ignore
      }
    });
  });

  // ─── Publish ────────────────────────────────────────────────────────────
  async function doPublish() {
    if (!blocks.length) {
      publishMsg = "Ajoute au moins un bloc avant de publier.";
      return;
    }
    publishing = true;
    publishMsg = "";
    try {
      const tags = manualTags.length ? manualTags : suggestedTags;
      await invoke("publish_page", {
        title,
        content: generatedHtml,
        tags,
      });
      publishMsg = "✓ Site publié sur le réseau.";
      setTimeout(() => (publishMsg = ""), 3500);
    } catch (e) {
      publishMsg = "Erreur : " + String(e);
    } finally {
      publishing = false;
    }
  }

  // ─── Picker categories ──────────────────────────────────────────────────
  const PICKER_CATS: Record<typeof pickerCategory, { type: BlockType; label: string; emoji: string }[]> = {
    texte: [
      { type: "heading", label: "Titre", emoji: "𝐇" },
      { type: "paragraph", label: "Paragraphe", emoji: "¶" },
      { type: "quote", label: "Citation", emoji: "❝" },
      { type: "list", label: "Liste", emoji: "•" },
      { type: "code", label: "Code", emoji: "</>" },
    ],
    media: [
      { type: "image", label: "Image", emoji: "🖼" },
      { type: "gallery", label: "Galerie", emoji: "🖼🖼" },
      { type: "video", label: "Vidéo", emoji: "▶" },
    ],
    structure: [
      { type: "columns", label: "Colonnes", emoji: "⫴" },
      { type: "spacer", label: "Espace", emoji: "↕" },
      { type: "divider", label: "Séparateur", emoji: "—" },
    ],
    sections: [
      { type: "hero", label: "Hero", emoji: "🌅" },
      { type: "cards", label: "Cartes", emoji: "🗂" },
      { type: "feature", label: "Feature", emoji: "✦" },
      { type: "faq", label: "FAQ", emoji: "?" },
      { type: "callout", label: "Callout", emoji: "💡" },
    ],
    nav: [
      { type: "navbar", label: "Navbar", emoji: "≡" },
      { type: "footer", label: "Footer", emoji: "▭" },
      { type: "button", label: "Bouton", emoji: "⊕" },
    ],
    avance: [{ type: "embed", label: "HTML brut", emoji: "{}" }],
  };

  function onContentEdit(e: Event, id: string, key: string) {
    const el = e.currentTarget as HTMLElement;
    setBlockData(id, key, el.innerText);
  }

  function onListItemEdit(e: Event, blockId: string, idx: number) {
    const el = e.currentTarget as HTMLElement;
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const items = (b.data.items as string[]).slice();
    items[idx] = el.innerText;
    updateBlock(blockId, { items });
  }

  function addListItem(blockId: string) {
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const items = ((b.data.items as string[]) ?? []).slice();
    items.push("Nouvel item");
    updateBlock(blockId, { items });
  }

  function removeListItem(blockId: string, idx: number) {
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const items = (b.data.items as string[]).slice();
    items.splice(idx, 1);
    updateBlock(blockId, { items });
  }

  function addCardItem(blockId: string) {
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const items = ((b.data.items as any[]) ?? []).slice();
    if (items.length >= 6) return;
    items.push({ emoji: "✨", title: "Nouvelle carte", desc: "Description." });
    updateBlock(blockId, { items });
  }

  function removeCardItem(blockId: string, idx: number) {
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const items = ((b.data.items as any[]) ?? []).slice();
    items.splice(idx, 1);
    updateBlock(blockId, { items });
  }

  function addFaqItem(blockId: string) {
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const items = ((b.data.items as any[]) ?? []).slice();
    items.push({ q: "Nouvelle question ?", a: "Réponse." });
    updateBlock(blockId, { items });
  }

  function removeFaqItem(blockId: string, idx: number) {
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const items = ((b.data.items as any[]) ?? []).slice();
    items.splice(idx, 1);
    updateBlock(blockId, { items });
  }

  function addNavLink(blockId: string) {
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const links = ((b.data.links as any[]) ?? []).slice();
    links.push({ label: "Lien", href: "#" });
    updateBlock(blockId, { links });
  }

  function removeNavLink(blockId: string, idx: number) {
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const links = ((b.data.links as any[]) ?? []).slice();
    links.splice(idx, 1);
    updateBlock(blockId, { links });
  }
</script>

<div class="builder">
  <!-- Topbar -->
  <header class="topbar">
    <div class="topbar-left">
      <input class="topbar-input title-input" bind:value={title} placeholder="Titre du site" />
      <span class="dot"></span>
      <input class="topbar-input domain-input" bind:value={domain} placeholder="monsite.torus" />
    </div>

    <div class="tags-row">
      {#each manualTags as t (t)}
        <span class="pill">
          {t}
          <button type="button" class="pill-x" onclick={() => removeTag(t)} aria-label="Supprimer le tag">×</button>
        </span>
      {/each}
      <input
        class="tag-input"
        bind:value={tagInput}
        onkeydown={onTagKey}
        placeholder={manualTags.length ? "" : "Tags (Entrée)"}
      />
      {#if suggestedTags.length}
        <div class="suggestions" aria-label="Tags suggérés">
          {#each suggestedTags as s (s)}
            <button type="button" class="pill pill-ghost" onclick={() => addTag(s)}>+ {s}</button>
          {/each}
        </div>
      {/if}
    </div>

    <div class="topbar-right">
      <div class="theme-row">
        {#each Object.entries(THEMES) as [id, t] (id)}
          <button
            type="button"
            class="theme-dot"
            class:active={theme === id}
            style="background: {t.preview}"
            title={t.label}
            onclick={() => (theme = id as ThemeId)}
            aria-label={`Thème ${t.label}`}
          ></button>
        {/each}
      </div>
      <button type="button" class="topbar-btn" onclick={() => (showMeta = !showMeta)}>Méta</button>
      <button type="button" class="topbar-btn" onclick={() => (showCode = !showCode)}>{showCode ? "Aperçu" : "Code"}</button>
      <button type="button" class="topbar-btn primary" onclick={doPublish} disabled={publishing}>
        {publishing ? "Publication…" : "Publier 🚀"}
      </button>
    </div>
  </header>

  {#if publishMsg}
    <div class="publish-msg">{publishMsg}</div>
  {/if}

  {#if showMeta}
    <div class="meta-panel">
      <label>Langue
        <select bind:value={metaLang}>
          <option value="fr">Français</option>
          <option value="en">English</option>
          <option value="es">Español</option>
        </select>
      </label>
      <label>Catégorie
        <select bind:value={metaCategory}>
          <option value="personnel">Personnel</option>
          <option value="blog">Blog</option>
          <option value="boutique">Boutique</option>
          <option value="portfolio">Portfolio</option>
          <option value="autre">Autre</option>
        </select>
      </label>
      <label class="grow">Description
        <input bind:value={metaDescription} placeholder="Une phrase qui décrit le site"/>
      </label>
    </div>
  {/if}

  <div class="layout">
    <!-- Editor -->
    <section class="editor" aria-label="Éditeur de blocs">
      {#if blocks.length === 0}
        <div class="empty">
          <h2>Choisis un point de départ</h2>
          <div class="templates">
            <button type="button" onclick={() => applyTemplate("landing")}><span>🌅</span><b>Landing Page</b><em>Hero + features + CTA</em></button>
            <button type="button" onclick={() => applyTemplate("blog")}><span>📰</span><b>Blog</b><em>Article + image + citation</em></button>
            <button type="button" onclick={() => applyTemplate("portfolio")}><span>🎨</span><b>Portfolio</b><em>Hero + galerie + bio</em></button>
            <button type="button" onclick={() => applyTemplate("boutique")}><span>🛍</span><b>Boutique</b><em>Produits + FAQ</em></button>
            <button type="button" onclick={() => applyTemplate("perso")}><span>✨</span><b>Page perso</b><em>Bio + compétences</em></button>
          </div>
          <div class="empty-or">— ou —</div>
          <button type="button" class="big-add" onclick={() => (pickerOpenAt = 0)}>+ Ajouter un bloc</button>
        </div>
      {/if}

      <div class="add-row">
        {#if blocks.length > 0}
          <button type="button" class="add-line" onclick={() => (pickerOpenAt = 0)} aria-label="Insérer un bloc en haut">+</button>
        {/if}
      </div>

      {#each blocks as b, i (b.id)}
        <div
          class="block"
          role="group"
          aria-label={`Bloc ${b.type}`}
          draggable="true"
          ondragstart={(e) => onDragStart(e, b.id)}
          ondragover={onDragOver}
          ondrop={(e) => onDrop(e, b.id)}
          data-type={b.type}
        >
          <div class="block-handle" aria-label="Glisser pour réordonner">⋮⋮</div>

          <div class="block-toolbar" aria-label="Actions du bloc">
            <button type="button" onclick={() => moveBlock(b.id, -1)} title="Monter">↑</button>
            <button type="button" onclick={() => moveBlock(b.id, 1)} title="Descendre">↓</button>
            <button type="button" onclick={() => duplicateBlock(b.id)} title="Dupliquer">⧉</button>
            <button type="button" onclick={() => deleteBlock(b.id)} title="Supprimer">🗑</button>
          </div>

          <div class="block-body">
            {#if b.type === "heading"}
              <div class="row">
                <select value={b.data.level} onchange={(e) => setBlockData(b.id, "level", parseInt((e.currentTarget as HTMLSelectElement).value))}>
                  <option value={1}>H1</option>
                  <option value={2}>H2</option>
                  <option value={3}>H3</option>
                </select>
                <h2
                  class="ce h{b.data.level}"
                  contenteditable="true"
                  spellcheck="false"
                  oninput={(e) => onContentEdit(e, b.id, "text")}
                >{b.data.text}</h2>
              </div>
            {:else if b.type === "paragraph"}
              <p class="ce" contenteditable="true" oninput={(e) => onContentEdit(e, b.id, "text")}>{b.data.text}</p>
            {:else if b.type === "quote"}
              <blockquote>
                <p class="ce" contenteditable="true" oninput={(e) => onContentEdit(e, b.id, "text")}>{b.data.text}</p>
                <cite class="ce" contenteditable="true" oninput={(e) => onContentEdit(e, b.id, "author")}>{b.data.author}</cite>
              </blockquote>
            {:else if b.type === "list"}
              <div class="list-edit">
                <label class="inline">
                  <input type="checkbox" checked={!!b.data.ordered} onchange={(e) => setBlockData(b.id, "ordered", (e.currentTarget as HTMLInputElement).checked)}/>
                  Liste numérotée
                </label>
                <ul class="list-items">
                  {#each b.data.items as _it, ii (b.id + "-" + ii)}
                    <li>
                      <span
                        class="ce"
                        contenteditable="true"
                        oninput={(e) => onListItemEdit(e, b.id, ii)}
                      >{b.data.items[ii]}</span>
                      <button type="button" class="mini" onclick={() => removeListItem(b.id, ii)}>×</button>
                    </li>
                  {/each}
                </ul>
                <button type="button" class="mini" onclick={() => addListItem(b.id)}>+ item</button>
              </div>
            {:else if b.type === "code"}
              <textarea class="code-area" rows="6"
                value={b.data.code}
                oninput={(e) => setBlockData(b.id, "code", (e.currentTarget as HTMLTextAreaElement).value)}
              ></textarea>
            {:else if b.type === "image"}
              <div class="image-edit">
                {#if b.data.src}
                  <img src={b.data.src} alt={b.data.alt} />
                {:else}
                  <button type="button" class="picker" onclick={() => pickImage(b.id, "src")}>
                    <span>🖼</span> Choisir une image
                  </button>
                {/if}
                <input class="caption" placeholder="Légende (optionnel)" value={b.data.caption}
                  oninput={(e) => setBlockData(b.id, "caption", (e.currentTarget as HTMLInputElement).value)}/>
                <input class="caption" placeholder="Texte alternatif (a11y)" value={b.data.alt}
                  oninput={(e) => setBlockData(b.id, "alt", (e.currentTarget as HTMLInputElement).value)}/>
                {#if b.data.src}
                  <button type="button" class="mini" onclick={() => setBlockData(b.id, "src", "")}>Remplacer l'image</button>
                {/if}
              </div>
            {:else if b.type === "gallery"}
              <div class="gallery-edit">
                {#each b.data.images as src, ii (src + ii)}
                  <div class="thumb">
                    <img src={src} alt=""/>
                    <button type="button" class="mini" onclick={() => removeGalleryImage(b.id, ii)}>×</button>
                  </div>
                {/each}
                <button type="button" class="picker" onclick={() => pickImage(b.id, "gallery")}>+ Image</button>
              </div>
            {:else if b.type === "video"}
              <input class="caption" placeholder="URL (mp4 direct ou iframe embed)" value={b.data.url}
                oninput={(e) => setBlockData(b.id, "url", (e.currentTarget as HTMLInputElement).value)}/>
              {#if b.data.url}
                <div class="video-preview-inline">{@html blockHtml(b)}</div>
              {/if}
            {:else if b.type === "columns"}
              <div class="row">
                <label class="inline">
                  Colonnes :
                  <select value={b.data.cols} onchange={(e) => setBlockData(b.id, "cols", parseInt((e.currentTarget as HTMLSelectElement).value))}>
                    <option value={2}>2</option>
                    <option value={3}>3</option>
                  </select>
                </label>
              </div>
              <div class="cols-edit" style="grid-template-columns: repeat({b.data.cols}, 1fr)">
                {#each b.data.children as ch, ii (ch.id)}
                  <div class="col-cell">
                    <textarea
                      class="col-text"
                      placeholder="Contenu colonne"
                      value={ch.data.text ?? ""}
                      oninput={(e) => {
                        const text = (e.currentTarget as HTMLTextAreaElement).value;
                        const children = (b.data.children as Block[]).slice();
                        children[ii] = { ...children[ii], data: { ...children[ii].data, text } };
                        updateBlock(b.id, { children });
                      }}
                    ></textarea>
                  </div>
                {/each}
              </div>
            {:else if b.type === "spacer"}
              <label class="inline">
                Taille :
                <select value={b.data.size} onchange={(e) => setBlockData(b.id, "size", (e.currentTarget as HTMLSelectElement).value)}>
                  <option value="S">S</option>
                  <option value="M">M</option>
                  <option value="L">L</option>
                </select>
              </label>
            {:else if b.type === "divider"}
              <div class="divider-preview"></div>
            {:else if b.type === "hero"}
              <div class="hero-edit">
                <input class="big-input" value={b.data.title} placeholder="Titre principal"
                  oninput={(e) => setBlockData(b.id, "title", (e.currentTarget as HTMLInputElement).value)}/>
                <input class="caption" value={b.data.subtitle} placeholder="Sous-titre"
                  oninput={(e) => setBlockData(b.id, "subtitle", (e.currentTarget as HTMLInputElement).value)}/>
                <div class="row">
                  <input class="caption" value={b.data.ctaText} placeholder="Texte du bouton"
                    oninput={(e) => setBlockData(b.id, "ctaText", (e.currentTarget as HTMLInputElement).value)}/>
                  <input class="caption" value={b.data.ctaHref} placeholder="Lien (#section ou URL)"
                    oninput={(e) => setBlockData(b.id, "ctaHref", (e.currentTarget as HTMLInputElement).value)}/>
                </div>
              </div>
            {:else if b.type === "cards"}
              <div class="cards-edit">
                {#each b.data.items as _it, ii (b.id + "-c-" + ii)}
                  <div class="card-cell">
                    <input class="emoji-input" value={b.data.items[ii].emoji} placeholder="🎯"
                      oninput={(e) => {
                        const items = (b.data.items as any[]).slice();
                        items[ii] = { ...items[ii], emoji: (e.currentTarget as HTMLInputElement).value };
                        updateBlock(b.id, { items });
                      }}/>
                    <input class="caption" value={b.data.items[ii].title} placeholder="Titre"
                      oninput={(e) => {
                        const items = (b.data.items as any[]).slice();
                        items[ii] = { ...items[ii], title: (e.currentTarget as HTMLInputElement).value };
                        updateBlock(b.id, { items });
                      }}/>
                    <textarea class="card-desc" placeholder="Description"
                      value={b.data.items[ii].desc}
                      oninput={(e) => {
                        const items = (b.data.items as any[]).slice();
                        items[ii] = { ...items[ii], desc: (e.currentTarget as HTMLTextAreaElement).value };
                        updateBlock(b.id, { items });
                      }}></textarea>
                    <button type="button" class="mini" onclick={() => removeCardItem(b.id, ii)}>×</button>
                  </div>
                {/each}
                {#if b.data.items.length < 6}
                  <button type="button" class="mini" onclick={() => addCardItem(b.id)}>+ Carte</button>
                {/if}
              </div>
            {:else if b.type === "feature"}
              <div class="row">
                <input class="emoji-input" value={b.data.icon} placeholder="🎯"
                  oninput={(e) => setBlockData(b.id, "icon", (e.currentTarget as HTMLInputElement).value)}/>
                <input class="big-input" value={b.data.title} placeholder="Titre"
                  oninput={(e) => setBlockData(b.id, "title", (e.currentTarget as HTMLInputElement).value)}/>
                <select value={b.data.side} onchange={(e) => setBlockData(b.id, "side", (e.currentTarget as HTMLSelectElement).value)}>
                  <option value="left">Icône à gauche</option>
                  <option value="right">Icône à droite</option>
                </select>
              </div>
              <textarea class="card-desc" placeholder="Description"
                value={b.data.desc}
                oninput={(e) => setBlockData(b.id, "desc", (e.currentTarget as HTMLTextAreaElement).value)}></textarea>
            {:else if b.type === "faq"}
              <div class="faq-edit">
                {#each b.data.items as _it, ii (b.id + "-faq-" + ii)}
                  <div class="faq-cell">
                    <input class="big-input" value={b.data.items[ii].q} placeholder="Question ?"
                      oninput={(e) => {
                        const items = (b.data.items as any[]).slice();
                        items[ii] = { ...items[ii], q: (e.currentTarget as HTMLInputElement).value };
                        updateBlock(b.id, { items });
                      }}/>
                    <textarea class="card-desc" placeholder="Réponse"
                      value={b.data.items[ii].a}
                      oninput={(e) => {
                        const items = (b.data.items as any[]).slice();
                        items[ii] = { ...items[ii], a: (e.currentTarget as HTMLTextAreaElement).value };
                        updateBlock(b.id, { items });
                      }}></textarea>
                    <button type="button" class="mini" onclick={() => removeFaqItem(b.id, ii)}>× supprimer</button>
                  </div>
                {/each}
                <button type="button" class="mini" onclick={() => addFaqItem(b.id)}>+ Question</button>
              </div>
            {:else if b.type === "callout"}
              <div class="row">
                <input class="emoji-input" value={b.data.emoji} placeholder="💡"
                  oninput={(e) => setBlockData(b.id, "emoji", (e.currentTarget as HTMLInputElement).value)}/>
                <input class="big-input" value={b.data.text} placeholder="Message en avant"
                  oninput={(e) => setBlockData(b.id, "text", (e.currentTarget as HTMLInputElement).value)}/>
              </div>
            {:else if b.type === "navbar"}
              <input class="big-input" value={b.data.logo} placeholder="Logo / Nom du site"
                oninput={(e) => setBlockData(b.id, "logo", (e.currentTarget as HTMLInputElement).value)}/>
              <div class="links-edit">
                {#each b.data.links as _l, ii (b.id + "-l-" + ii)}
                  <div class="row">
                    <input class="caption" value={b.data.links[ii].label} placeholder="Label"
                      oninput={(e) => {
                        const links = (b.data.links as any[]).slice();
                        links[ii] = { ...links[ii], label: (e.currentTarget as HTMLInputElement).value };
                        updateBlock(b.id, { links });
                      }}/>
                    <input class="caption" value={b.data.links[ii].href} placeholder="Lien"
                      oninput={(e) => {
                        const links = (b.data.links as any[]).slice();
                        links[ii] = { ...links[ii], href: (e.currentTarget as HTMLInputElement).value };
                        updateBlock(b.id, { links });
                      }}/>
                    <button type="button" class="mini" onclick={() => removeNavLink(b.id, ii)}>×</button>
                  </div>
                {/each}
                <button type="button" class="mini" onclick={() => addNavLink(b.id)}>+ Lien</button>
              </div>
            {:else if b.type === "footer"}
              <input class="big-input" value={b.data.copyright} placeholder="© ..."
                oninput={(e) => setBlockData(b.id, "copyright", (e.currentTarget as HTMLInputElement).value)}/>
              <div class="links-edit">
                {#each b.data.links as _l, ii (b.id + "-fl-" + ii)}
                  <div class="row">
                    <input class="caption" value={b.data.links[ii].label} placeholder="Label"
                      oninput={(e) => {
                        const links = (b.data.links as any[]).slice();
                        links[ii] = { ...links[ii], label: (e.currentTarget as HTMLInputElement).value };
                        updateBlock(b.id, { links });
                      }}/>
                    <input class="caption" value={b.data.links[ii].href} placeholder="Lien"
                      oninput={(e) => {
                        const links = (b.data.links as any[]).slice();
                        links[ii] = { ...links[ii], href: (e.currentTarget as HTMLInputElement).value };
                        updateBlock(b.id, { links });
                      }}/>
                    <button type="button" class="mini" onclick={() => removeNavLink(b.id, ii)}>×</button>
                  </div>
                {/each}
                <button type="button" class="mini" onclick={() => addNavLink(b.id)}>+ Lien</button>
              </div>
            {:else if b.type === "button"}
              <div class="row">
                <input class="caption" value={b.data.text} placeholder="Texte"
                  oninput={(e) => setBlockData(b.id, "text", (e.currentTarget as HTMLInputElement).value)}/>
                <input class="caption" value={b.data.href} placeholder="Lien"
                  oninput={(e) => setBlockData(b.id, "href", (e.currentTarget as HTMLInputElement).value)}/>
                <select value={b.data.color} onchange={(e) => setBlockData(b.id, "color", (e.currentTarget as HTMLSelectElement).value)}>
                  <option value="accent">Plein</option>
                  <option value="ghost">Contour</option>
                </select>
              </div>
            {:else if b.type === "embed"}
              <textarea class="code-area" rows="5" placeholder="HTML brut (sans <script>)"
                value={b.data.html}
                oninput={(e) => setBlockData(b.id, "html", (e.currentTarget as HTMLTextAreaElement).value)}></textarea>
            {/if}
          </div>
        </div>

        <div class="add-row">
          <button type="button" class="add-line" onclick={() => (pickerOpenAt = i + 1)}>+</button>
        </div>
      {/each}
    </section>

    <!-- Preview / Code -->
    <section class="preview" aria-label="Aperçu">
      {#if showCode}
        <pre class="code-view"><code>{generatedHtml}</code></pre>
      {:else}
        <iframe
          bind:this={previewFrame}
          title="Aperçu"
          sandbox="allow-same-origin"
        ></iframe>
      {/if}
    </section>
  </div>

  {#if pickerOpenAt !== null}
    <div
      class="picker-overlay"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={() => (pickerOpenAt = null)}
      onkeydown={(e) => { if (e.key === "Escape") pickerOpenAt = null; }}
    >
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <div class="picker-panel" role="document" onclick={(e) => e.stopPropagation()}>
        <header class="picker-header">
          <h3>Ajouter un bloc</h3>
          <button type="button" class="mini" onclick={() => (pickerOpenAt = null)}>×</button>
        </header>
        <nav class="picker-tabs">
          {#each (Object.keys(PICKER_CATS) as Array<typeof pickerCategory>) as cat (cat)}
            <button type="button" class:active={pickerCategory === cat} onclick={() => (pickerCategory = cat)}>{cat}</button>
          {/each}
        </nav>
        <div class="picker-grid">
          {#each PICKER_CATS[pickerCategory] as opt (opt.type)}
            <button type="button" class="picker-tile" onclick={() => insertBlock(opt.type, pickerOpenAt!)}>
              <span class="picker-emoji">{opt.emoji}</span>
              <b>{opt.label}</b>
            </button>
          {/each}
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
.builder {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--color-bg-0, #050507);
  color: var(--color-text-0, #e8e8f0);
  font-family: var(--font-display, 'Inter', system-ui, sans-serif);
}

.topbar {
  display: grid;
  grid-template-columns: 1fr 1.4fr auto;
  gap: 12px;
  align-items: center;
  padding: 10px 16px;
  background: var(--color-bg-1, #0a0a0d);
  border-bottom: 1px solid var(--color-border, rgba(255,255,255,.06));
  flex-shrink: 0;
}
.topbar-left { display: flex; align-items: center; gap: 8px; min-width: 0; }
.topbar-right { display: flex; align-items: center; gap: 10px; }
.topbar-input {
  background: var(--color-bg-2, #111116);
  border: 1px solid var(--color-border, rgba(255,255,255,.06));
  color: var(--color-text-0, #e8e8f0);
  padding: 8px 12px;
  border-radius: 8px;
  font-size: 14px;
  outline: none;
  min-width: 0;
}
.topbar-input:focus { border-color: var(--color-accent, #00E5CC); }
.title-input { font-weight: 600; flex: 1; }
.domain-input { width: 180px; font-family: ui-monospace, monospace; font-size: 13px; }
.dot { width: 4px; height: 4px; background: var(--color-border, rgba(255,255,255,.2)); border-radius: 50%; }

.tags-row {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  padding: 6px 8px;
  background: var(--color-bg-2, #111116);
  border: 1px solid var(--color-border, rgba(255,255,255,.06));
  border-radius: 8px;
  min-height: 36px;
}
.pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  background: var(--color-accent-dim, rgba(0,229,204,.12));
  color: var(--color-accent, #00E5CC);
  padding: 3px 10px;
  border-radius: 999px;
  font-size: 12px;
  border: 1px solid var(--color-accent-dim, rgba(0,229,204,.12));
}
.pill-x {
  background: transparent;
  border: 0;
  color: inherit;
  cursor: pointer;
  font-size: 14px;
  padding: 0;
  line-height: 1;
}
.pill-ghost {
  background: transparent;
  color: var(--color-text-2, #8888a0);
  border: 1px dashed var(--color-border, rgba(255,255,255,.12));
  cursor: pointer;
}
.pill-ghost:hover { color: var(--color-accent, #00E5CC); border-color: var(--color-accent, #00E5CC); }
.tag-input {
  background: transparent;
  border: 0;
  color: inherit;
  outline: none;
  flex: 1;
  min-width: 80px;
  font-size: 13px;
}
.suggestions { display: flex; gap: 4px; flex-wrap: wrap; }

.theme-row { display: flex; gap: 6px; padding: 0 8px; border-right: 1px solid var(--color-border, rgba(255,255,255,.06)); margin-right: 4px; }
.theme-dot {
  width: 22px;
  height: 22px;
  border-radius: 50%;
  border: 2px solid transparent;
  cursor: pointer;
  padding: 0;
  transition: transform .12s, border-color .12s;
}
.theme-dot.active { border-color: var(--color-accent, #00E5CC); transform: scale(1.12); }
.theme-dot:hover { transform: scale(1.08); }

.topbar-btn {
  background: var(--color-bg-2, #111116);
  border: 1px solid var(--color-border, rgba(255,255,255,.06));
  color: var(--color-text-0, #e8e8f0);
  padding: 7px 14px;
  border-radius: 8px;
  font-size: 13px;
  cursor: pointer;
  font-weight: 500;
}
.topbar-btn:hover { background: var(--color-bg-hover, #1f1f25); }
.topbar-btn.primary { background: var(--color-accent, #00E5CC); color: #050507; border-color: transparent; }
.topbar-btn.primary:hover { background: var(--color-accent-hover, #00f5db); }
.topbar-btn:disabled { opacity: .5; cursor: not-allowed; }

.publish-msg { padding: 8px 16px; background: var(--color-accent-dim, rgba(0,229,204,.12)); color: var(--color-accent, #00E5CC); font-size: 13px; }

.meta-panel {
  display: flex;
  gap: 12px;
  padding: 10px 16px;
  background: var(--color-bg-1, #0a0a0d);
  border-bottom: 1px solid var(--color-border, rgba(255,255,255,.06));
  flex-wrap: wrap;
}
.meta-panel label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--color-text-2, #8888a0); }
.meta-panel label.grow { flex: 1; min-width: 220px; }
.meta-panel input, .meta-panel select {
  background: var(--color-bg-2, #111116);
  border: 1px solid var(--color-border, rgba(255,255,255,.06));
  color: var(--color-text-0, #e8e8f0);
  padding: 6px 10px;
  border-radius: 6px;
  font-size: 13px;
  outline: none;
}

.layout {
  display: grid;
  grid-template-columns: 1fr 1fr;
  flex: 1;
  min-height: 0;
}
.editor {
  overflow-y: auto;
  padding: 24px 32px 80px;
  background: var(--color-bg-0, #050507);
}
.preview {
  border-left: 1px solid var(--color-border, rgba(255,255,255,.06));
  background: #fff;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
.preview iframe { flex: 1; width: 100%; border: 0; background: white; }
.code-view {
  flex: 1;
  margin: 0;
  padding: 16px;
  overflow: auto;
  background: var(--color-bg-1, #0a0a0d);
  color: var(--color-text-0, #e8e8f0);
  font-size: 12.5px;
  font-family: ui-monospace, 'SF Mono', monospace;
}

@media (max-width: 900px) {
  .layout { grid-template-columns: 1fr; }
  .preview { border-left: 0; border-top: 1px solid var(--color-border, rgba(255,255,255,.06)); height: 50vh; }
}

/* Empty state */
.empty {
  text-align: center;
  padding: 60px 24px 80px;
  max-width: 720px;
  margin: 0 auto;
}
.empty h2 { font-size: 24px; margin-bottom: 24px; font-weight: 600; }
.templates {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: 12px;
  margin-bottom: 28px;
}
.templates button {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 6px;
  padding: 18px;
  background: var(--color-bg-2, #111116);
  border: 1px solid var(--color-border, rgba(255,255,255,.06));
  border-radius: 12px;
  color: var(--color-text-0, #e8e8f0);
  cursor: pointer;
  text-align: left;
  transition: border-color .12s, transform .12s;
}
.templates button:hover { border-color: var(--color-accent, #00E5CC); transform: translateY(-2px); }
.templates button span { font-size: 24px; }
.templates button b { font-size: 14px; }
.templates button em { font-size: 12px; color: var(--color-text-2, #8888a0); font-style: normal; }

.empty-or { color: var(--color-text-2, #8888a0); margin: 12px 0 16px; font-size: 13px; }
.big-add {
  background: transparent;
  border: 1px dashed var(--color-border, rgba(255,255,255,.18));
  color: var(--color-text-1, #a0a0b0);
  padding: 12px 24px;
  border-radius: 10px;
  cursor: pointer;
  font-size: 14px;
}
.big-add:hover { color: var(--color-accent, #00E5CC); border-color: var(--color-accent, #00E5CC); }

/* Block */
.block {
  position: relative;
  background: var(--color-bg-1, #0a0a0d);
  border: 1px solid var(--color-border, rgba(255,255,255,.06));
  border-radius: 12px;
  padding: 16px 20px 16px 36px;
  margin: 0 0 4px;
  cursor: grab;
  transition: border-color .12s;
}
.block:active { cursor: grabbing; }
.block:hover { border-color: var(--color-border-hover, rgba(255,255,255,.12)); }
.block:hover .block-toolbar { opacity: 1; }

.block-handle {
  position: absolute;
  left: 8px;
  top: 50%;
  transform: translateY(-50%);
  color: var(--color-text-3, #444458);
  font-size: 14px;
  letter-spacing: -1px;
  user-select: none;
}

.block-toolbar {
  position: absolute;
  top: 8px;
  right: 8px;
  display: flex;
  gap: 2px;
  padding: 2px;
  background: var(--color-bg-2, #111116);
  border: 1px solid var(--color-border, rgba(255,255,255,.06));
  border-radius: 6px;
  opacity: 0;
  transition: opacity .12s;
  z-index: 2;
}
.block-toolbar button {
  background: transparent;
  border: 0;
  color: var(--color-text-1, #a0a0b0);
  padding: 4px 8px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 12px;
}
.block-toolbar button:hover { background: var(--color-bg-hover, #1f1f25); color: var(--color-text-0, #e8e8f0); }

.block-body { min-height: 24px; }
.row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; margin-bottom: 8px; }
.inline { display: inline-flex; align-items: center; gap: 6px; font-size: 13px; color: var(--color-text-2, #8888a0); }

.ce {
  outline: none;
  cursor: text;
  border-radius: 4px;
  padding: 2px 4px;
  margin: -2px -4px;
  word-break: break-word;
}
.ce:focus { background: var(--color-bg-2, #111116); }
.ce.h1 { font-size: 32px; font-weight: 700; letter-spacing: -.02em; }
.ce.h2 { font-size: 24px; font-weight: 600; }
.ce.h3 { font-size: 18px; font-weight: 600; }

blockquote { border-left: 3px solid var(--color-accent, #00E5CC); padding-left: 14px; }
blockquote cite { display: block; margin-top: 8px; color: var(--color-text-2, #8888a0); font-size: 13px; font-style: normal; }

.list-edit { display: flex; flex-direction: column; gap: 6px; }
.list-items { list-style: none; padding: 0; }
.list-items li { display: flex; align-items: center; gap: 8px; padding: 4px 0; }
.list-items li::before { content: "•"; color: var(--color-accent, #00E5CC); }

.code-area {
  width: 100%;
  background: var(--color-bg-2, #111116);
  border: 1px solid var(--color-border, rgba(255,255,255,.06));
  color: var(--color-text-0, #e8e8f0);
  padding: 12px;
  border-radius: 8px;
  font-family: ui-monospace, 'SF Mono', monospace;
  font-size: 13px;
  outline: none;
  resize: vertical;
}

.image-edit { display: flex; flex-direction: column; gap: 8px; }
.image-edit img { max-width: 100%; max-height: 320px; border-radius: 8px; object-fit: cover; }
.picker {
  background: var(--color-bg-2, #111116);
  border: 1px dashed var(--color-border, rgba(255,255,255,.18));
  color: var(--color-text-1, #a0a0b0);
  padding: 24px;
  border-radius: 10px;
  cursor: pointer;
  font-size: 14px;
  display: inline-flex;
  align-items: center;
  gap: 8px;
}
.picker:hover { border-color: var(--color-accent, #00E5CC); color: var(--color-accent, #00E5CC); }
.caption {
  background: var(--color-bg-2, #111116);
  border: 1px solid var(--color-border, rgba(255,255,255,.06));
  color: var(--color-text-0, #e8e8f0);
  padding: 7px 10px;
  border-radius: 6px;
  font-size: 13px;
  outline: none;
  width: 100%;
}
.caption:focus { border-color: var(--color-accent, #00E5CC); }
.big-input {
  background: var(--color-bg-2, #111116);
  border: 1px solid var(--color-border, rgba(255,255,255,.06));
  color: var(--color-text-0, #e8e8f0);
  padding: 10px 14px;
  border-radius: 8px;
  font-size: 16px;
  font-weight: 600;
  outline: none;
  width: 100%;
}
.big-input:focus { border-color: var(--color-accent, #00E5CC); }
.emoji-input {
  background: var(--color-bg-2, #111116);
  border: 1px solid var(--color-border, rgba(255,255,255,.06));
  color: var(--color-text-0, #e8e8f0);
  padding: 6px;
  border-radius: 6px;
  font-size: 22px;
  outline: none;
  text-align: center;
  width: 56px;
}

.gallery-edit {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
  gap: 8px;
}
.thumb { position: relative; aspect-ratio: 1; }
.thumb img { width: 100%; height: 100%; object-fit: cover; border-radius: 8px; }
.thumb .mini { position: absolute; top: 4px; right: 4px; }

.cols-edit { display: grid; gap: 12px; }
.col-cell { background: var(--color-bg-2, #111116); border: 1px solid var(--color-border, rgba(255,255,255,.06)); border-radius: 8px; padding: 12px; }
.col-text {
  width: 100%;
  background: transparent;
  border: 0;
  color: var(--color-text-0, #e8e8f0);
  font-family: inherit;
  font-size: 14px;
  outline: none;
  resize: vertical;
  min-height: 60px;
}

.divider-preview { border-top: 1px solid var(--color-border, rgba(255,255,255,.18)); margin: 8px 0; }

.hero-edit { display: flex; flex-direction: column; gap: 8px; }

.cards-edit { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 10px; }
.card-cell { background: var(--color-bg-2, #111116); border: 1px solid var(--color-border, rgba(255,255,255,.06)); border-radius: 8px; padding: 10px; display: flex; flex-direction: column; gap: 6px; position: relative; }
.card-desc {
  width: 100%;
  background: var(--color-bg-1, #0a0a0d);
  border: 1px solid var(--color-border, rgba(255,255,255,.06));
  color: var(--color-text-0, #e8e8f0);
  padding: 8px;
  border-radius: 6px;
  font-family: inherit;
  font-size: 12.5px;
  resize: vertical;
  outline: none;
  min-height: 56px;
}

.faq-edit { display: flex; flex-direction: column; gap: 12px; }
.faq-cell { background: var(--color-bg-2, #111116); border: 1px solid var(--color-border, rgba(255,255,255,.06)); border-radius: 8px; padding: 10px; display: flex; flex-direction: column; gap: 6px; }

.links-edit { display: flex; flex-direction: column; gap: 6px; margin-top: 6px; }

.video-preview-inline { margin-top: 8px; aspect-ratio: 16/9; background: var(--color-bg-2, #111116); border-radius: 8px; overflow: hidden; }

.mini {
  background: transparent;
  border: 1px solid var(--color-border, rgba(255,255,255,.12));
  color: var(--color-text-1, #a0a0b0);
  padding: 4px 10px;
  border-radius: 6px;
  font-size: 12px;
  cursor: pointer;
}
.mini:hover { color: var(--color-accent, #00E5CC); border-color: var(--color-accent, #00E5CC); }

.add-row { display: flex; justify-content: center; height: 22px; align-items: center; }
.add-line {
  background: transparent;
  border: 0;
  color: var(--color-text-3, #444458);
  font-size: 14px;
  cursor: pointer;
  width: 28px;
  height: 22px;
  border-radius: 6px;
  opacity: 0;
  transition: opacity .12s, background .12s;
}
.add-row:hover .add-line { opacity: 1; }
.add-line:hover { background: var(--color-accent-dim, rgba(0,229,204,.12)); color: var(--color-accent, #00E5CC); }

/* Picker modal */
.picker-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0,0,0,.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
}
.picker-panel {
  background: var(--color-bg-1, #0a0a0d);
  border: 1px solid var(--color-border, rgba(255,255,255,.12));
  border-radius: 14px;
  width: min(560px, 92vw);
  max-height: 80vh;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
.picker-header { display: flex; justify-content: space-between; align-items: center; padding: 14px 18px; border-bottom: 1px solid var(--color-border, rgba(255,255,255,.06)); }
.picker-header h3 { font-size: 15px; font-weight: 600; }
.picker-tabs { display: flex; gap: 4px; padding: 8px 12px; border-bottom: 1px solid var(--color-border, rgba(255,255,255,.06)); flex-wrap: wrap; }
.picker-tabs button {
  background: transparent;
  border: 0;
  color: var(--color-text-2, #8888a0);
  padding: 6px 12px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 12.5px;
  text-transform: capitalize;
}
.picker-tabs button.active, .picker-tabs button:hover { background: var(--color-bg-2, #111116); color: var(--color-text-0, #e8e8f0); }
.picker-grid { padding: 14px; display: grid; grid-template-columns: repeat(auto-fill, minmax(120px, 1fr)); gap: 8px; overflow-y: auto; }
.picker-tile {
  background: var(--color-bg-2, #111116);
  border: 1px solid var(--color-border, rgba(255,255,255,.06));
  border-radius: 10px;
  padding: 14px 8px;
  color: var(--color-text-0, #e8e8f0);
  cursor: pointer;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
}
.picker-tile:hover { border-color: var(--color-accent, #00E5CC); }
.picker-emoji { font-size: 22px; }
.picker-tile b { font-size: 12px; font-weight: 500; }
</style>
