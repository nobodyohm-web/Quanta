<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  type Forum = { id: string; name: string; description: string; creator_pk: string; created_at: number };
  type Thread = { id: string; forum_id: string; title: string; body_cid: string; author_pk: string; created_at: number };
  type Comment = { id: string; thread_id: string; parent_id: string | null; body_cid: string; author_pk: string; created_at: number };

  type View = "list" | "thread" | "newForum";

  let view = $state<View>("list");
  let forums = $state<Forum[]>([]);
  let threadsByForum = $state<Record<string, Thread[]>>({});
  let openForumId = $state<string | null>(null);

  let activeThread = $state<Thread | null>(null);
  let activeComments = $state<Comment[]>([]);
  let newCommentBody = $state("");

  let newForumName = $state("");
  let newForumDesc = $state("");
  let newThreadTitle = $state("");
  let newThreadBody = $state("");
  let activeForumForNewThread = $state<string | null>(null);

  let loading = $state(false);
  let msg = $state("");

  async function loadForums() {
    loading = true;
    try {
      forums = await invoke<Forum[]>("list_forums");
    } catch (e) {
      msg = `Erreur : ${e}`;
    } finally {
      loading = false;
    }
  }

  async function loadThreads(forumId: string) {
    try {
      threadsByForum[forumId] = await invoke<Thread[]>("list_threads", { forumId });
    } catch (e) {
      msg = `Erreur threads : ${e}`;
    }
  }

  async function toggleForum(id: string) {
    if (openForumId === id) {
      openForumId = null;
      return;
    }
    openForumId = id;
    if (!threadsByForum[id]) await loadThreads(id);
  }

  async function openThread(t: Thread) {
    activeThread = t;
    view = "thread";
    try {
      activeComments = await invoke<Comment[]>("list_comments", { threadId: t.id });
    } catch (e) {
      msg = `Erreur commentaires : ${e}`;
    }
  }

  async function createForum() {
    if (!newForumName.trim()) return;
    msg = "Création…";
    try {
      await invoke<void>("forum_create", {
        name: newForumName.trim(),
        description: newForumDesc.trim(),
      });
      msg = `✓ Forum "${newForumName}" créé`;
      newForumName = "";
      newForumDesc = "";
      view = "list";
      await loadForums();
    } catch (e) {
      msg = `Erreur : ${e}`;
    }
  }

  async function createThread() {
    if (!activeForumForNewThread || !newThreadTitle.trim()) return;
    msg = "Création thread…";
    try {
      await invoke<string>("thread_create", {
        forumId: activeForumForNewThread,
        title: newThreadTitle.trim(),
        body: newThreadBody.trim(),
        forkedFrom: null,
      });
      msg = "✓ Thread créé";
      newThreadTitle = "";
      newThreadBody = "";
      await loadThreads(activeForumForNewThread);
      activeForumForNewThread = null;
    } catch (e) {
      msg = `Erreur : ${e}`;
    }
  }

  async function postComment() {
    if (!activeThread || !newCommentBody.trim()) return;
    msg = "Envoi…";
    try {
      await invoke<string>("comment_create", {
        threadId: activeThread.id,
        body: newCommentBody.trim(),
        parentCommentId: null,
      });
      msg = "✓ Commentaire posté";
      newCommentBody = "";
      activeComments = await invoke<Comment[]>("list_comments", { threadId: activeThread.id });
    } catch (e) {
      msg = `Erreur : ${e}`;
    }
  }

  $effect(() => { loadForums(); });

  function shortPk(pk: string): string {
    return pk.slice(0, 8) + "…" + pk.slice(-4);
  }
  function formatTs(ts: number): string {
    const d = new Date(ts * 1000);
    return d.toLocaleDateString("fr") + " " + d.toLocaleTimeString("fr", { hour: "2-digit", minute: "2-digit" });
  }
</script>

