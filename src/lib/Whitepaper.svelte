<script lang="ts">
  // Whitepaper — a readable, bank/editorial view of WHITEPAPER_FR.md inside the app.
  //
  // The content is the SINGLE SOURCE OF TRUTH: the markdown file is imported raw
  // (Vite `?raw`) and rendered by a tiny inline parser — no external markdown
  // dependency, and the view can never drift from the file on disk.
  import source from "../../WHITEPAPER_FR.md?raw";
  import { t } from "./i18n.svelte";

  // ── Tiny, dependency-free markdown → HTML (headings, paragraphs, lists,
  //    blockquotes, fenced code, **bold**, *italic*, `code`). Faithful only:
  //    it never rewrites the substance, it only shapes it. ──

  type Section = { id: string; label: string };

  function escapeHtml(s: string): string {
    return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  }

  function inline(s: string): string {
    let out = escapeHtml(s);
    out = out.replace(/`([^`]+)`/g, (_m, c) => `<code>${c}</code>`);
    out = out.replace(/\*\*([^*]+)\*\*/g, (_m, c) => `<strong>${c}</strong>`);
    out = out.replace(/\*([^*]+)\*/g, (_m, c) => `<em>${c}</em>`);
    return out;
  }

  function stripMd(s: string): string {
    return s.replace(/[`*]/g, "").trim();
  }

  const isBlank = (s: string) => s.trim() === "";
  const headingOf = (s: string) => /^(#{1,3})\s+(.*)$/.exec(s);
  const isFence = (s: string) => s.trimStart().startsWith("```");
  const isListItem = (s: string) => /^\s*-\s+/.test(s);
  const isQuote = (s: string) => s.trimStart().startsWith(">");

  function render(md: string): { html: string; toc: Section[] } {
    const lines = md.replace(/\r\n/g, "\n").split("\n");
    const out: string[] = [];
    const toc: Section[] = [];
    let sawTitle = false;
    let i = 0;

    while (i < lines.length) {
      const line = lines[i];

      // fenced code block — verbatim, escaped, horizontally scrollable
      if (isFence(line)) {
        i++;
        const buf: string[] = [];
        while (i < lines.length && !isFence(lines[i])) {
          buf.push(lines[i]);
          i++;
        }
        i++; // closing fence
        out.push(
          `<pre class="wp-pre"><code>${escapeHtml(buf.join("\n"))}</code></pre>`,
        );
        continue;
      }

      if (isBlank(line)) {
        i++;
        continue;
      }

      // section rule → thin editorial hairline
      if (line.trim() === "---") {
        out.push(`<hr class="wp-hr" />`);
        i++;
        continue;
      }

      // heading
      const h = headingOf(line);
      if (h) {
        const level = h[1].length;
        const text = h[2];
        // drop the document's own leading H1 — the screen already has a title
        if (level === 1 && !sawTitle) {
          sawTitle = true;
          i++;
          continue;
        }
        if (level === 2) {
          const id = `wp-s${toc.length + 1}`;
          toc.push({ id, label: stripMd(text) });
          out.push(`<h2 id="${id}" class="wp-h2">${inline(text)}</h2>`);
        } else {
          out.push(`<h${level} class="wp-h${level}">${inline(text)}</h${level}>`);
        }
        i++;
        continue;
      }

      // blockquote (lead callout / honesty notes)
      if (isQuote(line)) {
        const buf: string[] = [];
        while (i < lines.length && isQuote(lines[i])) {
          buf.push(lines[i].replace(/^\s*>\s?/, ""));
          i++;
        }
        out.push(
          `<blockquote class="wp-quote">${buf.map(inline).join("<br />")}</blockquote>`,
        );
        continue;
      }

      // unordered list, with indented continuation lines folded in
      if (isListItem(line)) {
        const items: string[] = [];
        while (i < lines.length) {
          const l = lines[i];
          if (isListItem(l)) {
            items.push(l.replace(/^\s*-\s+/, ""));
            i++;
          } else if (
            !isBlank(l) &&
            !headingOf(l) &&
            l.trim() !== "---" &&
            !isFence(l) &&
            (l.startsWith(" ") || l.startsWith("\t"))
          ) {
            items[items.length - 1] += " " + l.trim();
            i++;
          } else {
            break;
          }
        }
        out.push(
          `<ul class="wp-ul">${items.map((it) => `<li>${inline(it)}</li>`).join("")}</ul>`,
        );
        continue;
      }

      // paragraph — fold wrapped lines into one
      const buf = [line];
      i++;
      while (
        i < lines.length &&
        !isBlank(lines[i]) &&
        !headingOf(lines[i]) &&
        lines[i].trim() !== "---" &&
        !isFence(lines[i]) &&
        !isQuote(lines[i]) &&
        !isListItem(lines[i])
      ) {
        buf.push(lines[i]);
        i++;
      }
      out.push(`<p class="wp-p">${inline(buf.join(" "))}</p>`);
    }

    return { html: out.join("\n"), toc };
  }

  // Parse at mount. This runs at top level (component init) — a throw here would
  // propagate through the {#key view} block and freeze navigation (dead clicks),
  // so degrade gracefully to the raw source instead of taking the app down.
  function safeRender(md: string): { html: string; toc: Section[] } {
    try {
      return render(md);
    } catch {
      return { html: `<pre class="wp-pre"><code>${escapeHtml(md)}</code></pre>`, toc: [] };
    }
  }

  const parsed = safeRender(source);
  const html = parsed.html;
  const toc = parsed.toc;
  const readMin = Math.max(1, Math.round(source.trim().split(/\s+/).length / 220));

  // Scroll-spy: highlight the section currently under the reader's eye.
  let activeId = $state("");

  $effect(() => {
    const els = toc
      .map((s) => document.getElementById(s.id))
      .filter((el): el is HTMLElement => el !== null);
    if (els.length === 0) return;
    const obs = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) activeId = e.target.id;
        }
      },
      { rootMargin: "0px 0px -72% 0px", threshold: 0 },
    );
    for (const el of els) obs.observe(el);
    return () => obs.disconnect();
  });

  function jump(id: string) {
    document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
  }

  function download() {
    const blob = new Blob([source], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "Quanta-Whitepaper.md";
    a.click();
    URL.revokeObjectURL(url);
  }
</script>

<div class="page">
  <div class="wp-top">
    <div class="wp-lede">
      <div class="section-label">{t('wp.doc')}</div>
      <h1 class="page-title">{t('wp.title')}</h1>
      <div class="page-sub">{t('wp.subtitle')}</div>
    </div>
    <div class="wp-actions">
      <span class="wp-read">{t('wp.readTime').replace('{n}', String(readMin))}</span>
      <button class="btn btn-ghost btn-sm" onclick={download} title={t('wp.downloadTitle')}>
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6">
          <path d="M8 2v8m0 0L4.8 6.8M8 10l3.2-3.2" stroke-linecap="round" stroke-linejoin="round" />
          <path d="M2.6 12.6h10.8" stroke-linecap="round" />
        </svg>
        <span>.md</span>
      </button>
    </div>
  </div>

  <div class="wp-layout">
    <!-- Content comes verbatim from WHITEPAPER_FR.md, shaped by the inline parser above. -->
    <article class="wp-body">{@html html}</article>

    <aside class="wp-toc">
      <div class="section-label">{t('wp.toc')}</div>
      <nav class="wp-toc-nav">
        {#each toc as s}
          <button
            class="wp-toc-link"
            class:active={activeId === s.id}
            onclick={() => jump(s.id)}
          >{s.label}</button>
        {/each}
      </nav>
    </aside>
  </div>
</div>

<style>
  .wp-top {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--space-6);
    margin-bottom: var(--space-8);
  }
  .wp-actions {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    flex-shrink: 0;
  }
  .wp-read {
    font-size: 12px;
    color: var(--color-text-3);
    font-variant-numeric: tabular-nums lining-nums;
    white-space: nowrap;
  }
  .btn-ghost svg { display: block; }
  .btn-ghost { display: inline-flex; align-items: center; gap: 6px; }

  .wp-layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 216px;
    gap: 56px;
    align-items: start;
  }

  /* ── Reading column — editorial typography, tabular figures ── */
  .wp-body {
    max-width: 68ch;
    font-family: var(--font-display);
    font-variant-numeric: tabular-nums lining-nums;
    color: var(--color-text-1);
    font-size: 16px;
    line-height: 1.72;
    overflow-wrap: break-word;
  }

  .wp-body :global(.wp-p) {
    margin: 0 0 var(--space-5);
  }

  .wp-body :global(.wp-h2) {
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: var(--color-text-0);
    line-height: 1.25;
    margin: var(--space-10) 0 var(--space-5);
    padding-top: var(--space-4);
    scroll-margin-top: var(--space-6);
  }
  /* the discreet teal filet at the head of each section */
  .wp-body :global(.wp-h2)::before {
    content: "";
    display: block;
    width: 30px;
    height: 2px;
    border-radius: 2px;
    background: var(--teal-500);
    margin-bottom: var(--space-4);
  }

  .wp-body :global(.wp-h3) {
    font-size: 16px;
    font-weight: 650;
    color: var(--color-text-0);
    letter-spacing: -0.01em;
    margin: var(--space-6) 0 var(--space-2);
    scroll-margin-top: var(--space-6);
  }

  .wp-body :global(strong) {
    font-weight: 660;
    color: var(--color-text-0);
  }
  .wp-body :global(em) {
    font-style: italic;
  }

  .wp-body :global(code) {
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
    font-size: 0.86em;
    background: var(--color-bg-2);
    color: var(--color-text-0);
    padding: 0.1em 0.38em;
    border-radius: 5px;
    font-variant-numeric: normal;
  }

  .wp-body :global(.wp-pre) {
    background: var(--color-bg-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius);
    padding: var(--space-4) var(--space-5);
    margin: 0 0 var(--space-6);
    overflow-x: auto;
  }
  .wp-body :global(.wp-pre code) {
    background: none;
    padding: 0;
    border-radius: 0;
    color: var(--color-text-1);
    font-size: 12.5px;
    line-height: 1.6;
    white-space: pre;
    font-variant-numeric: tabular-nums lining-nums;
  }

  .wp-body :global(.wp-quote) {
    margin: 0 0 var(--space-6);
    padding: 2px 0 2px var(--space-5);
    border-left: 2px solid var(--teal-300);
    color: var(--color-text-2);
    font-size: 14px;
    line-height: 1.65;
  }
  .wp-body :global(.wp-quote strong) {
    color: var(--color-text-1);
  }

  .wp-body :global(.wp-ul) {
    margin: 0 0 var(--space-5);
    padding: 0;
    list-style: none;
  }
  .wp-body :global(.wp-ul li) {
    position: relative;
    padding-left: var(--space-5);
    margin-bottom: var(--space-3);
    line-height: 1.62;
  }
  .wp-body :global(.wp-ul li)::before {
    content: "";
    position: absolute;
    left: 3px;
    top: 0.68em;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--teal-400);
  }

  .wp-body :global(.wp-hr) {
    border: 0;
    border-top: 1px solid var(--color-border);
    margin: var(--space-8) 0 0;
    opacity: 0.7;
  }
  /* a rule immediately followed by a section heading would double-space —
     tuck the heading up under it */
  .wp-body :global(.wp-hr + .wp-h2) {
    margin-top: var(--space-8);
  }

  /* ── Table of contents ── */
  .wp-toc {
    position: sticky;
    top: var(--space-6);
  }
  .wp-toc-nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    border-left: 1px solid var(--color-border);
  }
  .wp-toc-link {
    text-align: left;
    background: none;
    border: none;
    border-left: 2px solid transparent;
    margin-left: -1px;
    padding: 5px 0 5px var(--space-4);
    font-family: var(--font-display);
    font-size: 12.5px;
    line-height: 1.4;
    color: var(--color-text-3);
    cursor: pointer;
    transition: color 0.15s ease, border-color 0.15s ease;
  }
  .wp-toc-link:hover {
    color: var(--color-text-1);
  }
  .wp-toc-link.active {
    color: var(--color-accent);
    border-left-color: var(--color-accent);
    font-weight: 600;
  }

  @media (max-width: 900px) {
    .wp-layout {
      grid-template-columns: 1fr;
      gap: 0;
    }
    .wp-toc {
      display: none;
    }
  }
  @media (max-width: 560px) {
    .wp-top {
      flex-direction: column;
      align-items: flex-start;
      gap: var(--space-4);
    }
  }
</style>
