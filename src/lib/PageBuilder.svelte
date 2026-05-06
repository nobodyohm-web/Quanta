<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  // ── State ──
  let title = $state("Ma Page");
  let domain = $state("");
  let activeTab = $state<"html" | "css" | "js">("html");
  let publishing = $state(false);
  let published = $state(false);
  let pubError = $state("");
  let previewFrame: HTMLIFrameElement;

  // ── Code editors ──
  let htmlCode = $state(`<div class="hero">
  <h1>Bienvenue sur Torus</h1>
  <p>Le Web souverain, sans serveur.</p>
  <a href="#" class="btn">Explorer →</a>
</div>

<section class="features">
  <div class="card">
    <h3>🔒 Chiffré</h3>
    <p>Vos données restent les vôtres.</p>
  </div>
  <div class="card">
    <h3>🌐 P2P</h3>
    <p>Hébergé par le réseau, pas un cloud.</p>
  </div>
  <div class="card">
    <h3>💰 Récompensé</h3>
    <p>Gagnez des QUANTA en contribuant.</p>
  </div>
</section>`);

  let cssCode = $state(`* { margin: 0; padding: 0; box-sizing: border-box; }
body {
  font-family: 'Inter', system-ui, sans-serif;
  background: #0a0a0a;
  color: #e0e0e0;
  line-height: 1.6;
}
.hero {
  text-align: center;
  padding: 80px 24px 60px;
}
.hero h1 {
  font-size: 42px;
  font-weight: 700;
  letter-spacing: -0.03em;
  background: linear-gradient(135deg, #00E5CC, #00DC82);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
}
.hero p {
  margin-top: 12px;
  font-size: 18px;
  color: #888;
}
.btn {
  display: inline-block;
  margin-top: 24px;
  padding: 12px 28px;
  background: #00E5CC;
  color: #000;
  border-radius: 8px;
  text-decoration: none;
  font-weight: 600;
  transition: opacity 0.2s;
}
.btn:hover { opacity: 0.85; }
.features {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 16px;
  padding: 0 24px 60px;
  max-width: 800px;
  margin: 0 auto;
}
.card {
  padding: 24px;
  background: #111;
  border: 1px solid #222;
  border-radius: 12px;
}
.card h3 { font-size: 18px; margin-bottom: 8px; }
.card p { font-size: 14px; color: #888; }`);

  let jsCode = $state(`// JavaScript optionnel
document.querySelectorAll('.card').forEach(card => {
  card.addEventListener('mouseenter', () => {
    card.style.borderColor = '#00E5CC';
    card.style.transform = 'translateY(-2px)';
    card.style.transition = 'all 0.2s ease';
  });
  card.addEventListener('mouseleave', () => {
    card.style.borderColor = '#222';
    card.style.transform = 'translateY(0)';
  });
});`);

  // ── Build full HTML page for preview ──
  const fullHtml = $derived(`<!doctype html>
<html lang="fr">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${title}</title>
  <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap" rel="stylesheet">
  <style>${cssCode}</style>
</head>
<body>
${htmlCode}
<script>${jsCode}<\/script>
</body>
</html>`);

  // ── Update preview on change ──
  $effect(() => {
    if (!previewFrame) return;
    const doc = previewFrame.contentDocument;
    if (doc) {
      doc.open();
      doc.write(fullHtml);
      doc.close();
    }
  });

  // ── Publish to P2P network ──
  async function publish() {
    if (!title.trim()) { pubError = "Titre requis"; return; }
    publishing = true;
    published = false;
    pubError = "";
    try {
      await invoke("publish_page", {
        title: title.trim(),
        content: fullHtml,
      });
      published = true;
      setTimeout(() => published = false, 4000);
    } catch (e) {
      pubError = String(e);
    } finally {
      publishing = false;
    }
  }

  // ── Helpers ──
  function handleTab(e: KeyboardEvent) {
    if (e.key === 'Tab') {
      e.preventDefault();
      const textarea = e.target as HTMLTextAreaElement;
      const start = textarea.selectionStart;
      const end = textarea.selectionEnd;
      const val = textarea.value;
      textarea.value = val.substring(0, start) + '  ' + val.substring(end);
      textarea.selectionStart = textarea.selectionEnd = start + 2;
      textarea.dispatchEvent(new Event('input'));
    }
  }

  const tabLabels = { html: 'HTML', css: 'CSS', js: 'JS' } as const;
  const tabIcons = { html: '🏗', css: '🎨', js: '⚡' } as const;
</script>

