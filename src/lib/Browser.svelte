<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  // ─── Types miroir backend ────────────────────────────────────────────
  type DocKind = "Site" | "Blog" | "Forum" | "Comment" | "Shop";
  type SearchHit = {
    cid: string;
    title: string;
    snippet: string;
    author_pk: string;
    torus_domain?: string | null;
    kind: DocKind;
    lang: string;
    updated_at: number;
    score: number;
  };
  type DomainResolution = {
    name: string;
    target_pk: string | null;
    state: "current" | "grace" | "expired" | "unknown";
    due_qta: number;
  };
  type SitePageView = { path: string; title: string; html: string };
  type SiteAssetView = {
    path: string;
    mime: string;
    content_b64: string;
    dag_cid: string | null;
    size: number;
  };
  type PageStats = {
    weighted_likes: number;
    weighted_dislikes: number;
    like_count: number;
    dislike_count: number;
    tip_total_micro_qta: number;
    boost_until_ts: number;
  };

  // ─── État local ──────────────────────────────────────────────────────
  let url = $state("");
  let query = $state("");
  let mode = $state<"home" | "results" | "page">("home");
  let loading = $state(false);
  let error = $state("");
  let hits = $state<SearchHit[]>([]);
  let pageTitle = $state("");
  let pageHtml = $state("");
  let allowScripts = $state(false);

  /// Page actuelle pour les actions sociales (like/follow/report).
  let currentTargetCid = $state<string | null>(null);
  let currentTargetAuthor = $state<string | null>(null);
  let currentTargetDomain = $state<string | null>(null);
  let currentStats = $state<PageStats | null>(null);

  // ─── Historique simple (mémoire de session) ──────────────────────────
  let history = $state<{ url: string; title: string }[]>([]);
  let cursor = $state(-1);
  const canBack = $derived(cursor > 0);
  const canForward = $derived(cursor < history.length - 1 && cursor >= 0);

  function pushHistory(u: string, title: string) {
    history = [...history.slice(0, cursor + 1), { url: u, title }];
    cursor = history.length - 1;
  }

  // ─── Détection URL vs recherche ──────────────────────────────────────
  function looksLikeTorusUrl(s: string): boolean {
    return s.startsWith("torus://") || s.endsWith(".torus") || /^[a-f0-9]{64}$/.test(s);
  }

  async function go() {
    error = "";
    const raw = url.trim();
    if (!raw) return;
    if (looksLikeTorusUrl(raw)) {
      await openByUrl(raw);
    } else {
      query = raw;
      await search();
    }
  }

  async function search() {
    error = "";
    loading = true;
    try {
      hits = await invoke<SearchHit[]>("search_pages", {
        query,
        lang: null,
        kind: null,
        sinceTs: null,
        creatorPk: null,
        limit: 20,
      });
      mode = "results";
      pushHistory(`?q=${encodeURIComponent(query)}`, `Recherche: ${query}`);
    } catch (e) {
      error = (e as Error)?.toString() || "Erreur de recherche";
    } finally {
      loading = false;
    }
  }

  /// Découpe `torus://name.torus/path` en (host, path).
  function parseUrl(raw: string): { host: string; path: string } {
    const stripped = raw.replace(/^torus:\/\//, "");
    const slash = stripped.indexOf("/");
    if (slash === -1) return { host: stripped, path: "/" };
    return { host: stripped.slice(0, slash), path: stripped.slice(slash) || "/" };
  }

  /// Tente de résoudre `host` (.torus) vers une pubkey ; sinon considère que
  /// c'est déjà une pubkey hex 64.
  async function resolveHost(host: string): Promise<string> {
    if (/^[a-f0-9]{64}$/.test(host)) return host;
    const r = await invoke<DomainResolution>("resolve_domain", { name: host });
    if (!r.target_pk) {
      throw new Error(`${host} : ${r.state}`);
    }
    return r.target_pk;
  }

  async function openByUrl(raw: string) {
    error = "";
    loading = true;
    try {
      const { host, path } = parseUrl(raw);
      const author_pk = await resolveHost(host);

      // 1. Tente le mode multi-page (V3.3 SiteManifest).
      const sitePage = await invoke<SitePageView | null>("get_site_page", {
        authorPk: author_pk,
        path,
      });

      if (sitePage) {
        pageTitle = sitePage.title;
        pageHtml = await renderWithAssets(author_pk, sitePage.html);
      } else {
        // 2. Fallback : single-page legacy (V2 PublishedPage).
        const legacy = await invoke<{ title: string; content: string } | null>(
          "get_page",
          { walletPk: author_pk },
        );
        if (legacy) {
          pageTitle = legacy.title;
          pageHtml = legacy.content;
        } else {
          throw new Error(`Aucun contenu publié pour ${host}`);
        }
      }

      // CID local pour les actions sociales : hash stable du couple (author, path).
      currentTargetCid = await cidOf(author_pk, path);
      currentTargetAuthor = author_pk;
      currentTargetDomain = /^[a-f0-9]{64}$/.test(host) ? null : host;

      // Charge les stats sociales actuelles (best-effort).
      try {
        currentStats = await invoke<PageStats | null>("get_page_social_stats", {
          cid: currentTargetCid,
        });
      } catch {
        currentStats = null;
      }

      mode = "page";
      pushHistory(raw, pageTitle);
      url = `torus://${host}${path === "/" ? "" : path}`;
    } catch (e) {
      error = (e as Error)?.toString() || "Site introuvable";
    } finally {
      loading = false;
    }
  }

  /// Inject les assets inline (CSS, images) dans le HTML reçu.
  /// Remplace les `src="/img/x.png"` et `href="/style.css"` par data: URIs.
  async function renderWithAssets(authorPk: string, html: string): Promise<string> {
    // On scanne uniquement les attributs commençant par `/` (paths internes).
    const re = /\b(src|href)=["'](\/[^"']+)["']/g;
    const matches = [...html.matchAll(re)];
    if (matches.length === 0) return html;

    const cache = new Map<string, string>();
    for (const m of matches) {
      const path = m[2];
      if (cache.has(path)) continue;
      try {
        const a = await invoke<SiteAssetView | null>("get_site_asset", {
          authorPk,
          path,
        });
        if (a && a.content_b64) {
          cache.set(path, `data:${a.mime};base64,${a.content_b64}`);
        }
      } catch {
        /* asset manquant : on garde la URL originale (probablement HTTP externe — bloqué par CSP) */
      }
    }
    let out = html;
    for (const [path, data] of cache.entries()) {
      const escaped = path.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      out = out.replace(
        new RegExp(`\\b(src|href)=["']${escaped}["']`, "g"),
        `$1="${data}"`,
      );
    }
    return out;
  }

  /// CID stable pour un couple (auteur, path) — utilisé comme clé sociale.
  async function cidOf(authorPk: string, path: string): Promise<string> {
    // Hash JS via SubtleCrypto ; suffisamment stable pour servir d'identifiant local.
    const enc = new TextEncoder().encode(`${authorPk}|${path}`);
    const h = await crypto.subtle.digest("SHA-256", enc);
    return [...new Uint8Array(h)]
      .map((b) => b.toString(16).padStart(2, "0"))
      .join("");
  }

  async function openHit(hit: SearchHit) {
    const route = hit.torus_domain
      ? `torus://${hit.torus_domain}`
      : `torus://${hit.author_pk}`;
    url = route;
    await openByUrl(route);
  }

  function back() {
    if (!canBack) return;
    cursor -= 1;
    const h = history[cursor];
    url = h.url;
  }
  function forward() {
    if (!canForward) return;
    cursor += 1;
    const h = history[cursor];
    url = h.url;
  }
  function home() {
    mode = "home";
    url = "";
    query = "";
    hits = [];
  }

  // ─── Actions sociales ────────────────────────────────────────────────
  let actionMsg = $state("");

  async function likePage() {
    if (!currentTargetCid || !currentTargetAuthor) return;
    actionMsg = "Like en cours…";
    try {
      await invoke<void>("social_vote", {
        targetCid: currentTargetCid,
        targetAuthorPk: currentTargetAuthor,
        amountQta: 0.1,
        weight: 1,
      });
      actionMsg = "♥ Like envoyé (0.1 QTA)";
      // Refresh stats
      currentStats = await invoke<PageStats | null>("get_page_social_stats", {
        cid: currentTargetCid,
      });
    } catch (e) {
      actionMsg = `Erreur : ${e}`;
    }
  }

  async function tipCreator(amountQta: number) {
    if (!currentTargetCid || !currentTargetAuthor) return;
    actionMsg = `Tip ${amountQta} QTA…`;
    try {
      await invoke<void>("social_tip", {
        targetCid: currentTargetCid,
        targetAuthorPk: currentTargetAuthor,
        amountQta,
        memo: "",
      });
      actionMsg = `✓ Tip ${amountQta} QTA envoyé`;
    } catch (e) {
      actionMsg = `Erreur : ${e}`;
    }
  }

  async function followCreator() {
    if (!currentTargetAuthor) return;
    actionMsg = "Abonnement…";
    try {
      await invoke<void>("social_follow", {
        followeePk: currentTargetAuthor,
        tier: "signal",
        active: true,
      });
      actionMsg = "✓ Abonné (Signal — gratuit)";
    } catch (e) {
      actionMsg = `Erreur : ${e}`;
    }
  }

  async function reportPage() {
    if (!currentTargetCid || !currentTargetAuthor) return;
    actionMsg = "Signalement…";
    try {
      await invoke<string | null>("submit_moderation_report", {
        targetCid: currentTargetCid,
        targetAuthorPk: currentTargetAuthor,
        category: "spam",
        evidenceCid: null,
      });
      actionMsg = "✓ Signalement envoyé (0.1 QTA brûlé)";
    } catch (e) {
      actionMsg = `Erreur : ${e}`;
    }
  }

  // Bind enter key
  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter") go();
  }