<div class="forums">
  <header class="head">
    <h1>Forums</h1>
    {#if view === "list"}
      <button class="primary" onclick={() => view = "newForum"}>+ Créer un forum</button>
    {:else}
      <button class="ghost" onclick={() => { view = "list"; activeThread = null; }}>‹ Retour</button>
    {/if}
  </header>

  {#if msg}<div class="msg">{msg}</div>{/if}

  {#if view === "newForum"}
    <section class="form">
      <h2>Nouveau forum</h2>
      <label>Nom <input bind:value={newForumName} placeholder="ex: cuisine-vegan" maxlength="50" /></label>
      <label>Description
        <textarea bind:value={newForumDesc} placeholder="De quoi va parler ce forum ?" rows="3" maxlength="500"></textarea>
      </label>
      <div class="row">
        <button class="primary" onclick={createForum} disabled={!newForumName.trim()}>Créer</button>
        <button class="ghost" onclick={() => view = "list"}>Annuler</button>
      </div>
    </section>

  {:else if view === "thread" && activeThread}
    <section class="thread">
      <div class="thread-head">
        <h2>{activeThread.title}</h2>
        <div class="meta">par {shortPk(activeThread.author_pk)} · {formatTs(activeThread.created_at)}</div>
      </div>
      <div class="comments">
        {#if activeComments.length === 0}
          <div class="empty">Aucun commentaire pour le moment.</div>
        {/if}
        {#each activeComments as c (c.id)}
          <article class="comment">
            <div class="meta">{shortPk(c.author_pk)} · {formatTs(c.created_at)}</div>
            <div class="body">{c.body_cid}</div>
          </article>
        {/each}
      </div>
      <div class="reply">
        <textarea bind:value={newCommentBody} placeholder="Écrire un commentaire…" rows="3" maxlength="2000"></textarea>
        <button class="primary" onclick={postComment} disabled={!newCommentBody.trim()}>Publier</button>
      </div>
    </section>

  {:else}
    <section class="list">
      {#if loading}
        <div class="empty">Chargement…</div>
      {:else if forums.length === 0}
        <div class="empty">
          Aucun forum encore. <button class="link" onclick={() => view = "newForum"}>Créez le premier</button>.
        </div>
      {/if}
      {#each forums as f (f.id)}
        <article class="forum">
          <button class="forum-head" onclick={() => toggleForum(f.id)}>
            <div>
              <div class="forum-name">{f.name}</div>
              <div class="forum-desc">{f.description}</div>
            </div>
            <div class="forum-creator">{shortPk(f.creator_pk)}</div>
          </button>
          {#if openForumId === f.id}
            <div class="threads">
              {#if (threadsByForum[f.id] ?? []).length === 0}
                <div class="empty small">Aucun thread.</div>
              {/if}
              {#each (threadsByForum[f.id] ?? []) as t (t.id)}
                <button class="thread-row" onclick={() => openThread(t)}>
                  <div class="t-title">{t.title}</div>
                  <div class="t-meta">{shortPk(t.author_pk)} · {formatTs(t.created_at)}</div>
                </button>
              {/each}
              {#if activeForumForNewThread === f.id}
                <div class="new-thread">
                  <input bind:value={newThreadTitle} placeholder="Titre du thread" maxlength="200" />
                  <textarea bind:value={newThreadBody} placeholder="Premier message" rows="3" maxlength="2000"></textarea>
                  <div class="row">
                    <button class="primary" onclick={createThread} disabled={!newThreadTitle.trim()}>Publier</button>
                    <button class="ghost" onclick={() => activeForumForNewThread = null}>Annuler</button>
                  </div>
                </div>
              {:else}
                <button class="ghost small" onclick={() => activeForumForNewThread = f.id}>+ Nouveau thread</button>
              {/if}
            </div>
          {/if}
        </article>
      {/each}
    </section>
  {/if}
</div>

<style>
  .forums { padding: 24px; max-width: 800px; margin: 0 auto; height: 100%; overflow-y: auto; }
  .head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 24px; }
  .head h1 { margin: 0; font-size: 28px; font-weight: 700; letter-spacing: -0.02em; }
  .msg {
    padding: 8px 12px; margin-bottom: 16px;
    background: var(--color-bg-2); border-radius: var(--radius-sm);
    font-size: 13px; color: var(--color-accent);
  }
  .empty { padding: 32px 0; color: var(--color-text-2); text-align: center; }
  .empty.small { padding: 16px; font-size: 12px; }
  .link { background: none; border: none; color: var(--color-accent); cursor: pointer; padding: 0; font: inherit; }

  .form { display: flex; flex-direction: column; gap: 12px; }
  .form h2 { margin: 0 0 8px; font-size: 18px; }
  .form label { display: flex; flex-direction: column; gap: 4px; font-size: 11px; color: var(--color-text-2); text-transform: uppercase; letter-spacing: 0.06em; }
  .form input, .form textarea {
    padding: 8px 12px; background: var(--color-bg-1); border: 1px solid var(--color-border);
    border-radius: var(--radius-sm); color: var(--color-text-0); font-family: var(--font-sans);
    font-size: 14px;
  }
  .form input:focus, .form textarea:focus { outline: 1px solid var(--color-accent); }
  .row { display: flex; gap: 8px; }

  .primary, .ghost {
    padding: 8px 16px; border-radius: var(--radius-sm); cursor: pointer; font-size: 13px; font-weight: 600;
  }
  .primary { background: var(--color-accent); color: #000; border: none; }
  .primary:disabled { opacity: 0.4; cursor: default; }
  .ghost { background: transparent; color: var(--color-text-1); border: 1px solid var(--color-border); }
  .ghost.small { padding: 4px 12px; font-size: 11px; font-weight: 400; }

  .forum {
    background: var(--color-bg-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    margin-bottom: 8px;
    overflow: hidden;
  }
  .forum-head {
    width: 100%; display: flex; align-items: center; justify-content: space-between; gap: 16px;
    padding: 14px 16px;
    background: transparent; border: none; color: var(--color-text-0);
    text-align: left; cursor: pointer;
  }
  .forum-head:hover { background: var(--color-bg-2); }
  .forum-name { font-weight: 600; font-size: 15px; }
  .forum-desc { font-size: 12px; color: var(--color-text-2); margin-top: 2px; }
  .forum-creator { font-family: var(--font-mono); font-size: 11px; color: var(--color-text-2); }
  .threads { padding: 0 16px 16px; display: flex; flex-direction: column; gap: 4px; }
  .thread-row {
    width: 100%; padding: 10px 12px;
    background: var(--color-bg-2); border: 1px solid var(--color-border);
    border-radius: var(--radius-sm); color: var(--color-text-0);
    text-align: left; cursor: pointer;
  }
  .thread-row:hover { border-color: var(--color-accent); }
  .t-title { font-weight: 500; font-size: 14px; }
  .t-meta { font-size: 11px; color: var(--color-text-2); margin-top: 2px; }
  .new-thread {
    display: flex; flex-direction: column; gap: 8px;
    padding: 12px;
    background: var(--color-bg-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
  }
  .new-thread input, .new-thread textarea {
    padding: 8px 12px; background: var(--color-bg-0); border: 1px solid var(--color-border);
    border-radius: var(--radius-sm); color: var(--color-text-0); font-family: var(--font-sans);
    font-size: 13px;
  }

  .thread {
    display: flex; flex-direction: column; gap: 16px;
  }
  .thread-head h2 { margin: 0 0 4px; font-size: 22px; }
  .thread-head .meta { color: var(--color-text-2); font-size: 12px; }
  .comments { display: flex; flex-direction: column; gap: 8px; }
  .comment {
    padding: 12px 16px;
    background: var(--color-bg-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
  }
  .comment .meta { font-size: 11px; color: var(--color-text-2); margin-bottom: 6px; }
  .comment .body { color: var(--color-text-0); font-size: 14px; line-height: 1.5; word-break: break-word; }
  .reply { display: flex; flex-direction: column; gap: 8px; }
  .reply textarea {
    padding: 8px 12px; background: var(--color-bg-1); border: 1px solid var(--color-border);
    border-radius: var(--radius-sm); color: var(--color-text-0); font-family: var(--font-sans);
    font-size: 14px;
  }
  .reply .primary { align-self: flex-end; }
</style>