<div class="builder">
  <!-- Top bar -->
  <header class="bar">
    <div class="bar-left">
      <input class="title-input" type="text" placeholder="Titre du site" bind:value={title} />
      <input class="domain-input mono" type="text" placeholder="monsite.torus" bind:value={domain} />
    </div>
    <div class="bar-right">
      {#if pubError}
        <span class="pub-err">{pubError}</span>
      {/if}
      {#if published}
        <span class="pub-ok">✓ Publié sur le réseau P2P</span>
      {/if}
      <button class="publish-btn" onclick={publish} disabled={publishing || !title.trim()}>
        {publishing ? '⏳ Publication…' : '🚀 Publier'}
      </button>
    </div>
  </header>

  <div class="layout">
    <!-- Code editor -->
    <div class="editor-pane">
      <div class="tabs">
        {#each (['html', 'css', 'js'] as const) as tab}
          <button
            class="tab"
            class:tab-active={activeTab === tab}
            onclick={() => activeTab = tab}
          >
            <span class="tab-icon">{tabIcons[tab]}</span>
            {tabLabels[tab]}
          </button>
        {/each}
      </div>

      <div class="code-area">
        {#if activeTab === 'html'}
          <textarea
            class="code-editor mono"
            bind:value={htmlCode}
            onkeydown={handleTab}
            spellcheck="false"
            placeholder="<!-- Votre HTML ici -->"
          ></textarea>
        {:else if activeTab === 'css'}
          <textarea
            class="code-editor mono"
            bind:value={cssCode}
            onkeydown={handleTab}
            spellcheck="false"
            placeholder="/* Votre CSS ici */"
          ></textarea>
        {:else}
          <textarea
            class="code-editor mono"
            bind:value={jsCode}
            onkeydown={handleTab}
            spellcheck="false"
            placeholder="// Votre JavaScript ici"
          ></textarea>
        {/if}
      </div>
    </div>

    <!-- Preview -->
    <div class="preview-pane">
      <div class="preview-header">
        <span class="preview-dot green"></span>
        <span class="preview-dot yellow"></span>
        <span class="preview-dot red"></span>
        <span class="preview-url mono">{domain ? `${domain}` : 'aperçu.torus'}</span>
      </div>
      <iframe
        bind:this={previewFrame}
        title="aperçu"
        class="preview-iframe"
        sandbox="allow-scripts allow-same-origin"
      ></iframe>
    </div>
  </div>
</div>

<style>
  .builder {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--color-bg-0);
  }

  /* ── Top bar ── */
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    border-bottom: 1px solid var(--color-border);
    gap: 12px;
    flex-shrink: 0;
  }
  .bar-left {
    display: flex;
    gap: 8px;
    flex: 1;
  }
  .bar-right {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .title-input, .domain-input {
    padding: 8px 12px;
    background: var(--color-bg-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-0);
    font-size: 13px;
  }
  .title-input { flex: 1; min-width: 180px; }
  .domain-input { width: 180px; font-size: 12px; }
  .title-input:focus, .domain-input:focus {
    outline: none;
    border-color: var(--color-accent, #00E5CC);
  }

  .publish-btn {
    padding: 8px 20px;
    background: var(--color-accent, #00E5CC);
    color: #000;
    border: none;
    border-radius: var(--radius-sm);
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    white-space: nowrap;
    transition: opacity 0.15s;
  }
  .publish-btn:hover:not(:disabled) { opacity: 0.85; }
  .publish-btn:disabled { opacity: 0.4; cursor: default; }

  .pub-err {
    font-size: 12px;
    color: var(--color-red, #f44);
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pub-ok {
    font-size: 12px;
    color: var(--color-accent, #00E5CC);
    white-space: nowrap;
    animation: fadeIn 0.2s ease-out;
  }

  /* ── Layout ── */
  .layout {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr 1fr;
    min-height: 0;
  }

  /* ── Editor ── */
  .editor-pane {
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--color-border);
    min-height: 0;
  }
  .tabs {
    display: flex;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }
  .tab {
    padding: 10px 20px;
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--color-text-2);
    font-family: inherit;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 6px;
    transition: color 0.15s, border-color 0.15s;
  }
  .tab:hover { color: var(--color-text-0); }
  .tab-active {
    color: var(--color-text-0);
    border-bottom-color: var(--color-accent, #00E5CC);
  }
  .tab-icon { font-size: 14px; }

  .code-area {
    flex: 1;
    min-height: 0;
  }
  .code-editor {
    width: 100%;
    height: 100%;
    padding: 16px;
    background: #0d0d0d;
    border: none;
    color: #d4d4d4;
    font-size: 13px;
    line-height: 1.6;
    resize: none;
    tab-size: 2;
    outline: none;
  }
  .code-editor::placeholder {
    color: #444;
  }

  /* ── Preview ── */
  .preview-pane {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .preview-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    background: #161616;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }
  .preview-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  .preview-dot.red { background: #ff5f57; }
  .preview-dot.yellow { background: #febc2e; }
  .preview-dot.green { background: #28c840; }
  .preview-url {
    margin-left: 8px;
    font-size: 11px;
    color: var(--color-text-2);
    background: var(--color-bg-2);
    padding: 3px 10px;
    border-radius: 4px;
    flex: 1;
  }
  .preview-iframe {
    flex: 1;
    width: 100%;
    border: none;
    background: #0a0a0a;
  }

  @keyframes fadeIn {
    from { opacity: 0; }
    to { opacity: 1; }
  }
</style>
