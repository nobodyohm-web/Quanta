<script lang="ts">
  // V3.2 : import { invoke } from "@tauri-apps/api/core";
  // (publish_site / get_my_site / etc. à wirer avec page_store étendu)

  type BlockType = "heading" | "paragraph" | "image" | "link" | "code";
  type Block = { id: string; type: BlockType; value: string; alt?: string };

  let title = $state("");
  let domain = $state("");
  let blocks = $state<Block[]>([
    { id: crypto.randomUUID(), type: "heading", value: "Bienvenue" },
    { id: crypto.randomUUID(), type: "paragraph", value: "Ceci est ma première page Torus." },
  ]);
  let saving = $state(false);
  let saved = $state(false);

  function addBlock(type: BlockType) {
    blocks = [...blocks, { id: crypto.randomUUID(), type, value: "" }];
  }
  function removeBlock(id: string) {
    blocks = blocks.filter(b => b.id !== id);
  }
  function moveBlock(id: string, dir: -1 | 1) {
    const i = blocks.findIndex(b => b.id === id);
    if (i < 0) return;
    const j = i + dir;
    if (j < 0 || j >= blocks.length) return;
    const arr = [...blocks];
    [arr[i], arr[j]] = [arr[j], arr[i]];
    blocks = arr;
  }

  const html = $derived(blocks.map(b => {
    const v = escape(b.value);
    switch (b.type) {
      case "heading":   return `<h2>${v}</h2>`;
      case "paragraph": return `<p>${v}</p>`;
      case "image":     return `<img src="${v}" alt="${escape(b.alt ?? '')}" />`;
      case "link":      return `<a href="${v}">${v}</a>`;
      case "code":      return `<pre><code>${v}</code></pre>`;
    }
  }).join("\n"));

  const preview = $derived(`<!doctype html><html><head><meta charset="utf-8"><title>${escape(title)}</title><style>body{font-family:Inter,sans-serif;color:#eee;background:#0a0a0a;padding:24px;line-height:1.6;}h2{color:#00DC82;}a{color:#00DC82;}</style></head><body>${html}</body></html>`);

  function escape(s: string): string {
    return s.replace(/[&<>"']/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;","\"":"&quot;","'":"&#39;"}[c]!));
  }

  async function publish() {
    saving = true;
    saved = false;
    try {
      // V3.2 wiring :
      // await invoke<void>("publish_site", { title, domain, html, blocks });
      await new Promise(r => setTimeout(r, 400));
      saved = true;
      setTimeout(() => saved = false, 3000);
    } finally {
      saving = false;
    }
  }
</script>

<div class="builder">
  <header class="bar">
    <input class="title" type="text" placeholder="Titre du site" bind:value={title} />
    <input class="domain" type="text" placeholder="alex.torus" bind:value={domain} />
    <button class="pubbtn" onclick={publish} disabled={saving || !title.trim()}>
      {saving ? "Publication…" : saved ? "✓ Publié" : "Publier"}
    </button>
  </header>

  <div class="layout">
    <aside class="palette">
      <h3>Blocs</h3>
      <button onclick={() => addBlock("heading")}>+ Titre</button>
      <button onclick={() => addBlock("paragraph")}>+ Paragraphe</button>
      <button onclick={() => addBlock("image")}>+ Image</button>
      <button onclick={() => addBlock("link")}>+ Lien</button>
      <button onclick={() => addBlock("code")}>+ Code</button>
    </aside>

    <section class="editor">
      {#each blocks as b, i (b.id)}
        <div class="block">
          <div class="block-head">
            <span class="block-type">{b.type}</span>
            <button onclick={() => moveBlock(b.id, -1)} disabled={i === 0}>↑</button>
            <button onclick={() => moveBlock(b.id, 1)} disabled={i === blocks.length - 1}>↓</button>
            <button onclick={() => removeBlock(b.id)} aria-label="Supprimer">×</button>
          </div>
          {#if b.type === "paragraph" || b.type === "code"}
            <textarea bind:value={b.value} placeholder={b.type === "code" ? "code source" : "texte"}></textarea>
          {:else if b.type === "image"}
            <input type="text" bind:value={b.value} placeholder="cid ou URL torus://" />
            <input type="text" bind:value={b.alt} placeholder="alt (accessibilité)" />
          {:else}
            <input type="text" bind:value={b.value} placeholder={b.type === "link" ? "torus://..." : ""} />
          {/if}
        </div>
      {/each}
      {#if blocks.length === 0}
        <div class="empty">Ajoute un bloc pour commencer.</div>
      {/if}
    </section>

    <aside class="preview">
      <h3>Aperçu</h3>
      <iframe title="aperçu" srcdoc={preview} sandbox="allow-same-origin"></iframe>
    </aside>
  </div>
</div>

<style>
  .builder { display: flex; flex-direction: column; height: 100%; background: var(--color-bg-0); }
  .bar {
    display: flex; gap: 8px; padding: 12px 16px;
    border-bottom: 1px solid var(--color-border);
  }
  .title, .domain {
    padding: 8px 12px;
    background: var(--color-bg-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-0);
    font-size: 13px;
  }
  .title { flex: 1; }
  .domain { flex: 0 0 200px; font-family: var(--font-mono); }
  .pubbtn {
    padding: 8px 16px;
    background: var(--color-accent);
    color: #000;
    border: none;
    border-radius: var(--radius-sm);
    font-weight: 600;
    cursor: pointer;
  }
  .pubbtn:disabled { opacity: 0.4; cursor: default; }

  .layout {
    flex: 1;
    display: grid;
    grid-template-columns: 180px 1fr 360px;
    gap: 0;
    min-height: 0;
  }
  aside, .editor { padding: 16px; overflow: auto; }
  aside { border-left: 1px solid var(--color-border); display: flex; flex-direction: column; gap: 6px; }
  .palette { border-left: 0; border-right: 1px solid var(--color-border); }
  aside h3 {
    font-size: 11px; font-weight: 600;
    color: var(--color-text-2);
    text-transform: uppercase; letter-spacing: 0.06em;
    margin: 0 0 8px;
  }
  .palette button {
    padding: 8px 12px;
    background: var(--color-bg-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-1);
    text-align: left;
    cursor: pointer;
    font-size: 13px;
  }
  .palette button:hover { color: var(--color-text-0); }

  .block {
    margin-bottom: 12px;
    padding: 12px;
    background: var(--color-bg-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
  }
  .block-head {
    display: flex; gap: 6px; align-items: center;
    margin-bottom: 8px;
    color: var(--color-text-2);
    font-size: 11px;
  }
  .block-type {
    flex: 1;
    text-transform: uppercase; letter-spacing: 0.06em;
  }
  .block-head button {
    padding: 2px 8px;
    background: transparent;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-2);
    cursor: pointer;
  }
  .block-head button:disabled { opacity: 0.3; cursor: default; }
  .block input, .block textarea {
    width: 100%;
    padding: 8px 10px;
    background: var(--color-bg-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-0);
    font-family: inherit;
    font-size: 13px;
    margin-bottom: 6px;
  }
  .block textarea { min-height: 70px; resize: vertical; font-family: var(--font-mono); }
  .empty {
    color: var(--color-text-2);
    text-align: center;
    padding: 32px;
  }

  .preview iframe {
    width: 100%; height: calc(100% - 28px);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: #0a0a0a;
  }
</style>
