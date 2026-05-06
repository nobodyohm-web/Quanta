<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  type Sub = {
    pk: string;
    follower_count: number;
    weighted_likes_received: number;
    tip_total_received_qta: number;
    boost_bps: number;
  };
  type Doc = {
    cid: string;
    title: string;
    snippet: string;
    author_pk: string;
    torus_domain?: string | null;
    kind: string;
    lang: string;
    updated_at: number;
  };

  let subs = $state<Sub[]>([]);
  let feed = $state<Doc[]>([]);
  let loading = $state(true);
  let msg = $state("");

  async function refresh() {
    loading = true;
    try {
      subs = await invoke<Sub[]>("list_my_subscriptions");
      feed = await invoke<Doc[]>("subscriptions_feed", { limit: 40 });
    } catch (e) {
      msg = `Erreur : ${e}`;
    } finally {
      loading = false;
    }
  }

  async function unfollow(pk: string) {
    msg = "Désabonnement…";
    try {
      await invoke<void>("social_follow", {
        followeePk: pk,
        tier: "signal",
        active: false,
      });
      msg = "✓ Désabonné";
      await refresh();
    } catch (e) {
      msg = `Erreur : ${e}`;
    }
  }

  $effect(() => { refresh(); });

  function shortPk(pk: string): string {
    return pk.slice(0, 10) + "…" + pk.slice(-4);
  }
  function fmtTs(ts: number): string {
    const d = new Date(ts * 1000);
    return d.toLocaleDateString("fr") + " " + d.toLocaleTimeString("fr", { hour: "2-digit", minute: "2-digit" });
  }
</script>

<div class="subs">
  <header class="head">
    <h1>Abonnements</h1>
    <button class="ghost" onclick={refresh}>↻ Rafraîchir</button>
  </header>

  {#if msg}<div class="msg">{msg}</div>{/if}

  {#if loading}
    <div class="empty">Chargement…</div>
  {:else if subs.length === 0}
    <div class="empty">
      <p>Vous ne suivez personne pour le moment.</p>
      <p class="hint">Visitez un site dans le Browser et cliquez "+ Suivre" pour ajouter un créateur ici.</p>
    </div>
  {:else}
    <section class="creators">
      <h2>Créateurs suivis ({subs.length})</h2>
      <div class="creator-list">
        {#each subs as s (s.pk)}
          <article class="creator">
            <div class="c-meta">
              <span class="c-pk">{shortPk(s.pk)}</span>
              <span class="c-stat">♥ {s.weighted_likes_received.toFixed(1)} pondérés</span>
              <span class="c-stat">{s.follower_count} suivis</span>
              <span class="c-stat">$ {s.tip_total_received_qta.toFixed(2)} QTA reçus</span>
            </div>
            <button class="ghost small" onclick={() => unfollow(s.pk)}>Se désabonner</button>
          </article>
        {/each}
      </div>
    </section>

    <section class="feed">
      <h2>Activité récente ({feed.length})</h2>
      {#if feed.length === 0}
        <div class="empty small">Aucune publication récente des créateurs suivis.</div>
      {/if}
      {#each feed as d (d.cid)}
        <article class="post">
          <div class="post-meta">
            <span class="dom">{d.torus_domain ?? shortPk(d.author_pk)}</span>
            <span class="kind">{d.kind.toLowerCase()}</span>
            <span class="ts">{fmtTs(d.updated_at)}</span>
          </div>
          <div class="post-title">{d.title}</div>
          <div class="post-snippet">{d.snippet}</div>
        </article>
      {/each}
    </section>
  {/if}
</div>

<style>
  .subs { padding: 24px; max-width: 800px; margin: 0 auto; height: 100%; overflow-y: auto; }
  .head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 24px; }
  .head h1 { margin: 0; font-size: 28px; font-weight: 700; letter-spacing: -0.02em; }
  .msg {
    padding: 8px 12px; margin-bottom: 16px;
    background: var(--color-bg-2); border-radius: var(--radius-sm);
    font-size: 13px; color: var(--color-accent);
  }
  .empty { padding: 32px 0; color: var(--color-text-2); text-align: center; }
  .empty.small { padding: 16px; font-size: 12px; }
  .empty .hint { font-size: 12px; color: var(--color-text-2); margin-top: 8px; }

  .ghost {
    padding: 6px 12px; background: transparent; color: var(--color-text-1);
    border: 1px solid var(--color-border); border-radius: var(--radius-sm);
    cursor: pointer; font-size: 12px;
  }
  .ghost.small { padding: 4px 8px; font-size: 11px; }

  section h2 {
    margin: 24px 0 12px; font-size: 15px; font-weight: 600;
    color: var(--color-text-1);
    text-transform: uppercase; letter-spacing: 0.06em;
  }

  .creator-list { display: flex; flex-direction: column; gap: 4px; }
  .creator {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    padding: 10px 14px;
    background: var(--color-bg-1); border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
  }
  .c-meta { display: flex; gap: 12px; align-items: baseline; flex-wrap: wrap; flex: 1; }
  .c-pk { font-family: var(--font-mono); color: var(--color-accent); font-size: 12px; }
  .c-stat { font-size: 11px; color: var(--color-text-2); font-family: var(--font-mono); }

  .post {
    padding: 12px 16px; margin-bottom: 6px;
    background: var(--color-bg-1); border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
  }
  .post-meta {
    display: flex; gap: 12px; align-items: center;
    font-size: 11px; color: var(--color-text-2);
    margin-bottom: 4px;
  }
  .dom { font-family: var(--font-mono); color: var(--color-accent); }
  .kind { text-transform: uppercase; letter-spacing: 0.06em; }
  .ts { margin-left: auto; }
  .post-title { font-weight: 600; font-size: 15px; margin: 4px 0; color: var(--color-text-0); }
  .post-snippet { color: var(--color-text-1); font-size: 13px; line-height: 1.5; }
</style>