</script>

<div class="browser">
  <header class="bar">
    <div class="nav-btns">
      <button class="navbtn" disabled={!canBack} onclick={back} aria-label="Précédent">‹</button>
      <button class="navbtn" disabled={!canForward} onclick={forward} aria-label="Suivant">›</button>
      <button class="navbtn" onclick={home} aria-label="Accueil">⌂</button>
    </div>
    <div class="urlbox">
      <input
        type="text"
        class="urlinput"
        bind:value={url}
        onkeydown={onKey}
        placeholder="torus://nom · ou tapez votre recherche"
        spellcheck="false"
        autocapitalize="off"
        autocomplete="off"
      />
      <button class="gobtn" onclick={go}>Aller</button>
    </div>
  </header>

  {#if loading}
    <div class="msg">Chargement…</div>
  {:else if error}
    <div class="msg err">{error}</div>
  {:else if mode === "home"}
    <section class="home">
      <div class="hero">
        <h1>Le Web P2P</h1>
        <p>Recherchez · publiez · récompensez.</p>
        <input
          class="bigsearch"
          type="text"
          bind:value={url}
          onkeydown={onKey}
          placeholder="Cherchez « cuisine vegan » ou « torus://alex »"
        />
      </div>
      <div class="quick">
        <button onclick={() => { url = "torus://alex.torus"; go(); }}>alex.torus</button>
        <button onclick={() => { url = "open source"; go(); }}>open source</button>
        <button onclick={() => { url = "musique électronique"; go(); }}>musique</button>
      </div>
    </section>
  {:else if mode === "results"}
    <section class="results">
      <div class="meta">
        {hits.length} résultat{hits.length > 1 ? "s" : ""} pour « {query} »
      </div>
      {#if hits.length === 0}
        <div class="empty">
          Rien trouvé. Essayez d'autres mots-clés ou
          <button class="link" onclick={home}>publiez le premier site</button>.
        </div>
      {/if}
      {#each hits as hit (hit.cid)}
        <button class="hit" onclick={() => openHit(hit)}>
          <div class="hit-top">
            <span class="hit-domain">{hit.torus_domain ?? hit.author_pk.slice(0, 12) + "…"}</span>
            <span class="hit-kind">{hit.kind.toLowerCase()}</span>
            <span class="hit-score">★ {hit.score.toFixed(1)}</span>
          </div>
          <div class="hit-title">{hit.title}</div>
          <div class="hit-snippet">{hit.snippet}</div>
        </button>
      {/each}
    </section>
  {:else if mode === "page"}
    <section class="page">
      <div class="page-actions">
        <span class="page-title">{pageTitle}</span>
        {#if currentStats}
          <span class="stats">
            ♥ {currentStats.like_count}
            · {currentStats.weighted_likes.toFixed(1)} pondérés
          </span>
        {/if}
        <span class="spacer"></span>
        <label class="js-toggle"><input type="checkbox" bind:checked={allowScripts} /> JS</label>
        <button class="actbtn" onclick={likePage}>♥ Like (0.1 QTA)</button>
        <button class="actbtn" onclick={() => tipCreator(1)}>$ Tip 1 QTA</button>
        <button class="actbtn" onclick={followCreator}>+ Suivre</button>
        <button class="actbtn warn" onclick={reportPage}>⚠ Signaler</button>
      </div>
      {#if actionMsg}
        <div class="action-msg">{actionMsg}</div>
      {/if}
      <iframe
        title={pageTitle}
        srcdoc={pageHtml}
        sandbox={allowScripts ? "allow-scripts" : ""}
      ></iframe>
    </section>
  {/if}
</div>

<style>
  .browser {
    display: flex; flex-direction: column; height: 100%;
    background: var(--color-bg-0);
  }
  .bar {
    display: flex; align-items: center; gap: 8px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--color-border);
  }
  .nav-btns { display: flex; gap: 4px; }
  .navbtn {
    width: 32px; height: 32px;
    border-radius: var(--radius-sm);
    background: var(--color-bg-1);
    border: 1px solid var(--color-border);
    color: var(--color-text-1);
    font-size: 16px;
    cursor: pointer;
  }
  .navbtn:disabled { opacity: 0.3; cursor: default; }
  .urlbox {
    flex: 1; display: flex; gap: 8px;
  }
  .urlinput {
    flex: 1;
    padding: 8px 12px;
    background: var(--color-bg-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-0);
    font-family: var(--font-mono);
    font-size: 13px;
  }
  .urlinput:focus { outline: 1px solid var(--color-accent); }
  .gobtn {
    padding: 8px 16px;
    background: var(--color-accent);
    color: #000;
    border: none;
    border-radius: var(--radius-sm);
    font-weight: 600;
    cursor: pointer;
  }

  .msg { padding: 32px; color: var(--color-text-1); }
  .msg.err { color: var(--color-red); }

  .home {
    flex: 1;
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 32px;
    padding: 48px 24px;
  }
  .hero { text-align: center; max-width: 560px; width: 100%; }
  .hero h1 {
    font-size: 48px; font-weight: 700;
    letter-spacing: -0.04em;
    margin: 0 0 8px;
  }
  .hero p {
    color: var(--color-text-1);
    margin: 0 0 24px;
  }
  .bigsearch {
    width: 100%;
    padding: 14px 20px;
    background: var(--color-bg-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    color: var(--color-text-0);
    font-size: 16px;
  }
  .bigsearch:focus { outline: 1px solid var(--color-accent); }
  .quick { display: flex; gap: 8px; flex-wrap: wrap; justify-content: center; }
  .quick button {
    padding: 6px 12px;
    background: var(--color-bg-1);
    border: 1px solid var(--color-border);
    border-radius: 999px;
    color: var(--color-text-1);
    font-size: 12px;
    cursor: pointer;
  }
  .quick button:hover { color: var(--color-text-0); }

  .results { padding: 16px 24px; overflow: auto; }
  .meta {
    color: var(--color-text-2);
    font-size: 12px;
    margin-bottom: 16px;
  }
  .empty {
    color: var(--color-text-1);
    padding: 32px 0;
    font-size: 14px;
  }
  .link {
    background: none;
    border: none;
    color: var(--color-accent);
    cursor: pointer;
    font: inherit;
    padding: 0;
  }
  .hit {
    display: block;
    width: 100%;
    text-align: left;
    padding: 16px;
    margin-bottom: 8px;
    background: var(--color-bg-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-0);
    cursor: pointer;
  }
  .hit:hover { border-color: var(--color-accent); }
  .hit-top {
    display: flex; gap: 12px; align-items: center;
    font-size: 11px; color: var(--color-text-2);
    margin-bottom: 4px;
  }
  .hit-domain { font-family: var(--font-mono); color: var(--color-accent); }
  .hit-kind { text-transform: uppercase; letter-spacing: 0.06em; }
  .hit-score { margin-left: auto; }
  .hit-title { font-weight: 600; font-size: 16px; margin: 4px 0; }
  .hit-snippet { color: var(--color-text-1); font-size: 13px; line-height: 1.5; }

  .page { display: flex; flex-direction: column; flex: 1; min-height: 0; }
  .page-actions {
    display: flex; gap: 8px; align-items: center;
    padding: 8px 16px;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-1);
  }
  .page-title {
    font-weight: 600;
    color: var(--color-text-0);
  }
  .stats {
    font-size: 11px;
    color: var(--color-text-2);
    font-family: var(--font-mono);
  }
  .spacer { flex: 1; }
  .js-toggle {
    font-size: 11px; color: var(--color-text-2);
    display: flex; align-items: center; gap: 4px;
  }
  .actbtn {
    padding: 6px 12px;
    background: var(--color-bg-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-1);
    font-size: 12px;
    cursor: pointer;
  }
  .actbtn:hover { color: var(--color-text-0); }
  .actbtn.warn:hover { color: var(--color-red); border-color: var(--color-red); }
  .action-msg {
    padding: 4px 16px;
    background: var(--color-bg-2);
    color: var(--color-accent);
    font-size: 11px;
    border-bottom: 1px solid var(--color-border);
  }
  iframe { flex: 1; border: 0; background: var(--color-bg-0); width: 100%; }
</style>
