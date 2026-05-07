<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  // ─── Types ──────────────────────────────────────────────────────────────
  type BlockType =
    | "heading" | "paragraph" | "quote" | "list" | "code"
    | "image" | "gallery" | "video"
    | "columns" | "spacer" | "divider"
    | "hero" | "cards" | "feature" | "faq" | "callout"
    | "navbar" | "footer" | "button"
    | "embed"
    | "pricing" | "testimonial" | "countdown"
    | "social-links" | "stats" | "map" | "form-placeholder";

  type ThemeId =
    | "minimal" | "dark" | "ocean" | "warm" | "glass"
    | "neon" | "corporate" | "retro";

  type PreviewSize = "mobile" | "tablet" | "desktop";

  interface Block {
    id: string;
    type: BlockType;
    data: Record<string, any>;
  }

  interface DraftSnapshot {
    title: string;
    domain: string;
    theme: ThemeId;
    blocks: Block[];
    manualTags: string[];
    metaLang: string;
    metaCategory: string;
    metaDescription: string;
  }

  // ─── State ──────────────────────────────────────────────────────────────
  const STORAGE_KEY = "torus-builder-draft";

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
  let pickerOpenAt = $state<number | null>(null);
  let pickerCategory = $state<
    "texte" | "media" | "structure" | "sections" | "nav" | "data" | "avance"
  >("texte");

  // Active block / selection
  let activeBlockId = $state<string | null>(null);

  // Drag & drop
  let dragId = $state<string | null>(null);
  let dropTargetId = $state<string | null>(null);
  let dropPosition = $state<"top" | "bottom">("bottom");

  // Responsive preview
  let previewSize = $state<PreviewSize>("desktop");

  // Rich text floating toolbar
  let richToolbar = $state<{ visible: boolean; x: number; y: number }>({
    visible: false,
    x: 0,
    y: 0,
  });

  // Undo / redo
  let history: string[] = [];
  let historyIndex = $state(-1);
  let isApplyingHistory = false;
  let historyTimer: ReturnType<typeof setTimeout> | null = null;

  // ─── Utils ──────────────────────────────────────────────────────────────
  function uid(): string {
    if (typeof crypto !== "undefined" && (crypto as any).randomUUID) {
      return (crypto as any).randomUUID();
    }
    return Math.random().toString(36).slice(2) + Date.now().toString(36);
  }

  function deepClone<T>(v: T): T {
    return JSON.parse(JSON.stringify(v));
  }

  // ─── Block defaults ─────────────────────────────────────────────────────
  function defaultData(type: BlockType): Record<string, any> {
    switch (type) {
      case "heading": return { level: 2, text: "Un titre accrocheur" };
      case "paragraph": return { text: "Écris ton paragraphe ici. Clique pour éditer le texte directement." };
      case "quote": return { text: "Une citation marquante.", author: "Auteur·rice" };
      case "list": return { ordered: false, items: ["Premier item", "Deuxième item", "Troisième item"] };
      case "code": return { lang: "javascript", code: "console.log('Hello Torus');" };
      case "image": return { src: "", caption: "", alt: "", fit: "cover", filter: "none" };
      case "gallery": return { images: [] as string[] };
      case "video": return { url: "" };
      case "columns": return {
        cols: 2,
        children: [
          { id: uid(), title: "Premier titre", content: "Contenu de la première colonne." },
          { id: uid(), title: "Second titre", content: "Contenu de la seconde colonne." },
        ],
      };
      case "spacer": return { size: "M" };
      case "divider": return {};
      case "hero": return {
        title: "Une promesse forte",
        subtitle: "Le sous-titre qui explique pourquoi en une phrase.",
        ctaText: "Découvrir",
        ctaHref: "#",
        backgroundImage: "",
        align: "center",
      };
      case "cards": return {
        items: [
          { emoji: "✨", title: "Premier atout", desc: "Décris ici un point fort.", image: "", useImage: false },
          { emoji: "🚀", title: "Deuxième atout", desc: "Mets en avant une force.", image: "", useImage: false },
          { emoji: "🌍", title: "Troisième atout", desc: "Souligne ce qui change.", image: "", useImage: false },
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
      case "pricing": return {
        plans: [
          { name: "Starter", price: "0 €", period: "/mois", features: ["1 site", "100 visiteurs/jour", "Support email"], cta: "Commencer", href: "#", highlighted: false },
          { name: "Pro", price: "9 €", period: "/mois", features: ["10 sites", "Trafic illimité", "Support prioritaire", "Analytics avancés"], cta: "Choisir Pro", href: "#", highlighted: true },
          { name: "Team", price: "29 €", period: "/mois", features: ["Sites illimités", "Multi-utilisateurs", "Support dédié", "Custom domain"], cta: "Contacter", href: "#", highlighted: false },
        ],
      };
      case "testimonial": return {
        avatar: "",
        name: "Camille D.",
        role: "Cliente fidèle",
        text: "Une expérience exceptionnelle de bout en bout. Je recommande sans hésiter.",
      };
      case "countdown": return {
        target: new Date(Date.now() + 7 * 24 * 3600 * 1000).toISOString().slice(0, 10),
        label: "Lancement dans",
      };
      case "social-links": return {
        items: [
          { network: "twitter", url: "https://twitter.com/" },
          { network: "github", url: "https://github.com/" },
        ],
      };
      case "stats": return {
        items: [
          { value: "1 200+", label: "Utilisateurs" },
          { value: "50K", label: "Sites publiés" },
          { value: "99.9%", label: "Uptime" },
        ],
      };
      case "map": return { lat: 48.8566, lng: 2.3522, zoom: 13 };
      case "form-placeholder": return {
        action: "mailto:contact@example.com",
        title: "Nous contacter",
        submitText: "Envoyer",
      };
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
    activeBlockId = b.id;
  }

  function deleteBlock(id: string, skipConfirm = false) {
    if (!skipConfirm && !confirm("Supprimer ce bloc ?")) return;
    blocks = blocks.filter((b) => b.id !== id);
    if (activeBlockId === id) activeBlockId = null;
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
    const newBlock = { ...orig, id: uid(), data: deepClone(orig.data) };
    copy.splice(idx + 1, 0, newBlock);
    blocks = copy;
    activeBlockId = newBlock.id;
  }

  function updateBlock(id: string, patch: Record<string, any>) {
    blocks = blocks.map((b) => (b.id === id ? { ...b, data: { ...b.data, ...patch } } : b));
  }

  function setBlockData(id: string, key: string, value: any) {
    updateBlock(id, { [key]: value });
  }

  function setBlockActive(e: MouseEvent | KeyboardEvent, id: string) {
    e.stopPropagation();
    activeBlockId = id;
  }

  // ─── Drag & drop ────────────────────────────────────────────────────────
  function onDragStart(e: DragEvent, id: string) {
    dragId = id;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", id);
    }
  }

  function onDragOver(e: DragEvent, targetId: string) {
    e.preventDefault();
    if (!dragId || dragId === targetId) return;
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    const target = e.currentTarget as HTMLElement;
    const rect = target.getBoundingClientRect();
    const midY = rect.top + rect.height / 2;
    dropTargetId = targetId;
    dropPosition = e.clientY < midY ? "top" : "bottom";
  }

  function onDragLeave(e: DragEvent, targetId: string) {
    if (dropTargetId === targetId) {
      const related = e.relatedTarget as HTMLElement | null;
      const target = e.currentTarget as HTMLElement;
      if (!related || !target.contains(related)) {
        dropTargetId = null;
      }
    }
  }

  function onDrop(e: DragEvent, targetId: string) {
    e.preventDefault();
    const sourceId = dragId;
    const pos = dropPosition;
    dragId = null;
    dropTargetId = null;
    if (!sourceId || sourceId === targetId) return;
    const src = blocks.findIndex((b) => b.id === sourceId);
    const tgt = blocks.findIndex((b) => b.id === targetId);
    if (src < 0 || tgt < 0) return;
    const copy = blocks.slice();
    const [moved] = copy.splice(src, 1);
    let insertAt = copy.findIndex((b) => b.id === targetId);
    if (pos === "bottom") insertAt += 1;
    copy.splice(insertAt, 0, moved);
    blocks = copy;
  }

  function onDragEnd() {
    dragId = null;
    dropTargetId = null;
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

  function pickCardImage(blockId: string, idx: number) {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "image/*";
    input.onchange = () => {
      const f = input.files?.[0];
      if (!f) return;
      const reader = new FileReader();
      reader.onload = () => {
        const b = blocks.find((x) => x.id === blockId);
        if (!b) return;
        const items = (b.data.items as any[]).slice();
        items[idx] = { ...items[idx], image: reader.result as string, useImage: true };
        updateBlock(blockId, { items });
      };
      reader.readAsDataURL(f);
    };
    input.click();
  }

  function pickAvatar(blockId: string) {
    pickImage(blockId, "avatar");
  }

  function removeGalleryImage(blockId: string, idx: number) {
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const arr = (b.data.images as string[]).slice();
    arr.splice(idx, 1);
    updateBlock(blockId, { images: arr });
  }

  // Drop file directly on an image block
  function onImageBlockDrop(e: DragEvent, blockId: string, key: string = "src") {
    if (dragId) return; // block-reorder drag, ignore here
    const files = e.dataTransfer?.files;
    if (!files || !files.length) return;
    e.preventDefault();
    const f = files[0];
    if (!f.type.startsWith("image/")) return;
    const reader = new FileReader();
    reader.onload = () => {
      if (key === "gallery") {
        const b = blocks.find((x) => x.id === blockId);
        if (b) {
          const arr = (b.data.images as string[]) || [];
          updateBlock(blockId, { images: [...arr, reader.result as string] });
        }
      } else {
        updateBlock(blockId, { [key]: reader.result as string });
      }
    };
    reader.readAsDataURL(f);
  }

  function onImageBlockDragOver(e: DragEvent) {
    if (dragId) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
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
    neon: {
      label: "Neon",
      preview: "linear-gradient(135deg,#000000,#ff006e)",
      vars: `
        --bg:#000000;--surface:#0a0a0a;--text:#ffffff;--muted:#a0a0a0;
        --accent:#ff006e;--accent-fg:#000000;--border:rgba(255,0,110,.25);--radius:8px;
        --shadow:0 0 24px rgba(255,0,110,.4),0 0 48px rgba(255,0,110,.2);
        --font:'Inter',-apple-system,system-ui,sans-serif;
        --neon:1;`,
    },
    corporate: {
      label: "Corporate",
      preview: "linear-gradient(135deg,#f8f9fa,#2563eb)",
      vars: `
        --bg:#f8f9fa;--surface:#ffffff;--text:#1f2937;--muted:#6b7280;
        --accent:#2563eb;--accent-fg:#ffffff;--border:#e5e7eb;--radius:6px;
        --shadow:0 1px 3px rgba(0,0,0,.08);
        --font:Georgia,'Times New Roman',serif;`,
    },
    retro: {
      label: "Retro",
      preview: "linear-gradient(135deg,#f5f0e8,#b91c1c)",
      vars: `
        --bg:#f5f0e8;--surface:#ede4d3;--text:#3a2e1f;--muted:#8a7a5e;
        --accent:#b91c1c;--accent-fg:#f5f0e8;--border:#d4c5a8;--radius:24px;
        --shadow:0 3px 0 rgba(185,28,28,.18);
        --font:Georgia,'Times New Roman',serif;`,
    },
  };

  // ─── Templates ──────────────────────────────────────────────────────────
  type TemplateId =
    | "landing" | "blog" | "portfolio" | "boutique" | "perso"
    | "restaurant" | "evenement" | "freelance";

  function applyTemplate(name: TemplateId) {
    if (blocks.length && !confirm("Remplacer le contenu actuel par ce template ?")) return;
    switch (name) {
      case "landing":
        blocks = [
          makeBlock("navbar"),
          { ...makeBlock("hero"), data: { ...defaultData("hero"), title: "Votre produit, simplement.", subtitle: "Une promesse claire en une phrase qui donne envie d'en savoir plus.", ctaText: "Commencer", ctaHref: "#" } },
          { ...makeBlock("stats"), data: { items: [
            { value: "500+", label: "Sites" },
            { value: "10K", label: "Utilisateurs" },
            { value: "99.9%", label: "Uptime" },
          ] } },
          { ...makeBlock("cards"), data: { items: [
            { emoji: "⚡", title: "Rapide", desc: "Optimisé pour la vitesse.", image: "", useImage: false },
            { emoji: "🔒", title: "Sécurisé", desc: "Chiffrement de bout en bout.", image: "", useImage: false },
            { emoji: "🌐", title: "P2P", desc: "Sans serveur central.", image: "", useImage: false },
          ] } },
          makeBlock("feature"),
          { ...makeBlock("testimonial"), data: { avatar: "", name: "Léa M.", role: "Early adopter", text: "Le meilleur choix pour publier rapidement, sans serveur. Vraiment bluffant." } },
          makeBlock("button"),
          makeBlock("footer"),
        ];
        break;
      case "blog":
        blocks = [
          makeBlock("navbar"),
          { ...makeBlock("heading"), data: { level: 1, text: "Le titre de mon article" } },
          { ...makeBlock("paragraph"), data: { text: "Le chapô — une introduction qui résume l'article et donne envie de poursuivre." } },
          { ...makeBlock("image"), data: { src: "", caption: "Légende de l'image", alt: "Image principale", fit: "cover", filter: "none" } },
          { ...makeBlock("paragraph"), data: { text: "Le développement de l'article. Tu peux ajouter autant de paragraphes que nécessaire pour développer ton argumentaire." } },
          { ...makeBlock("callout"), data: { emoji: "📌", text: "À retenir : un point clé que ton lectorat ne doit pas manquer.", color: "cyan" } },
          { ...makeBlock("quote"), data: { text: "Une citation forte qui appuie ton propos.", author: "— Source" } },
          { ...makeBlock("paragraph"), data: { text: "Conclusion : récapitule les points clés et invite ton lectorat à réagir." } },
          { ...makeBlock("social-links"), data: { items: [
            { network: "twitter", url: "https://twitter.com/" },
            { network: "mastodon", url: "https://mastodon.social/" },
            { network: "github", url: "https://github.com/" },
          ] } },
          makeBlock("footer"),
        ];
        break;
      case "portfolio":
        blocks = [
          { ...makeBlock("hero"), data: { ...defaultData("hero"), title: "Bonjour, je suis Alex", subtitle: "Designer & développeur·se passionné·e par le web souverain.", ctaText: "Voir mon travail", ctaHref: "#works" } },
          { ...makeBlock("stats"), data: { items: [
            { value: "8 ans", label: "d'expérience" },
            { value: "60+", label: "projets livrés" },
            { value: "30+", label: "clients heureux" },
          ] } },
          { ...makeBlock("gallery"), data: { images: [] } },
          { ...makeBlock("paragraph"), data: { text: "Quelques mots sur moi : mon parcours, mes valeurs et ce qui me passionne dans mon métier." } },
          { ...makeBlock("social-links"), data: { items: [
            { network: "github", url: "https://github.com/" },
            { network: "linkedin", url: "https://linkedin.com/in/" },
            { network: "twitter", url: "https://twitter.com/" },
          ] } },
          makeBlock("footer"),
        ];
        break;
      case "boutique":
        blocks = [
          makeBlock("navbar"),
          { ...makeBlock("hero"), data: { ...defaultData("hero"), title: "Notre boutique", subtitle: "Des produits choisis avec soin pour celles et ceux qui aiment le beau.", ctaText: "Voir les produits", ctaHref: "#produits" } },
          { ...makeBlock("cards"), data: { items: [
            { emoji: "👟", title: "Sneakers", desc: "Modèle phare de la saison.\n89 €", image: "", useImage: false },
            { emoji: "🎒", title: "Sac", desc: "En toile recyclée.\n45 €", image: "", useImage: false },
            { emoji: "🧢", title: "Casquette", desc: "Brodée à la main.\n29 €", image: "", useImage: false },
            { emoji: "👕", title: "T-shirt", desc: "Coton bio certifié.\n35 €", image: "", useImage: false },
          ] } },
          { ...makeBlock("pricing"), data: { plans: [
            { name: "Standard", price: "Gratuit", period: "", features: ["Livraison standard", "Retour sous 14 jours", "Support email"], cta: "Commander", href: "#", highlighted: false },
            { name: "Premium", price: "5 €", period: "/mois", features: ["Livraison express", "Retour sous 30 jours", "Support prioritaire", "Accès aux nouveautés en avant-première"], cta: "Devenir Premium", href: "#", highlighted: true },
          ] } },
          makeBlock("faq"),
          makeBlock("footer"),
        ];
        break;
      case "perso":
        blocks = [
          { ...makeBlock("hero"), data: { ...defaultData("hero"), title: "Salut, moi c'est ✨", subtitle: "Petite page perso qui me ressemble.", ctaText: "Me contacter", ctaHref: "mailto:hello@example.com" } },
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
      case "restaurant":
        blocks = [
          makeBlock("navbar"),
          { ...makeBlock("hero"), data: { ...defaultData("hero"), title: "La table d'Adèle", subtitle: "Cuisine de saison, produits locaux, ambiance chaleureuse.", ctaText: "Réserver", ctaHref: "#reserver", backgroundImage: "", align: "center" } },
          { ...makeBlock("cards"), data: { items: [
            { emoji: "🥗", title: "Entrée du marché", desc: "Légumes croquants, condiments maison.\n12 €", image: "", useImage: false },
            { emoji: "🐟", title: "Poisson du jour", desc: "Selon arrivage, sauce vierge.\n24 €", image: "", useImage: false },
            { emoji: "🍰", title: "Dessert signature", desc: "Tarte fine, glace vanille.\n9 €", image: "", useImage: false },
          ] } },
          { ...makeBlock("callout"), data: { emoji: "🕒", text: "Ouvert du mardi au samedi, 12h-14h et 19h-22h.", color: "cyan" } },
          { ...makeBlock("map"), data: { lat: 48.8566, lng: 2.3522, zoom: 14 } },
          makeBlock("footer"),
        ];
        break;
      case "evenement":
        blocks = [
          { ...makeBlock("hero"), data: { ...defaultData("hero"), title: "Festival 2026", subtitle: "3 jours de musique, d'art et de partage.", ctaText: "Réserver", ctaHref: "#billets", backgroundImage: "", align: "center" } },
          { ...makeBlock("countdown"), data: { target: new Date(Date.now() + 30 * 24 * 3600 * 1000).toISOString().slice(0, 10), label: "Départ dans" } },
          { ...makeBlock("paragraph"), data: { text: "Une édition exceptionnelle avec plus de 40 artistes sur 3 scènes, des ateliers, un village associatif et des rencontres." } },
          { ...makeBlock("pricing"), data: { plans: [
            { name: "Pass 1 jour", price: "39 €", period: "", features: ["Accès à toutes les scènes", "1 boisson offerte"], cta: "Acheter", href: "#", highlighted: false },
            { name: "Pass 3 jours", price: "89 €", period: "", features: ["Accès complet", "T-shirt offert", "Boissons illimitées", "File coupe-file"], cta: "Acheter", href: "#", highlighted: true },
            { name: "VIP", price: "199 €", period: "", features: ["Tout du Pass 3 jours", "Espace VIP", "Rencontre artistes", "Catering premium"], cta: "Acheter", href: "#", highlighted: false },
          ] } },
          { ...makeBlock("map"), data: { lat: 43.6047, lng: 1.4442, zoom: 13 } },
          makeBlock("footer"),
        ];
        break;
      case "freelance":
        blocks = [
          makeBlock("navbar"),
          { ...makeBlock("hero"), data: { ...defaultData("hero"), title: "Designer & développeur freelance", subtitle: "Je conçois des produits numériques utiles, élégants et accessibles.", ctaText: "Me contacter", ctaHref: "#contact" } },
          { ...makeBlock("stats"), data: { items: [
            { value: "120+", label: "Projets" },
            { value: "60+", label: "Clients" },
            { value: "10 ans", label: "d'expérience" },
          ] } },
          { ...makeBlock("feature"), data: { icon: "🎨", title: "Design produit", desc: "UI/UX, design systems, prototypage interactif.", side: "left" } },
          { ...makeBlock("feature"), data: { icon: "💻", title: "Développement", desc: "Web moderne, performant et accessible.", side: "right" } },
          { ...makeBlock("feature"), data: { icon: "🚀", title: "Conseil", desc: "Stratégie produit, accompagnement d'équipe.", side: "left" } },
          { ...makeBlock("testimonial"), data: { avatar: "", name: "Yasmine R.", role: "CEO @ Studio Aube", text: "Travail rapide, qualité au rendez-vous, communication impeccable. Je recommande sans réserve." } },
          { ...makeBlock("social-links"), data: { items: [
            { network: "github", url: "https://github.com/" },
            { network: "linkedin", url: "https://linkedin.com/in/" },
            { network: "twitter", url: "https://twitter.com/" },
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

  function sanitiseHtml(html: string): string {
    const tag = "scr" + "ipt";
    const re = new RegExp("<" + tag + "[\\s\\S]*?<\\/" + tag + ">", "gi");
    let out = html.replace(re, "");
    out = out.replace(/<iframe[\s\S]*?<\/iframe>/gi, "");
    out = out.replace(/<style[\s\S]*?<\/style>/gi, "");
    out = out.replace(/\son\w+\s*=\s*"[^"]*"/gi, "");
    out = out.replace(/\son\w+\s*=\s*'[^']*'/gi, "");
    out = out.replace(/\son\w+\s*=\s*[^\s>]+/gi, "");
    out = out.replace(/javascript:/gi, "");
    return out;
  }

  function sanitiseEmbed(html: string): string {
    return sanitiseHtml(html);
  }

  // For paragraph/quote rich content: pass through if it already contains HTML,
  // otherwise escape and turn newlines into <br>.
  function richHtml(raw: string): string {
    const text = raw ?? "";
    if (/<[a-z]/i.test(text)) {
      return sanitiseHtml(text);
    }
    return text.split("\n").map(escape).join("<br>");
  }

  const SOCIAL_ICONS: Record<string, { label: string; svg: string }> = {
    twitter: { label: "Twitter / X", svg: '<svg viewBox="0 0 24 24" width="22" height="22" fill="currentColor"><path d="M18.244 2H21.5l-7.5 8.57L23 22h-6.844l-5.36-7.013L4.5 22H1.244l8.04-9.187L1 2h6.967l4.84 6.395L18.244 2zm-1.2 18h1.83L7.04 4H5.1l11.945 16z"/></svg>' },
    github: { label: "GitHub", svg: '<svg viewBox="0 0 24 24" width="22" height="22" fill="currentColor"><path d="M12 .5C5.7.5.5 5.7.5 12c0 5.1 3.3 9.4 7.9 10.9.6.1.8-.3.8-.6v-2.2c-3.2.7-3.9-1.5-3.9-1.5-.5-1.3-1.3-1.7-1.3-1.7-1-.7.1-.7.1-.7 1.2.1 1.8 1.2 1.8 1.2 1 1.8 2.7 1.3 3.4 1 .1-.7.4-1.3.7-1.6-2.6-.3-5.3-1.3-5.3-5.7 0-1.3.5-2.3 1.2-3.1-.1-.3-.5-1.5.1-3.2 0 0 1-.3 3.3 1.2 1-.3 2-.4 3-.4s2 .1 3 .4C16.6 4.7 17.6 5 17.6 5c.6 1.7.2 2.9.1 3.2.8.8 1.2 1.9 1.2 3.1 0 4.4-2.7 5.4-5.3 5.7.4.4.8 1.1.8 2.2v3.3c0 .3.2.7.8.6 4.6-1.5 7.9-5.8 7.9-10.9 0-6.3-5.2-11.5-11.5-11.5z"/></svg>' },
    linkedin: { label: "LinkedIn", svg: '<svg viewBox="0 0 24 24" width="22" height="22" fill="currentColor"><path d="M20.45 20.45h-3.55v-5.57c0-1.33-.03-3.04-1.85-3.04-1.85 0-2.13 1.45-2.13 2.94v5.67H9.36V9h3.41v1.56h.05c.47-.9 1.63-1.85 3.36-1.85 3.6 0 4.27 2.37 4.27 5.45v6.29zM5.34 7.43a2.06 2.06 0 1 1 0-4.13 2.06 2.06 0 0 1 0 4.13zm1.78 13.02H3.56V9h3.56v11.45zM22.23 0H1.77C.79 0 0 .77 0 1.72v20.56C0 23.23.79 24 1.77 24h20.45C23.2 24 24 23.23 24 22.28V1.72C24 .77 23.2 0 22.23 0z"/></svg>' },
    discord: { label: "Discord", svg: '<svg viewBox="0 0 24 24" width="22" height="22" fill="currentColor"><path d="M20.317 4.37a19.79 19.79 0 0 0-4.885-1.515.07.07 0 0 0-.073.035c-.21.375-.444.864-.608 1.25a18.27 18.27 0 0 0-5.487 0 12.65 12.65 0 0 0-.617-1.25.07.07 0 0 0-.074-.035A19.74 19.74 0 0 0 3.683 4.37a.064.064 0 0 0-.03.027C.533 9.046-.32 13.58.099 18.058a.08.08 0 0 0 .031.055 19.9 19.9 0 0 0 5.993 3.03.07.07 0 0 0 .076-.027 14.21 14.21 0 0 0 1.226-1.994.07.07 0 0 0-.038-.097 13.1 13.1 0 0 1-1.872-.892.07.07 0 0 1-.007-.117c.126-.094.252-.192.371-.292a.07.07 0 0 1 .073-.01c3.927 1.793 8.18 1.793 12.062 0a.07.07 0 0 1 .074.01c.12.099.245.198.372.292a.07.07 0 0 1-.006.117c-.598.348-1.22.645-1.873.891a.07.07 0 0 0-.038.098 16.05 16.05 0 0 0 1.226 1.994.07.07 0 0 0 .076.028 19.84 19.84 0 0 0 6.002-3.03.07.07 0 0 0 .031-.054c.5-5.177-.838-9.674-3.549-13.66a.06.06 0 0 0-.029-.028zM8.02 15.331c-1.182 0-2.157-1.085-2.157-2.418 0-1.333.956-2.419 2.157-2.419 1.21 0 2.176 1.095 2.157 2.419 0 1.333-.957 2.418-2.157 2.418zm7.974 0c-1.183 0-2.157-1.085-2.157-2.418 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.095 2.157 2.419 0 1.333-.946 2.418-2.157 2.418z"/></svg>' },
    youtube: { label: "YouTube", svg: '<svg viewBox="0 0 24 24" width="22" height="22" fill="currentColor"><path d="M23.5 6.2a3 3 0 0 0-2.1-2.1C19.6 3.6 12 3.6 12 3.6s-7.6 0-9.4.5A3 3 0 0 0 .5 6.2C0 8 0 12 0 12s0 4 .5 5.8a3 3 0 0 0 2.1 2.1c1.8.5 9.4.5 9.4.5s7.6 0 9.4-.5a3 3 0 0 0 2.1-2.1C24 16 24 12 24 12s0-4-.5-5.8zM9.6 15.6V8.4l6.3 3.6-6.3 3.6z"/></svg>' },
    tiktok: { label: "TikTok", svg: '<svg viewBox="0 0 24 24" width="22" height="22" fill="currentColor"><path d="M19.59 6.69a4.83 4.83 0 0 1-3.77-4.25V2h-3.45v13.67a2.89 2.89 0 0 1-5.2 1.74 2.89 2.89 0 0 1 2.31-4.64 2.93 2.93 0 0 1 .88.13V9.4a6.84 6.84 0 0 0-1-.05A6.33 6.33 0 0 0 5.8 20.1a6.34 6.34 0 0 0 10.86-4.43V8.74a8.16 8.16 0 0 0 4.77 1.52V6.81a4.85 4.85 0 0 1-1.84-.12z"/></svg>' },
    mastodon: { label: "Mastodon", svg: '<svg viewBox="0 0 24 24" width="22" height="22" fill="currentColor"><path d="M21.58 8.18c0-4.85-3.18-6.27-3.18-6.27C16.81.93 14.05.5 11.18.5h-.07c-2.87 0-5.62.43-7.21 1.4 0 0-3.18 1.43-3.18 6.27 0 1.11-.02 2.43.01 3.83.1 4.7.84 9.34 5.18 10.49 2 .53 3.71.64 5.09.56 2.5-.14 3.91-.89 3.91-.89l-.08-1.81s-1.79.57-3.81.5c-2-.07-4.11-.21-4.43-2.67a5.32 5.32 0 0 1-.04-.74s1.96.48 4.45.59c1.52.07 2.95-.09 4.4-.27 2.79-.33 5.21-2.05 5.51-3.62.48-2.46.45-6 .45-6zm-3.65 6.05h-2.27V8.66c0-1.17-.49-1.76-1.47-1.76-1.08 0-1.62.7-1.62 2.08v3.02h-2.25V8.98c0-1.39-.54-2.08-1.62-2.08-.98 0-1.47.59-1.47 1.76v5.57H4.96V8.49c0-1.17.3-2.1.9-2.79.62-.69 1.43-1.04 2.43-1.04 1.18 0 2.07.45 2.66 1.36L11.7 7l.55-.93c.59-.91 1.48-1.36 2.66-1.36 1 0 1.81.35 2.43 1.04.6.69.9 1.62.9 2.79v5.74z"/></svg>' },
  };

  function blockHtml(b: Block): string {
    const d = b.data;
    switch (b.type) {
      case "heading": {
        const lvl = Math.max(1, Math.min(3, d.level ?? 2));
        return `<h${lvl} class="b-heading">${escape(d.text ?? "")}</h${lvl}>`;
      }
      case "paragraph":
        return `<p class="b-para">${richHtml(d.text ?? "")}</p>`;
      case "quote":
        return `<blockquote class="b-quote"><p>${richHtml(d.text ?? "")}</p><cite>${escape(d.author ?? "")}</cite></blockquote>`;
      case "list": {
        const tag = d.ordered ? "ol" : "ul";
        const items = (d.items ?? []).map((it: string) => `<li>${escape(it)}</li>`).join("");
        return `<${tag} class="b-list">${items}</${tag}>`;
      }
      case "code":
        return `<pre class="b-code" data-lang="${escapeAttr(d.lang ?? "javascript")}"><code>${escape(d.code ?? "")}</code></pre>`;
      case "image": {
        if (!d.src) return `<div class="b-image-placeholder">Image vide</div>`;
        const fit = d.fit === "contain" ? "contain" : "cover";
        const filter = d.filter && d.filter !== "none" ? ` data-filter="${escapeAttr(d.filter)}"` : "";
        return `<figure class="b-image" data-fit="${fit}"${filter}><img src="${d.src}" alt="${escapeAttr(d.alt ?? "")}"/>${d.caption ? `<figcaption>${escape(d.caption)}</figcaption>` : ""}</figure>`;
      }
      case "gallery": {
        const imgs = (d.images ?? []) as string[];
        if (!imgs.length) return `<div class="b-gallery-placeholder">Galerie vide</div>`;
        return `<div class="b-gallery">${imgs.map((src) => `<img src="${src}" alt=""/>`).join("")}</div>`;
      }
      case "video": {
        const url: string = d.url ?? "";
        if (!url) return `<div class="b-video-placeholder">Aucune URL</div>`;
        const isMp4 = /\.(mp4|webm|ogg)(\?|$)/i.test(url);
        if (isMp4) return `<video class="b-video" controls src="${url}"></video>`;
        return `<div class="b-video"><iframe src="${url}" loading="lazy" allowfullscreen></iframe></div>`;
      }
      case "columns": {
        const children = (d.children ?? []) as Array<{ id: string; title?: string; content?: string }>;
        const inner = children
          .map((c) => `<div class="b-col">${c.title ? `<h3>${escape(c.title)}</h3>` : ""}${c.content ? `<p>${richHtml(c.content)}</p>` : ""}</div>`)
          .join("");
        return `<div class="b-columns" data-cols="${d.cols ?? 2}">${inner}</div>`;
      }
      case "spacer": {
        const sz = { S: 24, M: 56, L: 96 }[d.size as "S" | "M" | "L"] ?? 56;
        return `<div class="b-spacer" style="height:${sz}px"></div>`;
      }
      case "divider":
        return `<hr class="b-divider"/>`;
      case "hero": {
        const align = d.align === "left" ? "left" : d.align === "right" ? "right" : "center";
        const bg = d.backgroundImage ? ` style="background-image:url('${d.backgroundImage}');"` : "";
        const bgClass = d.backgroundImage ? " has-bg" : "";
        return `<section class="b-hero${bgClass}" data-align="${align}"${bg}>${d.backgroundImage ? '<div class="b-hero-overlay"></div>' : ""}<div class="b-hero-content"><h1>${escape(d.title ?? "")}</h1><p>${escape(d.subtitle ?? "")}</p>${d.ctaText ? `<a class="b-cta" href="${escapeAttr(d.ctaHref ?? "#")}">${escape(d.ctaText)}</a>` : ""}</div></section>`;
      }
      case "cards": {
        const items = (d.items ?? []) as Array<{ emoji: string; title: string; desc: string; image?: string; useImage?: boolean }>;
        const inner = items
          .map((it) => {
            const head = it.useImage && it.image
              ? `<div class="b-card-thumb"><img src="${it.image}" alt=""/></div>`
              : `<div class="b-card-emoji">${escape(it.emoji ?? "")}</div>`;
            return `<article class="b-card">${head}<h3>${escape(it.title ?? "")}</h3><p>${escape(it.desc ?? "")}</p></article>`;
          })
          .join("");
        return `<div class="b-cards" data-cols="${Math.min(4, Math.max(2, items.length))}">${inner}</div>`;
      }
      case "feature":
        return `<section class="b-feature" data-side="${escapeAttr(d.side ?? "left")}"><div class="b-feature-icon">${escape(d.icon ?? "")}</div><div class="b-feature-body"><h2>${escape(d.title ?? "")}</h2><p>${escape(d.desc ?? "")}</p></div></section>`;
      case "faq": {
        const items = (d.items ?? []) as Array<{ q: string; a: string }>;
        return `<dl class="b-faq">${items
          .map((it) => `<details><summary>${escape(it.q ?? "")}</summary><div class="b-faq-a">${escape(it.a ?? "")}</div></details>`)
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
      case "pricing": {
        const plans = (d.plans ?? []) as Array<{ name: string; price: string; period: string; features: string[]; cta: string; href: string; highlighted: boolean }>;
        const inner = plans
          .map((p) => `<article class="b-pricing-plan${p.highlighted ? " highlighted" : ""}"><h3>${escape(p.name)}</h3><div class="b-pricing-price">${escape(p.price)}<span>${escape(p.period ?? "")}</span></div><ul>${(p.features ?? []).map((f) => `<li>${escape(f)}</li>`).join("")}</ul><a class="b-pricing-cta" href="${escapeAttr(p.href || "#")}">${escape(p.cta || "Choisir")}</a></article>`)
          .join("");
        return `<div class="b-pricing" data-cols="${plans.length}">${inner}</div>`;
      }
      case "testimonial": {
        const avatar = d.avatar ? `<img class="b-testimonial-avatar" src="${d.avatar}" alt=""/>` : `<div class="b-testimonial-avatar placeholder">${escape((d.name ?? "?")[0] ?? "?")}</div>`;
        return `<figure class="b-testimonial">${avatar}<blockquote><p><em>${escape(d.text ?? "")}</em></p></blockquote><figcaption><strong>${escape(d.name ?? "")}</strong>${d.role ? ` <span>${escape(d.role)}</span>` : ""}</figcaption></figure>`;
      }
      case "countdown": {
        const target = d.target ?? "";
        const label = escape(d.label ?? "");
        const targetMs = target ? new Date(target).getTime() : 0;
        // Inline JS for countdown (sandbox iframe needs allow-scripts to run; otherwise displays static)
        const script = `(function(){var t=${targetMs};function pad(n){return String(n).padStart(2,'0');}function tick(){var n=new Date().getTime();var d=Math.max(0,t-n);var dd=Math.floor(d/86400000);var hh=Math.floor(d/3600000)%24;var mm=Math.floor(d/60000)%60;var ss=Math.floor(d/1000)%60;var el=document.currentScript&&document.currentScript.previousElementSibling;if(el){el.querySelector('[data-d]').textContent=pad(dd);el.querySelector('[data-h]').textContent=pad(hh);el.querySelector('[data-m]').textContent=pad(mm);el.querySelector('[data-s]').textContent=pad(ss);}}tick();setInterval(tick,1000);})();`;
        return `<div class="b-countdown"><span class="b-countdown-label">${label}</span><div class="b-countdown-grid"><div><b data-d>00</b><span>jours</span></div><div><b data-h>00</b><span>heures</span></div><div><b data-m>00</b><span>min</span></div><div><b data-s>00</b><span>sec</span></div></div></div><scr` + `ipt>${script}</scr` + `ipt>`;
      }
      case "social-links": {
        const items = (d.items ?? []) as Array<{ network: string; url: string }>;
        const inner = items
          .filter((x) => x.url)
          .map((x) => {
            const ic = SOCIAL_ICONS[x.network] ?? { label: x.network, svg: "" };
            return `<a class="b-social" href="${escapeAttr(x.url)}" rel="noopener" target="_blank" aria-label="${escapeAttr(ic.label)}">${ic.svg}</a>`;
          })
          .join("");
        return `<div class="b-social-row">${inner}</div>`;
      }
      case "stats": {
        const items = (d.items ?? []) as Array<{ value: string; label: string }>;
        const inner = items
          .map((x) => `<div class="b-stat"><b>${escape(x.value ?? "")}</b><span>${escape(x.label ?? "")}</span></div>`)
          .join("");
        return `<div class="b-stats" data-cols="${Math.min(4, Math.max(2, items.length))}">${inner}</div>`;
      }
      case "map": {
        const lat = parseFloat(d.lat ?? 0) || 0;
        const lng = parseFloat(d.lng ?? 0) || 0;
        const zoom = parseInt(d.zoom ?? 13) || 13;
        const span = 0.02 * Math.pow(2, 13 - zoom);
        const bbox = `${lng - span},${lat - span / 2},${lng + span},${lat + span / 2}`;
        const url = `https://www.openstreetmap.org/export/embed.html?bbox=${bbox}&layer=mapnik&marker=${lat},${lng}`;
        return `<div class="b-map"><iframe src="${url}" loading="lazy"></iframe></div>`;
      }
      case "form-placeholder": {
        const action = escapeAttr(d.action ?? "#");
        return `<form class="b-form" action="${action}" method="post"><h3>${escape(d.title ?? "Contact")}</h3><label>Nom<input type="text" name="name" required/></label><label>Email<input type="email" name="email" required/></label><label>Message<textarea name="message" rows="5" required></textarea></label><button type="submit">${escape(d.submitText ?? "Envoyer")}</button></form>`;
      }
    }
  }

  function blocksToHtml(bs: Block[], themeId: ThemeId): string {
    const t = THEMES[themeId];
    const body = bs.map(blockHtml).join("\n");
    const themeCss = t.vars;
    const isGlass = themeId === "glass";
    const isNeon = themeId === "neon";
    const isRetro = themeId === "retro";
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
.b-para b, .b-para strong { font-weight: 700; }
.b-para i, .b-para em { font-style: italic; }
.b-quote { border-left: 3px solid var(--accent); padding: 8px 0 8px 20px; margin: 24px 0; color: var(--muted); font-style: italic; }
.b-quote cite { display: block; margin-top: 8px; font-style: normal; color: var(--muted); font-size: 14px; }
.b-list { margin: 16px 0 16px 20px; }
.b-list li { margin: 6px 0; }
.b-code { background: var(--surface); border: 1px solid var(--border); padding: 14px 18px; border-radius: var(--radius); overflow-x: auto; font-family: ui-monospace, 'SF Mono', monospace; font-size: 13.5px; position: relative; }
.b-code::before { content: attr(data-lang); position: absolute; top: 6px; right: 12px; font-size: 10.5px; text-transform: uppercase; letter-spacing: .08em; color: var(--muted); }
.b-image { margin: 24px 0; }
.b-image img { width: 100%; border-radius: var(--radius); display: block; max-height: 540px; }
.b-image[data-fit='cover'] img { object-fit: cover; }
.b-image[data-fit='contain'] img { object-fit: contain; background: var(--surface); }
.b-image[data-filter='grayscale'] img { filter: grayscale(1); }
.b-image[data-filter='sepia'] img { filter: sepia(.7); }
.b-image[data-filter='contrast'] img { filter: contrast(1.4) saturate(1.1); }
.b-image figcaption { color: var(--muted); font-size: 13px; margin-top: 8px; text-align: center; }
.b-image-placeholder, .b-gallery-placeholder, .b-video-placeholder { padding: 40px; text-align: center; background: var(--surface); border: 1px dashed var(--border); border-radius: var(--radius); color: var(--muted); margin: 16px 0; }
.b-gallery { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; margin: 24px 0; }
.b-gallery img { width: 100%; aspect-ratio: 1; object-fit: cover; border-radius: 8px; }
.b-video { margin: 24px 0; aspect-ratio: 16/9; }
.b-video iframe, .b-video video { width: 100%; height: 100%; border: 0; border-radius: var(--radius); }
.b-columns { display: grid; gap: 24px; margin: 32px 0; }
.b-columns[data-cols='2'] { grid-template-columns: 1fr 1fr; }
.b-columns[data-cols='3'] { grid-template-columns: 1fr 1fr 1fr; }
.b-col h3 { margin-bottom: 8px; }
.b-col p { color: var(--muted); }
.b-spacer {}
.b-divider { border: 0; border-top: 1px solid var(--border); margin: 32px 0; }

.b-hero { position: relative; text-align: center; padding: 80px 24px; background: var(--surface); border-radius: var(--radius); margin: 24px 0; overflow: hidden; ${isGlass ? "backdrop-filter: blur(20px);" : ""} }
.b-hero[data-align='left'] { text-align: left; }
.b-hero[data-align='right'] { text-align: right; }
.b-hero.has-bg { background-size: cover; background-position: center; color: #fff; }
.b-hero.has-bg .b-hero-overlay { position: absolute; inset: 0; background: linear-gradient(180deg, rgba(0,0,0,.4), rgba(0,0,0,.65)); }
.b-hero.has-bg h1, .b-hero.has-bg p { color: #fff; }
.b-hero-content { position: relative; z-index: 1; }
.b-hero h1 { font-size: 56px; margin-bottom: 12px; }
.b-hero p { color: var(--muted); font-size: 18px; max-width: 620px; margin: 12px auto; }
.b-hero[data-align='left'] p, .b-hero[data-align='right'] p { margin-left: 0; margin-right: 0; }
.b-cta { display: inline-block; margin-top: 24px; background: var(--accent); color: var(--accent-fg); padding: 14px 28px; border-radius: 999px; font-weight: 600; ${isNeon ? "box-shadow: 0 0 20px rgba(255,0,110,.6);" : ""} }

.b-cards { display: grid; gap: 20px; margin: 32px 0; }
.b-cards[data-cols='2'] { grid-template-columns: repeat(2, 1fr); }
.b-cards[data-cols='3'] { grid-template-columns: repeat(3, 1fr); }
.b-cards[data-cols='4'] { grid-template-columns: repeat(4, 1fr); }
.b-card { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); padding: 24px; ${isGlass ? "backdrop-filter: blur(20px);" : ""} ${isRetro ? "box-shadow: var(--shadow);" : ""} }
.b-card-emoji { font-size: 32px; margin-bottom: 12px; }
.b-card-thumb { margin: -24px -24px 16px; border-radius: var(--radius) var(--radius) 0 0; overflow: hidden; aspect-ratio: 16/9; }
.b-card-thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
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
.b-button { display: inline-block; background: var(--accent); color: var(--accent-fg); padding: 12px 24px; border-radius: 999px; font-weight: 600; ${isNeon ? "box-shadow: 0 0 16px rgba(255,0,110,.5);" : ""} }
.b-button[data-color='ghost'] { background: transparent; color: var(--accent); border: 1px solid var(--accent); }

.b-embed { margin: 24px 0; }

.b-pricing { display: grid; gap: 18px; margin: 32px 0; }
.b-pricing[data-cols='2'] { grid-template-columns: repeat(2, 1fr); }
.b-pricing[data-cols='3'] { grid-template-columns: repeat(3, 1fr); }
.b-pricing-plan { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); padding: 28px 22px; display: flex; flex-direction: column; gap: 14px; }
.b-pricing-plan.highlighted { border-color: var(--accent); transform: scale(1.03); ${isNeon ? "box-shadow: 0 0 28px rgba(255,0,110,.35);" : "box-shadow: var(--shadow);"} }
.b-pricing-plan h3 { font-size: 22px; }
.b-pricing-price { font-size: 36px; font-weight: 700; letter-spacing: -.02em; }
.b-pricing-price span { font-size: 14px; font-weight: 400; color: var(--muted); margin-left: 4px; }
.b-pricing-plan ul { list-style: none; padding: 0; display: flex; flex-direction: column; gap: 8px; flex: 1; }
.b-pricing-plan li { color: var(--muted); padding-left: 20px; position: relative; }
.b-pricing-plan li::before { content: "✓"; position: absolute; left: 0; color: var(--accent); font-weight: 700; }
.b-pricing-cta { text-align: center; background: var(--accent); color: var(--accent-fg); padding: 12px 18px; border-radius: 999px; font-weight: 600; }
.b-pricing-plan:not(.highlighted) .b-pricing-cta { background: transparent; color: var(--accent); border: 1px solid var(--accent); }

.b-testimonial { display: flex; gap: 18px; align-items: flex-start; padding: 24px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); margin: 24px 0; }
.b-testimonial-avatar { width: 64px; height: 64px; border-radius: 50%; object-fit: cover; flex-shrink: 0; }
.b-testimonial-avatar.placeholder { display: flex; align-items: center; justify-content: center; background: var(--accent); color: var(--accent-fg); font-weight: 700; font-size: 24px; }
.b-testimonial blockquote { color: var(--text); margin-bottom: 8px; }
.b-testimonial figcaption strong { font-weight: 600; }
.b-testimonial figcaption span { color: var(--muted); margin-left: 6px; }

.b-countdown { text-align: center; margin: 32px 0; padding: 28px; background: var(--surface); border-radius: var(--radius); }
.b-countdown-label { display: block; color: var(--muted); margin-bottom: 14px; font-size: 14px; text-transform: uppercase; letter-spacing: .08em; }
.b-countdown-grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 14px; max-width: 480px; margin: 0 auto; }
.b-countdown-grid > div { background: var(--bg); border: 1px solid var(--border); border-radius: calc(var(--radius) - 4px); padding: 14px 8px; }
.b-countdown-grid b { display: block; font-size: 32px; font-weight: 700; letter-spacing: -.02em; }
.b-countdown-grid span { display: block; color: var(--muted); font-size: 12px; text-transform: uppercase; }

.b-social-row { display: flex; gap: 14px; justify-content: center; flex-wrap: wrap; margin: 24px 0; }
.b-social { display: inline-flex; align-items: center; justify-content: center; width: 44px; height: 44px; border-radius: 50%; background: var(--surface); border: 1px solid var(--border); color: var(--text); transition: transform .15s, color .15s, border-color .15s; }
.b-social:hover { color: var(--accent); border-color: var(--accent); transform: translateY(-2px); }

.b-stats { display: grid; gap: 18px; margin: 32px 0; text-align: center; }
.b-stats[data-cols='2'] { grid-template-columns: repeat(2, 1fr); }
.b-stats[data-cols='3'] { grid-template-columns: repeat(3, 1fr); }
.b-stats[data-cols='4'] { grid-template-columns: repeat(4, 1fr); }
.b-stat b { display: block; font-size: 44px; font-weight: 700; color: var(--accent); letter-spacing: -.02em; }
.b-stat span { display: block; color: var(--muted); font-size: 14px; margin-top: 4px; }

.b-map { margin: 24px 0; aspect-ratio: 16/9; border-radius: var(--radius); overflow: hidden; border: 1px solid var(--border); }
.b-map iframe { width: 100%; height: 100%; border: 0; }

.b-form { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); padding: 26px; margin: 24px 0; display: flex; flex-direction: column; gap: 12px; max-width: 540px; margin-left: auto; margin-right: auto; }
.b-form h3 { margin-bottom: 6px; }
.b-form label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; color: var(--muted); }
.b-form input, .b-form textarea { background: var(--bg); border: 1px solid var(--border); color: var(--text); padding: 10px 12px; border-radius: calc(var(--radius) - 6px); font-family: inherit; font-size: 14px; outline: none; resize: vertical; }
.b-form input:focus, .b-form textarea:focus { border-color: var(--accent); }
.b-form button { background: var(--accent); color: var(--accent-fg); border: 0; padding: 12px 18px; border-radius: 999px; font-weight: 600; font-size: 14px; cursor: pointer; }

@media (max-width: 720px) {
  .b-columns[data-cols='2'], .b-columns[data-cols='3'],
  .b-cards[data-cols='2'], .b-cards[data-cols='3'], .b-cards[data-cols='4'],
  .b-pricing[data-cols='2'], .b-pricing[data-cols='3'],
  .b-stats[data-cols='3'], .b-stats[data-cols='4'] { grid-template-columns: 1fr; }
  .b-stats[data-cols='2'] { grid-template-columns: repeat(2, 1fr); }
  .b-gallery { grid-template-columns: repeat(2, 1fr); }
  .b-hero h1 { font-size: 36px; }
  .b-pricing-plan.highlighted { transform: none; }
  .b-countdown-grid { grid-template-columns: repeat(2, 1fr); }
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

  // ─── Export HTML ────────────────────────────────────────────────────────
  function downloadHtml() {
    const safeTitle = (title || "site").replace(/[^a-z0-9-_]+/gi, "-").replace(/^-+|-+$/g, "") || "site";
    const blob = new Blob([generatedHtml], { type: "text/html" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${safeTitle}.html`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    setTimeout(() => URL.revokeObjectURL(url), 1000);
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
      { type: "map", label: "Carte", emoji: "🗺" },
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
      { type: "pricing", label: "Pricing", emoji: "💸" },
      { type: "testimonial", label: "Témoignage", emoji: "🗣" },
      { type: "form-placeholder", label: "Formulaire", emoji: "✉" },
    ],
    nav: [
      { type: "navbar", label: "Navbar", emoji: "≡" },
      { type: "footer", label: "Footer", emoji: "▭" },
      { type: "button", label: "Bouton", emoji: "⊕" },
      { type: "social-links", label: "Réseaux", emoji: "🔗" },
    ],
    data: [
      { type: "stats", label: "Stats", emoji: "📊" },
      { type: "countdown", label: "Compte à rebours", emoji: "⏰" },
    ],
    avance: [
      { type: "embed", label: "HTML brut", emoji: "{}" },
    ],
  };

  // ─── Content edit handlers ──────────────────────────────────────────────
  // Plain text handler: heading, list items, quote author
  function onContentEdit(e: Event, id: string, key: string) {
    const el = e.currentTarget as HTMLElement;
    setBlockData(id, key, el.innerText);
  }

  // Rich HTML handler: paragraph text, quote text, column content
  function onRichEdit(e: Event, id: string, key: string) {
    const el = e.currentTarget as HTMLElement;
    setBlockData(id, key, el.innerHTML);
  }

  function onColumnRichEdit(e: Event, blockId: string, colIdx: number, key: "title" | "content") {
    const el = e.currentTarget as HTMLElement;
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const children = ((b.data.children as any[]) ?? []).slice();
    children[colIdx] = { ...children[colIdx], [key]: key === "content" ? el.innerHTML : el.innerText };
    updateBlock(blockId, { children });
  }

  // Action: set content on mount; only re-sync when the value changes
  // externally (e.g. undo/redo) — not on every reactivity tick caused by typing.
  function setRichInit(node: HTMLElement, html: string) {
    let current = html ?? "";
    node.innerHTML = current;
    return {
      update(next: string) {
        const incoming = next ?? "";
        if (incoming !== current && incoming !== node.innerHTML) {
          node.innerHTML = incoming;
        }
        current = incoming;
      },
    };
  }

  function setTextInit(node: HTMLElement, text: string) {
    let current = text ?? "";
    node.textContent = current;
    return {
      update(next: string) {
        const incoming = next ?? "";
        if (incoming !== current && incoming !== node.textContent) {
          node.textContent = incoming;
        }
        current = incoming;
      },
    };
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
    items.push({ emoji: "✨", title: "Nouvelle carte", desc: "Description.", image: "", useImage: false });
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

  function addPricingPlan(blockId: string) {
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const plans = ((b.data.plans as any[]) ?? []).slice();
    if (plans.length >= 3) return;
    plans.push({ name: "Nouveau plan", price: "0 €", period: "/mois", features: ["Feature 1", "Feature 2"], cta: "Choisir", href: "#", highlighted: false });
    updateBlock(blockId, { plans });
  }

  function removePricingPlan(blockId: string, idx: number) {
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const plans = ((b.data.plans as any[]) ?? []).slice();
    plans.splice(idx, 1);
    updateBlock(blockId, { plans });
  }

  function setPricingFeatures(blockId: string, idx: number, value: string) {
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const plans = ((b.data.plans as any[]) ?? []).slice();
    plans[idx] = { ...plans[idx], features: value.split("\n").map((s) => s.trim()).filter(Boolean) };
    updateBlock(blockId, { plans });
  }

  function addStatItem(blockId: string) {
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const items = ((b.data.items as any[]) ?? []).slice();
    if (items.length >= 4) return;
    items.push({ value: "100+", label: "Label" });
    updateBlock(blockId, { items });
  }

  function removeStatItem(blockId: string, idx: number) {
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const items = ((b.data.items as any[]) ?? []).slice();
    items.splice(idx, 1);
    updateBlock(blockId, { items });
  }

  function addSocialLink(blockId: string) {
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const items = ((b.data.items as any[]) ?? []).slice();
    items.push({ network: "twitter", url: "" });
    updateBlock(blockId, { items });
  }

  function removeSocialLink(blockId: string, idx: number) {
    const b = blocks.find((x) => x.id === blockId);
    if (!b) return;
    const items = ((b.data.items as any[]) ?? []).slice();
    items.splice(idx, 1);
    updateBlock(blockId, { items });
  }

  // ─── Rich text floating toolbar ─────────────────────────────────────────
  function isInRichEditable(node: Node | null): boolean {
    let cur: Node | null = node;
    while (cur) {
      if (cur.nodeType === 1) {
        const el = cur as HTMLElement;
        if (el.classList && el.classList.contains("ce-rich")) return true;
      }
      cur = cur.parentNode;
    }
    return false;
  }

  function updateRichToolbar() {
    const sel = window.getSelection();
    if (!sel || sel.rangeCount === 0 || sel.isCollapsed) {
      richToolbar.visible = false;
      return;
    }
    const range = sel.getRangeAt(0);
    if (!isInRichEditable(range.commonAncestorContainer)) {
      richToolbar.visible = false;
      return;
    }
    const rect = range.getBoundingClientRect();
    if (rect.width === 0 && rect.height === 0) {
      richToolbar.visible = false;
      return;
    }
    richToolbar = {
      visible: true,
      x: rect.left + rect.width / 2,
      y: rect.top - 8,
    };
  }

  function applyFormat(cmd: "bold" | "italic" | "createLink") {
    if (cmd === "createLink") {
      const url = prompt("URL ?", "https://");
      if (!url) return;
      try { document.execCommand("createLink", false, url); } catch {}
    } else {
      try { document.execCommand(cmd, false); } catch {}
    }
    // After formatting, sync the edited element back into state
    const sel = window.getSelection();
    if (sel && sel.rangeCount) {
      let n: Node | null = sel.getRangeAt(0).commonAncestorContainer;
      while (n && (n.nodeType !== 1 || !(n as HTMLElement).classList.contains("ce-rich"))) {
        n = n.parentNode;
      }
      if (n) {
        const el = n as HTMLElement;
        const blockId = el.dataset.blockId;
        const fieldKey = el.dataset.fieldKey;
        const colIdx = el.dataset.colIdx;
        if (blockId && fieldKey) {
          if (colIdx !== undefined) {
            const idx = parseInt(colIdx);
            const b = blocks.find((x) => x.id === blockId);
            if (b) {
              const children = ((b.data.children as any[]) ?? []).slice();
              children[idx] = { ...children[idx], [fieldKey]: el.innerHTML };
              updateBlock(blockId, { children });
            }
          } else {
            setBlockData(blockId, fieldKey, el.innerHTML);
          }
        }
      }
    }
    setTimeout(updateRichToolbar, 0);
  }

  // ─── Keyboard shortcuts ─────────────────────────────────────────────────
  function isInputTarget(target: EventTarget | null): boolean {
    if (!target || !(target as HTMLElement).matches) return false;
    const el = target as HTMLElement;
    if (el.matches("input, textarea, select")) return true;
    if (el.isContentEditable) return true;
    return false;
  }

  function onGlobalKey(e: KeyboardEvent) {
    const meta = e.metaKey || e.ctrlKey;
    const inInput = isInputTarget(e.target);

    // Undo / redo (always)
    if (meta && e.key.toLowerCase() === "z") {
      if (inInput && (e.target as HTMLElement).matches("input, textarea")) return;
      e.preventDefault();
      if (e.shiftKey) redo();
      else undo();
      return;
    }

    // Escape
    if (e.key === "Escape") {
      if (pickerOpenAt !== null) {
        pickerOpenAt = null;
        return;
      }
      if (activeBlockId) {
        activeBlockId = null;
        return;
      }
    }

    // Slash → open picker (only when not in input)
    if (e.key === "/" && !inInput) {
      e.preventDefault();
      const idx = activeBlockId
        ? blocks.findIndex((b) => b.id === activeBlockId) + 1
        : blocks.length;
      pickerOpenAt = Math.max(0, idx);
      return;
    }

    if (!activeBlockId) return;

    // Cmd/Ctrl + D → duplicate
    if (meta && e.key.toLowerCase() === "d") {
      e.preventDefault();
      duplicateBlock(activeBlockId);
      return;
    }

    // Cmd/Ctrl + Up/Down → move
    if (meta && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      e.preventDefault();
      moveBlock(activeBlockId, e.key === "ArrowUp" ? -1 : 1);
      return;
    }

    // Backspace / Delete → delete (only outside editable)
    if ((e.key === "Backspace" || e.key === "Delete") && !inInput) {
      e.preventDefault();
      deleteBlock(activeBlockId);
      return;
    }
  }

  // ─── Document click → deselect ──────────────────────────────────────────
  function onDocumentClick(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (!target || !target.closest) return;
    if (target.closest(".block")) return;
    if (target.closest(".rich-toolbar")) return;
    if (target.closest(".picker-overlay")) return;
    activeBlockId = null;
  }

  // ─── localStorage draft ─────────────────────────────────────────────────
  function snapshotState(): DraftSnapshot {
    return {
      title,
      domain,
      theme,
      blocks: deepClone(blocks),
      manualTags: manualTags.slice(),
      metaLang,
      metaCategory,
      metaDescription,
    };
  }

  function applySnapshot(s: DraftSnapshot) {
    isApplyingHistory = true;
    title = s.title ?? "Mon site Torus";
    domain = s.domain ?? "";
    theme = (s.theme as ThemeId) ?? "dark";
    blocks = deepClone(s.blocks ?? []);
    manualTags = (s.manualTags ?? []).slice();
    metaLang = s.metaLang ?? "fr";
    metaCategory = s.metaCategory ?? "personnel";
    metaDescription = s.metaDescription ?? "";
    activeBlockId = null;
    queueMicrotask(() => { isApplyingHistory = false; });
  }

  function saveDraft() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(snapshotState()));
    } catch {
      // ignore quota / privacy errors
    }
  }

  function loadDraft(): boolean {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (!raw) return false;
      const data = JSON.parse(raw) as DraftSnapshot;
      if (!data || typeof data !== "object") return false;
      applySnapshot(data);
      return true;
    } catch {
      return false;
    }
  }

  function clearDraft(silent = false) {
    if (!silent && !confirm("Effacer le brouillon courant et repartir d'une page vierge ?")) return;
    try { localStorage.removeItem(STORAGE_KEY); } catch {}
    isApplyingHistory = true;
    blocks = [];
    title = "Mon site Torus";
    domain = "";
    theme = "dark";
    manualTags = [];
    metaLang = "fr";
    metaCategory = "personnel";
    metaDescription = "";
    activeBlockId = null;
    history = [];
    historyIndex = -1;
    queueMicrotask(() => { isApplyingHistory = false; });
  }

  // ─── Undo / redo history ────────────────────────────────────────────────
  function pushHistorySnapshot() {
    const snap = JSON.stringify(snapshotState());
    if (history[historyIndex] === snap) return;
    // drop redo branch
    history = history.slice(0, historyIndex + 1);
    history.push(snap);
    if (history.length > 30) history = history.slice(history.length - 30);
    historyIndex = history.length - 1;
  }

  function undo() {
    if (historyIndex <= 0) return;
    historyIndex -= 1;
    try {
      applySnapshot(JSON.parse(history[historyIndex]) as DraftSnapshot);
    } catch {}
  }

  function redo() {
    if (historyIndex >= history.length - 1) return;
    historyIndex += 1;
    try {
      applySnapshot(JSON.parse(history[historyIndex]) as DraftSnapshot);
    } catch {}
  }

  const canUndo = $derived(historyIndex > 0);
  const canRedo = $derived(historyIndex < history.length - 1);

  // ─── Effects: load + autosave + history + listeners ─────────────────────
  $effect(() => {
    loadDraft();
    // Seed initial history snapshot
    history = [JSON.stringify(snapshotState())];
    historyIndex = 0;
  });

  // Autosave + debounced history push on state change
  $effect(() => {
    // Read deps to track them
    void title;
    void domain;
    void theme;
    void blocks;
    void manualTags;
    void metaLang;
    void metaCategory;
    void metaDescription;

    if (isApplyingHistory) return;
    if (historyTimer) clearTimeout(historyTimer);
    historyTimer = setTimeout(() => {
      pushHistorySnapshot();
    }, 500);
  });

  $effect(() => {
    const interval = setInterval(saveDraft, 5000);
    return () => clearInterval(interval);
  });

  $effect(() => {
    window.addEventListener("keydown", onGlobalKey);
    document.addEventListener("click", onDocumentClick);
    document.addEventListener("selectionchange", updateRichToolbar);
    return () => {
      window.removeEventListener("keydown", onGlobalKey);
      document.removeEventListener("click", onDocumentClick);
      document.removeEventListener("selectionchange", updateRichToolbar);
    };
  });

  const previewWidth = $derived(
    previewSize === "mobile" ? "375px" :
    previewSize === "tablet" ? "768px" : "100%"
  );
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
      <div class="history-row">
        <button type="button" class="topbar-icon" onclick={undo} disabled={!canUndo} title="Annuler (Cmd+Z)" aria-label="Annuler">↶</button>
        <button type="button" class="topbar-icon" onclick={redo} disabled={!canRedo} title="Rétablir (Cmd+Shift+Z)" aria-label="Rétablir">↷</button>
      </div>
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
      <button type="button" class="topbar-btn" onclick={downloadHtml} title="Télécharger l'HTML">⬇ HTML</button>
      <button type="button" class="topbar-btn" onclick={() => clearDraft()} title="Repartir d'une page vierge">⊘ Nouveau</button>
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
      <button type="button" class="topbar-btn" onclick={() => clearDraft()}>Réinitialiser</button>
    </div>
  {/if}

  <div class="layout">
    <!-- Editor -->
    <section class="editor" aria-label="Éditeur de blocs">
      {#if blocks.length === 0}
        <div class="empty">
          <h2>Choisis un point de départ</h2>
          <div class="templates">
            <button type="button" onclick={() => applyTemplate("landing")}><span>🌅</span><b>Landing Page</b><em>Hero + stats + témoignage</em></button>
            <button type="button" onclick={() => applyTemplate("blog")}><span>📰</span><b>Blog</b><em>Article + image + réseaux</em></button>
            <button type="button" onclick={() => applyTemplate("portfolio")}><span>🎨</span><b>Portfolio</b><em>Hero + stats + galerie</em></button>
            <button type="button" onclick={() => applyTemplate("boutique")}><span>🛍</span><b>Boutique</b><em>Produits + pricing</em></button>
            <button type="button" onclick={() => applyTemplate("perso")}><span>✨</span><b>Page perso</b><em>Bio + compétences</em></button>
            <button type="button" onclick={() => applyTemplate("restaurant")}><span>🍽</span><b>Restaurant</b><em>Menu + carte + horaires</em></button>
            <button type="button" onclick={() => applyTemplate("evenement")}><span>🎉</span><b>Événement</b><em>Hero + countdown + billets</em></button>
            <button type="button" onclick={() => applyTemplate("freelance")}><span>💼</span><b>Freelance</b><em>Hero + features + témoignage</em></button>
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
        <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <div
          class="block"
          class:active={activeBlockId === b.id}
          class:dragging={dragId === b.id}
          class:drag-over-top={dropTargetId === b.id && dropPosition === "top"}
          class:drag-over-bottom={dropTargetId === b.id && dropPosition === "bottom"}
          role="group"
          aria-label={`Bloc ${b.type}`}
          draggable="true"
          ondragstart={(e) => onDragStart(e, b.id)}
          ondragover={(e) => onDragOver(e, b.id)}
          ondragleave={(e) => onDragLeave(e, b.id)}
          ondragend={onDragEnd}
          ondrop={(e) => onDrop(e, b.id)}
          onclick={(e) => setBlockActive(e, b.id)}
          onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") setBlockActive(e, b.id); }}
          tabindex="0"
          data-type={b.type}
        >
          <div class="block-handle" aria-label="Glisser pour réordonner">⋮⋮</div>

          <div class="block-toolbar" aria-label="Actions du bloc">
            <button type="button" onclick={() => moveBlock(b.id, -1)} title="Monter (Cmd+↑)">↑</button>
            <button type="button" onclick={() => moveBlock(b.id, 1)} title="Descendre (Cmd+↓)">↓</button>
            <button type="button" onclick={() => duplicateBlock(b.id)} title="Dupliquer (Cmd+D)">⧉</button>
            <button type="button" onclick={() => deleteBlock(b.id)} title="Supprimer (⌫)">🗑</button>
          </div>

          <div class="block-body">
            {#if b.type === "heading"}
              <div class="row">
                <select value={b.data.level} onchange={(e) => setBlockData(b.id, "level", parseInt((e.currentTarget as HTMLSelectElement).value))}>
                  <option value={1}>H1</option>
                  <option value={2}>H2</option>
                  <option value={3}>H3</option>
                </select>
                <!-- svelte-ignore a11y_missing_content -->
                <h2
                  class="ce h{b.data.level}"
                  contenteditable="true"
                  spellcheck="false"
                  aria-label="Texte du titre"
                  use:setTextInit={b.data.text}
                  oninput={(e) => onContentEdit(e, b.id, "text")}
                ></h2>
              </div>
            {:else if b.type === "paragraph"}
              <p class="ce ce-rich" data-block-id={b.id} data-field-key="text" data-empty={!b.data.text || b.data.text === ""} contenteditable="true"
                use:setRichInit={b.data.text}
                oninput={(e) => onRichEdit(e, b.id, "text")}></p>
            {:else if b.type === "quote"}
              <blockquote>
                <p class="ce ce-rich" data-block-id={b.id} data-field-key="text" contenteditable="true"
                  use:setRichInit={b.data.text}
                  oninput={(e) => onRichEdit(e, b.id, "text")}></p>
                <cite class="ce" contenteditable="true"
                  use:setTextInit={b.data.author}
                  oninput={(e) => onContentEdit(e, b.id, "author")}></cite>
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
                      <span class="ce" contenteditable="true"
                        use:setTextInit={b.data.items[ii]}
                        oninput={(e) => onListItemEdit(e, b.id, ii)}></span>
                      <button type="button" class="mini" onclick={() => removeListItem(b.id, ii)}>×</button>
                    </li>
                  {/each}
                </ul>
                <button type="button" class="mini" onclick={() => addListItem(b.id)}>+ item</button>
              </div>
            {:else if b.type === "code"}
              <div class="row">
                <label class="inline">
                  Langage :
                  <select value={b.data.lang} onchange={(e) => setBlockData(b.id, "lang", (e.currentTarget as HTMLSelectElement).value)}>
                    <option value="javascript">JavaScript</option>
                    <option value="typescript">TypeScript</option>
                    <option value="python">Python</option>
                    <option value="rust">Rust</option>
                    <option value="go">Go</option>
                    <option value="html">HTML</option>
                    <option value="css">CSS</option>
                    <option value="bash">Bash</option>
                    <option value="sql">SQL</option>
                  </select>
                </label>
              </div>
              <textarea class="code-area" rows="6"
                value={b.data.code}
                oninput={(e) => setBlockData(b.id, "code", (e.currentTarget as HTMLTextAreaElement).value)}
              ></textarea>
            {:else if b.type === "image"}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="image-edit"
                role="region"
                aria-label="Zone image"
                ondragover={onImageBlockDragOver}
                ondrop={(e) => onImageBlockDrop(e, b.id, "src")}>
                {#if b.data.src}
                  <img src={b.data.src} alt={b.data.alt}
                    class="img-preview"
                    data-fit={b.data.fit}
                    data-filter={b.data.filter} />
                {:else}
                  <button type="button" class="picker drop-zone" onclick={() => pickImage(b.id, "src")}>
                    <span>🖼</span>
                    <div>
                      <b>Choisir une image</b>
                      <em>ou glisser-déposer un fichier ici</em>
                    </div>
                  </button>
                {/if}
                <input class="caption" placeholder="Légende (optionnel)" value={b.data.caption}
                  oninput={(e) => setBlockData(b.id, "caption", (e.currentTarget as HTMLInputElement).value)}/>
                <input class="caption" placeholder="Texte alternatif (a11y)" value={b.data.alt}
                  oninput={(e) => setBlockData(b.id, "alt", (e.currentTarget as HTMLInputElement).value)}/>
                <div class="row">
                  <label class="inline">
                    Cadrage :
                    <select value={b.data.fit} onchange={(e) => setBlockData(b.id, "fit", (e.currentTarget as HTMLSelectElement).value)}>
                      <option value="cover">Couvrir</option>
                      <option value="contain">Contenir</option>
                    </select>
                  </label>
                  <label class="inline">
                    Filtre :
                    <select value={b.data.filter} onchange={(e) => setBlockData(b.id, "filter", (e.currentTarget as HTMLSelectElement).value)}>
                      <option value="none">Aucun</option>
                      <option value="grayscale">Noir & blanc</option>
                      <option value="sepia">Sépia</option>
                      <option value="contrast">Contraste fort</option>
                    </select>
                  </label>
                </div>
                {#if b.data.src}
                  <button type="button" class="mini" onclick={() => setBlockData(b.id, "src", "")}>Remplacer l'image</button>
                {/if}
              </div>
            {:else if b.type === "gallery"}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="gallery-edit"
                role="region"
                aria-label="Zone galerie"
                ondragover={onImageBlockDragOver}
                ondrop={(e) => onImageBlockDrop(e, b.id, "gallery")}>
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
                    <input class="big-input" placeholder="Titre de la colonne"
                      value={ch.title ?? ""}
                      oninput={(e) => {
                        const t = (e.currentTarget as HTMLInputElement).value;
                        const children = (b.data.children as any[]).slice();
                        children[ii] = { ...children[ii], title: t };
                        updateBlock(b.id, { children });
                      }}/>
                    <p class="ce ce-rich col-content" data-block-id={b.id} data-field-key="content" data-col-idx={ii} contenteditable="true"
                      use:setRichInit={ch.content ?? ""}
                      oninput={(e) => onColumnRichEdit(e, b.id, ii, "content")}></p>
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
                <div class="row">
                  <label class="inline">
                    Alignement :
                    <select value={b.data.align ?? "center"} onchange={(e) => setBlockData(b.id, "align", (e.currentTarget as HTMLSelectElement).value)}>
                      <option value="left">Gauche</option>
                      <option value="center">Centre</option>
                      <option value="right">Droite</option>
                    </select>
                  </label>
                  {#if b.data.backgroundImage}
                    <button type="button" class="mini" onclick={() => setBlockData(b.id, "backgroundImage", "")}>Retirer le fond</button>
                  {:else}
                    <button type="button" class="mini" onclick={() => pickImage(b.id, "backgroundImage")}>+ Image de fond</button>
                  {/if}
                </div>
                {#if b.data.backgroundImage}
                  <div class="hero-bg-preview" style="background-image: url('{b.data.backgroundImage}');"></div>
                {/if}
              </div>
            {:else if b.type === "cards"}
              <div class="cards-edit">
                {#each b.data.items as _it, ii (b.id + "-c-" + ii)}
                  <div class="card-cell">
                    <div class="row">
                      <label class="inline">
                        <input type="checkbox" checked={!!b.data.items[ii].useImage}
                          onchange={(e) => {
                            const items = (b.data.items as any[]).slice();
                            items[ii] = { ...items[ii], useImage: (e.currentTarget as HTMLInputElement).checked };
                            updateBlock(b.id, { items });
                          }}/>
                        Image
                      </label>
                    </div>
                    {#if b.data.items[ii].useImage}
                      {#if b.data.items[ii].image}
                        <img class="card-thumb-edit" src={b.data.items[ii].image} alt=""/>
                      {/if}
                      <button type="button" class="mini" onclick={() => pickCardImage(b.id, ii)}>{b.data.items[ii].image ? "Remplacer" : "Choisir image"}</button>
                    {:else}
                      <input class="emoji-input" value={b.data.items[ii].emoji} placeholder="🎯"
                        oninput={(e) => {
                          const items = (b.data.items as any[]).slice();
                          items[ii] = { ...items[ii], emoji: (e.currentTarget as HTMLInputElement).value };
                          updateBlock(b.id, { items });
                        }}/>
                    {/if}
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
            {:else if b.type === "pricing"}
              <div class="pricing-edit">
                {#each b.data.plans as _p, ii (b.id + "-p-" + ii)}
                  <div class="pricing-cell" class:highlight={_p.highlighted}>
                    <input class="big-input" value={b.data.plans[ii].name} placeholder="Nom du plan"
                      oninput={(e) => {
                        const plans = (b.data.plans as any[]).slice();
                        plans[ii] = { ...plans[ii], name: (e.currentTarget as HTMLInputElement).value };
                        updateBlock(b.id, { plans });
                      }}/>
                    <div class="row">
                      <input class="caption" value={b.data.plans[ii].price} placeholder="Prix (ex: 9 €)"
                        oninput={(e) => {
                          const plans = (b.data.plans as any[]).slice();
                          plans[ii] = { ...plans[ii], price: (e.currentTarget as HTMLInputElement).value };
                          updateBlock(b.id, { plans });
                        }}/>
                      <input class="caption" value={b.data.plans[ii].period} placeholder="Période (/mois)"
                        oninput={(e) => {
                          const plans = (b.data.plans as any[]).slice();
                          plans[ii] = { ...plans[ii], period: (e.currentTarget as HTMLInputElement).value };
                          updateBlock(b.id, { plans });
                        }}/>
                    </div>
                    <textarea class="card-desc" placeholder="Une feature par ligne"
                      value={(b.data.plans[ii].features ?? []).join("\n")}
                      oninput={(e) => setPricingFeatures(b.id, ii, (e.currentTarget as HTMLTextAreaElement).value)}></textarea>
                    <div class="row">
                      <input class="caption" value={b.data.plans[ii].cta} placeholder="Texte du bouton"
                        oninput={(e) => {
                          const plans = (b.data.plans as any[]).slice();
                          plans[ii] = { ...plans[ii], cta: (e.currentTarget as HTMLInputElement).value };
                          updateBlock(b.id, { plans });
                        }}/>
                      <input class="caption" value={b.data.plans[ii].href} placeholder="Lien"
                        oninput={(e) => {
                          const plans = (b.data.plans as any[]).slice();
                          plans[ii] = { ...plans[ii], href: (e.currentTarget as HTMLInputElement).value };
                          updateBlock(b.id, { plans });
                        }}/>
                    </div>
                    <div class="row">
                      <label class="inline">
                        <input type="checkbox" checked={!!b.data.plans[ii].highlighted}
                          onchange={(e) => {
                            const plans = (b.data.plans as any[]).slice();
                            plans[ii] = { ...plans[ii], highlighted: (e.currentTarget as HTMLInputElement).checked };
                            updateBlock(b.id, { plans });
                          }}/>
                        Plan mis en avant
                      </label>
                      <button type="button" class="mini" onclick={() => removePricingPlan(b.id, ii)}>× supprimer</button>
                    </div>
                  </div>
                {/each}
                {#if b.data.plans.length < 3}
                  <button type="button" class="mini" onclick={() => addPricingPlan(b.id)}>+ Plan</button>
                {/if}
              </div>
            {:else if b.type === "testimonial"}
              <div class="testimonial-edit">
                <div class="row">
                  {#if b.data.avatar}
                    <img class="avatar-edit" src={b.data.avatar} alt=""/>
                  {/if}
                  <button type="button" class="mini" onclick={() => pickAvatar(b.id)}>{b.data.avatar ? "Remplacer" : "Avatar"}</button>
                  {#if b.data.avatar}
                    <button type="button" class="mini" onclick={() => setBlockData(b.id, "avatar", "")}>Retirer</button>
                  {/if}
                </div>
                <input class="big-input" value={b.data.name} placeholder="Nom"
                  oninput={(e) => setBlockData(b.id, "name", (e.currentTarget as HTMLInputElement).value)}/>
                <input class="caption" value={b.data.role} placeholder="Rôle / fonction"
                  oninput={(e) => setBlockData(b.id, "role", (e.currentTarget as HTMLInputElement).value)}/>
                <textarea class="card-desc" placeholder="Témoignage"
                  value={b.data.text}
                  oninput={(e) => setBlockData(b.id, "text", (e.currentTarget as HTMLTextAreaElement).value)}></textarea>
              </div>
            {:else if b.type === "countdown"}
              <div class="row">
                <label class="inline">
                  Date cible :
                  <input type="date" class="caption" value={b.data.target}
                    oninput={(e) => setBlockData(b.id, "target", (e.currentTarget as HTMLInputElement).value)}/>
                </label>
              </div>
              <input class="caption" value={b.data.label} placeholder="Label (ex: Lancement dans)"
                oninput={(e) => setBlockData(b.id, "label", (e.currentTarget as HTMLInputElement).value)}/>
              <p class="hint">Le compte à rebours s'affiche en temps réel sur le site publié.</p>
            {:else if b.type === "social-links"}
              <div class="social-edit">
                {#each b.data.items as _it, ii (b.id + "-s-" + ii)}
                  <div class="row">
                    <select value={b.data.items[ii].network}
                      onchange={(e) => {
                        const items = (b.data.items as any[]).slice();
                        items[ii] = { ...items[ii], network: (e.currentTarget as HTMLSelectElement).value };
                        updateBlock(b.id, { items });
                      }}>
                      <option value="twitter">Twitter / X</option>
                      <option value="github">GitHub</option>
                      <option value="linkedin">LinkedIn</option>
                      <option value="discord">Discord</option>
                      <option value="youtube">YouTube</option>
                      <option value="tiktok">TikTok</option>
                      <option value="mastodon">Mastodon</option>
                    </select>
                    <input class="caption" value={b.data.items[ii].url} placeholder="URL"
                      oninput={(e) => {
                        const items = (b.data.items as any[]).slice();
                        items[ii] = { ...items[ii], url: (e.currentTarget as HTMLInputElement).value };
                        updateBlock(b.id, { items });
                      }}/>
                    <button type="button" class="mini" onclick={() => removeSocialLink(b.id, ii)}>×</button>
                  </div>
                {/each}
                <button type="button" class="mini" onclick={() => addSocialLink(b.id)}>+ Réseau</button>
              </div>
            {:else if b.type === "stats"}
              <div class="stats-edit">
                {#each b.data.items as _it, ii (b.id + "-st-" + ii)}
                  <div class="stat-cell">
                    <input class="big-input" value={b.data.items[ii].value} placeholder="1 200+"
                      oninput={(e) => {
                        const items = (b.data.items as any[]).slice();
                        items[ii] = { ...items[ii], value: (e.currentTarget as HTMLInputElement).value };
                        updateBlock(b.id, { items });
                      }}/>
                    <input class="caption" value={b.data.items[ii].label} placeholder="Utilisateurs"
                      oninput={(e) => {
                        const items = (b.data.items as any[]).slice();
                        items[ii] = { ...items[ii], label: (e.currentTarget as HTMLInputElement).value };
                        updateBlock(b.id, { items });
                      }}/>
                    <button type="button" class="mini" onclick={() => removeStatItem(b.id, ii)}>×</button>
                  </div>
                {/each}
                {#if b.data.items.length < 4}
                  <button type="button" class="mini" onclick={() => addStatItem(b.id)}>+ Stat</button>
                {/if}
              </div>
            {:else if b.type === "map"}
              <div class="row">
                <label class="inline">Lat
                  <input class="caption num" type="number" step="0.0001" value={b.data.lat}
                    oninput={(e) => setBlockData(b.id, "lat", parseFloat((e.currentTarget as HTMLInputElement).value))}/>
                </label>
                <label class="inline">Lng
                  <input class="caption num" type="number" step="0.0001" value={b.data.lng}
                    oninput={(e) => setBlockData(b.id, "lng", parseFloat((e.currentTarget as HTMLInputElement).value))}/>
                </label>
                <label class="inline">Zoom
                  <input class="caption num" type="number" min="3" max="18" value={b.data.zoom}
                    oninput={(e) => setBlockData(b.id, "zoom", parseInt((e.currentTarget as HTMLInputElement).value))}/>
                </label>
              </div>
              <p class="hint">OpenStreetMap embed — la carte interactive s'affiche dans l'aperçu et sur le site publié.</p>
            {:else if b.type === "form-placeholder"}
              <input class="big-input" value={b.data.title} placeholder="Titre du formulaire"
                oninput={(e) => setBlockData(b.id, "title", (e.currentTarget as HTMLInputElement).value)}/>
              <input class="caption" value={b.data.action} placeholder="mailto:contact@example.com ou URL"
                oninput={(e) => setBlockData(b.id, "action", (e.currentTarget as HTMLInputElement).value)}/>
              <input class="caption" value={b.data.submitText} placeholder="Texte du bouton"
                oninput={(e) => setBlockData(b.id, "submitText", (e.currentTarget as HTMLInputElement).value)}/>
              <p class="hint">Formulaire purement cosmétique — pas de backend. Utilise un mailto: ou un endpoint externe.</p>
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
      {#if !showCode}
        <div class="preview-bar">
          <button type="button" class:active={previewSize === "mobile"} onclick={() => (previewSize = "mobile")} title="Mobile (375px)" aria-label="Aperçu mobile">📱</button>
          <button type="button" class:active={previewSize === "tablet"} onclick={() => (previewSize = "tablet")} title="Tablette (768px)" aria-label="Aperçu tablette">📊</button>
          <button type="button" class:active={previewSize === "desktop"} onclick={() => (previewSize = "desktop")} title="Desktop (100%)" aria-label="Aperçu desktop">🖥</button>
        </div>
      {/if}
      {#if showCode}
        <pre class="code-view"><code>{generatedHtml}</code></pre>
      {:else}
        <div class="preview-frame-wrap" data-size={previewSize}>
          <iframe
            bind:this={previewFrame}
            title="Aperçu"
            sandbox="allow-same-origin allow-scripts"
            style="width: {previewWidth};"
          ></iframe>
        </div>
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

  {#if richToolbar.visible}
    <div
      class="rich-toolbar"
      style="left: {richToolbar.x}px; top: {richToolbar.y}px;"
      onmousedown={(e) => e.preventDefault()}
      role="toolbar"
      tabindex="-1"
      aria-label="Mise en forme inline"
    >
      <button type="button" onclick={() => applyFormat("bold")} title="Gras (Cmd+B)"><b>B</b></button>
      <button type="button" onclick={() => applyFormat("italic")} title="Italique (Cmd+I)"><i>I</i></button>
      <button type="button" onclick={() => applyFormat("createLink")} title="Lien">🔗</button>
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

/* ─── Topbar ───────────────────────────────────────────────────────────── */
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
.topbar-right { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
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

/* History buttons */
.history-row { display: flex; gap: 2px; padding: 0 6px 0 0; border-right: 1px solid var(--color-border, rgba(255,255,255,.06)); margin-right: 4px; }
.topbar-icon {
  background: var(--color-bg-2, #111116);
  border: 1px solid var(--color-border, rgba(255,255,255,.06));
  color: var(--color-text-1, #a0a0b0);
  width: 30px; height: 30px;
  border-radius: 6px;
  font-size: 16px;
  cursor: pointer;
  display: inline-flex; align-items: center; justify-content: center;
}
.topbar-icon:hover:not(:disabled) { color: var(--color-accent, #00E5CC); border-color: var(--color-accent, #00E5CC); }
.topbar-icon:disabled { opacity: .3; cursor: not-allowed; }

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

.theme-row { display: flex; gap: 5px; padding: 0 8px; border-right: 1px solid var(--color-border, rgba(255,255,255,.06)); margin-right: 4px; flex-wrap: wrap; max-width: 240px; }
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
  align-items: flex-end;
  animation: slideDown .18s ease;
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

/* ─── Preview ──────────────────────────────────────────────────────────── */
.preview {
  border-left: 1px solid var(--color-border, rgba(255,255,255,.06));
  background: var(--color-bg-1, #0a0a0d);
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
.preview-bar {
  display: flex;
  gap: 4px;
  padding: 6px 10px;
  background: var(--color-bg-2, #111116);
  border-bottom: 1px solid var(--color-border, rgba(255,255,255,.06));
  flex-shrink: 0;
  justify-content: center;
}
.preview-bar button {
  background: transparent;
  border: 1px solid transparent;
  color: var(--color-text-2, #8888a0);
  padding: 4px 10px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 14px;
}
.preview-bar button:hover { color: var(--color-text-0, #e8e8f0); }
.preview-bar button.active { background: var(--color-bg-1, #0a0a0d); color: var(--color-accent, #00E5CC); border-color: var(--color-border, rgba(255,255,255,.06)); }
.preview-frame-wrap {
  flex: 1;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: 12px;
  overflow: auto;
  background: var(--color-bg-1, #0a0a0d);
}
.preview-frame-wrap iframe {
  height: 100%;
  border: 0;
  background: white;
  border-radius: 6px;
  transition: width .25s ease;
  box-shadow: 0 0 0 1px var(--color-border, rgba(255,255,255,.06));
}
.preview-frame-wrap[data-size='mobile'] iframe,
.preview-frame-wrap[data-size='tablet'] iframe {
  border-radius: 18px;
  box-shadow: 0 0 0 6px #1a1a1f, 0 0 0 7px var(--color-border, rgba(255,255,255,.06));
  margin: 12px 0;
  min-height: 600px;
  height: calc(100% - 24px);
}
.preview-frame-wrap[data-size='desktop'] iframe {
  width: 100%;
}
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

/* ─── Empty state ──────────────────────────────────────────────────────── */
.empty {
  text-align: center;
  padding: 60px 24px 80px;
  max-width: 760px;
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

/* ─── Block ────────────────────────────────────────────────────────────── */
.block {
  position: relative;
  background: var(--color-bg-1, #0a0a0d);
  border: 1px solid var(--color-border, rgba(255,255,255,.06));
  border-radius: 12px;
  padding: 16px 20px 16px 36px;
  margin: 0 0 4px;
  cursor: grab;
  transition: border-color .12s, box-shadow .12s, opacity .12s;
  animation: blockSlideIn .18s ease;
  outline: none;
}
.block:active { cursor: grabbing; }
.block:hover { border-color: var(--color-border-hover, rgba(255,255,255,.12)); }
.block:hover .block-toolbar { opacity: 1; }
.block.active {
  border-color: var(--color-accent, #00E5CC);
  box-shadow: 0 0 0 1px var(--color-accent, #00E5CC);
}
.block.active .block-toolbar { opacity: 1; }
.block.dragging { opacity: 0.4; }
.block.drag-over-top::before {
  content: "";
  position: absolute;
  top: -2px;
  left: 0;
  right: 0;
  height: 3px;
  background: var(--color-accent, #00E5CC);
  border-radius: 2px;
  z-index: 5;
}
.block.drag-over-bottom::after {
  content: "";
  position: absolute;
  bottom: -2px;
  left: 0;
  right: 0;
  height: 3px;
  background: var(--color-accent, #00E5CC);
  border-radius: 2px;
  z-index: 5;
}

@keyframes blockSlideIn {
  from { opacity: 0; transform: translateY(-8px); }
  to { opacity: 1; transform: translateY(0); }
}
@keyframes slideDown {
  from { opacity: 0; transform: translateY(-6px); }
  to { opacity: 1; transform: translateY(0); }
}
@keyframes fadeUp {
  from { opacity: 0; transform: translateY(8px); }
  to { opacity: 1; transform: translateY(0); }
}

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
  transition: opacity .15s;
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
.hint { font-size: 12px; color: var(--color-text-2, #8888a0); margin-top: 6px; }

.ce {
  outline: none;
  cursor: text;
  border-radius: 4px;
  padding: 2px 4px;
  margin: -2px -4px;
  word-break: break-word;
  min-height: 1.4em;
}
.ce:focus { background: var(--color-bg-2, #111116); }
.ce.h1 { font-size: 32px; font-weight: 700; letter-spacing: -.02em; }
.ce.h2 { font-size: 24px; font-weight: 600; }
.ce.h3 { font-size: 18px; font-weight: 600; }
.ce-rich:empty::before {
  content: "Cliquez pour écrire…";
  color: var(--color-text-3, #444458);
  pointer-events: none;
}
.ce-rich :global(b),
.ce-rich :global(strong) { font-weight: 700; }
.ce-rich :global(i),
.ce-rich :global(em) { font-style: italic; }
.ce-rich :global(a) {
  color: var(--color-accent, #00E5CC);
  text-decoration: underline;
}

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
.image-edit .img-preview { max-width: 100%; max-height: 320px; border-radius: 8px; object-fit: cover; display: block; }
.image-edit .img-preview[data-fit='contain'] { object-fit: contain; background: var(--color-bg-2, #111116); }
.image-edit .img-preview[data-filter='grayscale'] { filter: grayscale(1); }
.image-edit .img-preview[data-filter='sepia'] { filter: sepia(.7); }
.image-edit .img-preview[data-filter='contrast'] { filter: contrast(1.4) saturate(1.1); }

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
  gap: 12px;
  text-align: left;
}
.picker:hover { border-color: var(--color-accent, #00E5CC); color: var(--color-accent, #00E5CC); }
.drop-zone { width: 100%; padding: 32px 24px; }
.drop-zone span { font-size: 28px; }
.drop-zone div { display: flex; flex-direction: column; gap: 2px; }
.drop-zone b { font-size: 14px; font-weight: 600; }
.drop-zone em { font-size: 12px; font-style: normal; color: var(--color-text-2, #8888a0); }

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
.caption.num { width: 100px; }
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
.col-cell { background: var(--color-bg-2, #111116); border: 1px solid var(--color-border, rgba(255,255,255,.06)); border-radius: 8px; padding: 12px; display: flex; flex-direction: column; gap: 8px; }
.col-content {
  background: var(--color-bg-1, #0a0a0d);
  border: 1px solid var(--color-border, rgba(255,255,255,.06));
  color: var(--color-text-0, #e8e8f0);
  padding: 10px 12px;
  border-radius: 6px;
  min-height: 80px;
  outline: none;
}
.col-content:empty::before {
  content: "Contenu de la colonne…";
  color: var(--color-text-3, #444458);
}

.divider-preview { border-top: 1px solid var(--color-border, rgba(255,255,255,.18)); margin: 8px 0; }

.hero-edit { display: flex; flex-direction: column; gap: 8px; }
.hero-bg-preview {
  width: 100%;
  height: 120px;
  border-radius: 8px;
  background-size: cover;
  background-position: center;
  border: 1px solid var(--color-border, rgba(255,255,255,.06));
}

.cards-edit { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 10px; }
.card-cell { background: var(--color-bg-2, #111116); border: 1px solid var(--color-border, rgba(255,255,255,.06)); border-radius: 8px; padding: 10px; display: flex; flex-direction: column; gap: 6px; position: relative; }
.card-thumb-edit { width: 100%; aspect-ratio: 16/9; object-fit: cover; border-radius: 6px; display: block; }
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

/* Pricing editor */
.pricing-edit { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 12px; }
.pricing-cell { background: var(--color-bg-2, #111116); border: 1px solid var(--color-border, rgba(255,255,255,.06)); border-radius: 8px; padding: 12px; display: flex; flex-direction: column; gap: 8px; }
.pricing-cell.highlight { border-color: var(--color-accent, #00E5CC); }

/* Testimonial editor */
.testimonial-edit { display: flex; flex-direction: column; gap: 8px; }
.avatar-edit { width: 56px; height: 56px; border-radius: 50%; object-fit: cover; }

/* Stats editor */
.stats-edit { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 8px; }
.stat-cell { background: var(--color-bg-2, #111116); border: 1px solid var(--color-border, rgba(255,255,255,.06)); border-radius: 8px; padding: 10px; display: flex; flex-direction: column; gap: 6px; }

/* Social editor */
.social-edit { display: flex; flex-direction: column; gap: 6px; }

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
  animation: fadeUp .15s ease;
}
.picker-panel {
  background: var(--color-bg-1, #0a0a0d);
  border: 1px solid var(--color-border, rgba(255,255,255,.12));
  border-radius: 14px;
  width: min(620px, 92vw);
  max-height: 80vh;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  animation: fadeUp .2s ease;
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

/* Rich text floating toolbar */
.rich-toolbar {
  position: fixed;
  z-index: 150;
  display: inline-flex;
  gap: 2px;
  padding: 4px;
  background: #1a1a22;
  border: 1px solid rgba(255,255,255,.12);
  border-radius: 8px;
  box-shadow: 0 6px 24px rgba(0,0,0,.5);
  transform: translate(-50%, -100%);
  animation: fadeUp .12s ease;
}
.rich-toolbar button {
  background: transparent;
  border: 0;
  color: #e8e8f0;
  width: 30px;
  height: 30px;
  border-radius: 5px;
  font-size: 14px;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.rich-toolbar button:hover { background: rgba(255,255,255,.08); color: var(--color-accent, #00E5CC); }
.rich-toolbar button b { font-weight: 700; }
.rich-toolbar button i { font-style: italic; }
</style>

