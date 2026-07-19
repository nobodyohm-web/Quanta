<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import StrengthMeter from "./StrengthMeter.svelte";
  import QuantaMark from "./brand/QuantaMark.svelte";
  import LanguageSelect from "./LanguageSelect.svelte";
  import { t } from "./i18n.svelte";

  let {
    onCreated = (_pk: string) => {},
    onSwitchToUnlock = () => {},
  } = $props<{
    onCreated?: (pk: string) => void;
    onSwitchToUnlock?: () => void;
  }>();

  // ── Flow state ──────────────────────────────────────────────────────────
  // "create" → "backup" (show phrase) → "verify" (re-enter 3 words) → done.
  // "restore" is a parallel entry (enter phrase + new password).
  type Step = "create" | "backup" | "verify" | "restore";
  let step = $state<Step>("create");

  let pseudo = $state("");
  let pass = $state("");
  let confirmPass = $state("");
  let loading = $state(false);
  let err = $state("");

  // Recovery phrase (only in memory during onboarding, then discarded).
  let phrase = $state("");
  let phraseWords = $derived(phrase.trim().split(/\s+/).filter(Boolean));
  let savedAck = $state(false);

  // Verify step: three random word positions the user must re-type.
  let checkIdx = $state<number[]>([]);
  let checkVals = $state<string[]>(["", "", ""]);

  // Restore step.
  let restorePhrase = $state("");

  // Pending public key from create/restore, revealed only after the flow completes.
  let pendingPk = $state("");

  // ── Password strength gate (enforced, not just shown) ─────────────────────
  const passStrong = $derived(
    pass.length >= 10 &&
      /[a-z]/i.test(pass) &&
      /[0-9]/.test(pass) &&
      // a bit of variety: a symbol OR ≥ 14 chars
      (/[^a-z0-9]/i.test(pass) || pass.length >= 14),
  );
  const passMatch = $derived(confirmPass.length > 0 && confirmPass === pass);

  // ── @pseudo validity + availability ───────────────────────────────────────
  const PSEUDO_RE = /^[a-z][a-z0-9_]{2,19}$/;
  const pseudoValid = $derived(PSEUDO_RE.test(pseudo.trim().toLowerCase()));
  let pseudoStatus = $state<"idle" | "checking" | "free" | "taken">("idle");
  let checkTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    const p = pseudo.trim().toLowerCase();
    pseudoStatus = "idle";
    if (checkTimer) clearTimeout(checkTimer);
    if (!PSEUDO_RE.test(p)) return;
    pseudoStatus = "checking";
    checkTimer = setTimeout(async () => {
      try {
        const free = await invoke<boolean>("is_username_available", { username: p });
        // Guard against a stale check if the user kept typing.
        if (pseudo.trim().toLowerCase() === p) pseudoStatus = free ? "free" : "taken";
      } catch {
        if (pseudo.trim().toLowerCase() === p) pseudoStatus = "idle";
      }
    }, 350);
  });

  const canCreate = $derived(pseudoValid && pseudoStatus !== "taken" && passStrong && passMatch);

  // ── Step 1 — create the identity, then fetch the recovery phrase ──────────
  async function create() {
    err = "";
    if (!canCreate) {
      err = t("welcome.errFix");
      return;
    }
    loading = true;
    try {
      const name = pseudo.trim().toLowerCase();
      const id = await invoke<{ public_key_hex: string }>("create_identity", {
        displayName: name,
        password: pass,
      });
      pendingPk = id.public_key_hex;
      // The recovery phrase — the ONLY backup of the funds. Must be saved.
      phrase = await invoke<string>("get_recovery_phrase");
      // Pick 3 distinct random positions to verify later.
      checkIdx = pickThree(phraseWords.length);
      checkVals = ["", "", ""];
      savedAck = false;
      step = "backup";
    } catch (e) {
      err = (e as Error)?.toString() || t("welcome.errCreate");
    } finally {
      loading = false;
    }
  }

  function pickThree(n: number): number[] {
    const set = new Set<number>();
    while (set.size < 3 && set.size < n) set.add(Math.floor(Math.random() * n));
    return [...set].sort((a, b) => a - b);
  }

  let phraseCopied = $state(false);
  async function copyPhrase() {
    await navigator.clipboard.writeText(phrase);
    phraseCopied = true;
    setTimeout(() => (phraseCopied = false), 1800);
  }

  // ── Step 3 — verify the user actually saved the phrase ────────────────────
  function finishVerify() {
    err = "";
    const ok = checkIdx.every((wi, i) => checkVals[i].trim().toLowerCase() === phraseWords[wi]);
    if (!ok) {
      err = t("verify.errWrong");
      return;
    }
    loading = true;
    (async () => {
      try {
        // Claim the @pseudo now that the identity exists (best-effort — the wallet
        // is already usable even if the claim races another node).
        await invoke("claim_username", { username: pseudo.trim().toLowerCase() }).catch(() => {});
      } finally {
        // Scrub the phrase from memory.
        phrase = "";
        checkVals = ["", "", ""];
        loading = false;
        onCreated(pendingPk);
      }
    })();
  }

  // ── Restore an existing wallet from its recovery phrase ───────────────────
  async function restore() {
    err = "";
    const words = restorePhrase.trim().split(/\s+/).filter(Boolean);
    if (words.length !== 24) {
      err = t("restore.errWords");
      return;
    }
    if (!passStrong || !passMatch) {
      err = t("welcome.errFix");
      return;
    }
    loading = true;
    try {
      const id = await invoke<{ public_key_hex: string }>("restore_from_phrase", {
        mnemonic: words.join(" ").toLowerCase(),
        displayName: pseudo.trim().toLowerCase() || "restored",
        password: pass,
      });
      restorePhrase = "";
      onCreated(id.public_key_hex);
    } catch (e) {
      err = t("restore.errInvalid");
    } finally {
      loading = false;
    }
  }

  function goRestore() {
    err = "";
    pass = "";
    confirmPass = "";
    step = "restore";
  }
  function backToCreate() {
    err = "";
    step = "create";
  }
</script>

<div class="welcome">
  <div class="wrap">
    <!-- Brand moment -->
    <div class="hero">
      <div class="brand">
        <QuantaMark size={34} tone="aurora" />
        <span class="wordmark">QUANTA</span>
      </div>
      <h1 class="headline">{@html t("welcome.headline")}</h1>
      <p class="sub">{@html t("welcome.sub")}</p>
    </div>

    <!-- ── Step: create ────────────────────────────────────────────────── -->
    {#if step === "create"}
      <div class="card panel">
        <div class="form">
          <div class="fg">
            <input
              type="text"
              class="input input-lg"
              placeholder={t("welcome.pseudo")}
              bind:value={pseudo}
              maxlength="20"
              autocomplete="off"
            />
            <div class="hint-row">
              {#if pseudo && !pseudoValid}
                <span class="hint bad">{t("welcome.pseudoInvalid")}</span>
              {:else if pseudoStatus === "checking"}
                <span class="hint">{t("welcome.pseudoChecking")}</span>
              {:else if pseudoStatus === "free"}
                <span class="hint good">✓ {t("welcome.pseudoFree")}</span>
              {:else if pseudoStatus === "taken"}
                <span class="hint bad">✗ {t("welcome.pseudoTaken")}</span>
              {/if}
            </div>
          </div>

          <div class="fg">
            <input
              type="password"
              class="input input-lg"
              placeholder={t("welcome.password")}
              bind:value={pass}
              autocomplete="new-password"
            />
            <StrengthMeter password={pass} />
          </div>

          <div class="fg">
            <input
              type="password"
              class="input input-lg"
              placeholder={t("welcome.confirm")}
              bind:value={confirmPass}
              autocomplete="new-password"
            />
            {#if confirmPass && !passMatch}
              <span class="hint bad">{t("welcome.errMismatch")}</span>
            {/if}
          </div>

          {#if err}<div class="err">{err}</div>{/if}

          <button class="btn btn-primary cta" onclick={create} disabled={loading || !canCreate}>
            {loading ? t("welcome.creating") : t("welcome.continue")}
          </button>

          <div class="links">
            <button class="ghost-link" onclick={goRestore}>{t("welcome.restoreLink")}</button>
            <button class="ghost-link" onclick={onSwitchToUnlock}>{t("welcome.haveIdentity")}</button>
          </div>
        </div>
      </div>

    <!-- ── Step: backup the recovery phrase ────────────────────────────── -->
    {:else if step === "backup"}
      <div class="card panel">
        <h2 class="step-title">{t("backup.title")}</h2>
        <p class="step-intro">{@html t("backup.intro")}</p>
        <div class="phrase">
          {#each phraseWords as w, i}
            <span class="word"><b>{i + 1}</b>{w}</span>
          {/each}
        </div>
        <button class="btn btn-ghost copybtn" onclick={copyPhrase}>
          {phraseCopied ? t("backup.copied") : t("backup.copy")}
        </button>
        <p class="warn">{@html t("backup.warning")}</p>
        <label class="ack">
          <input type="checkbox" bind:checked={savedAck} />
          <span>{t("backup.saved")}</span>
        </label>
        {#if err}<div class="err">{err}</div>{/if}
        <button class="btn btn-primary cta" disabled={!savedAck} onclick={() => (step = "verify")}>
          {t("welcome.continue")}
        </button>
      </div>

    <!-- ── Step: verify the phrase was saved ───────────────────────────── -->
    {:else if step === "verify"}
      <div class="card panel">
        <h2 class="step-title">{t("verify.title")}</h2>
        <p class="step-intro">{t("verify.intro")}</p>
        <div class="form">
          {#each checkIdx as wi, i}
            <div class="fg">
              <input
                type="text"
                class="input input-lg"
                placeholder={`${t("verify.word")} #${wi + 1}`}
                bind:value={checkVals[i]}
                autocomplete="off"
              />
            </div>
          {/each}
          {#if err}<div class="err">{err}</div>{/if}
          <button class="btn btn-primary cta" disabled={loading} onclick={finishVerify}>
            {loading ? t("welcome.creating") : t("verify.finish")}
          </button>
          <div class="links">
            <button class="ghost-link" onclick={() => (step = "backup")}>{t("verify.back")}</button>
          </div>
        </div>
      </div>

    <!-- ── Step: restore ───────────────────────────────────────────────── -->
    {:else if step === "restore"}
      <div class="card panel">
        <h2 class="step-title">{t("restore.title")}</h2>
        <p class="step-intro">{t("restore.intro")}</p>
        <div class="form">
          <textarea
            class="input phrase-input"
            rows="3"
            placeholder={t("restore.phrasePh")}
            bind:value={restorePhrase}
            autocomplete="off"
          ></textarea>
          <div class="fg">
            <input
              type="password"
              class="input input-lg"
              placeholder={t("restore.newPassword")}
              bind:value={pass}
              autocomplete="new-password"
            />
            <StrengthMeter password={pass} />
          </div>
          <div class="fg">
            <input
              type="password"
              class="input input-lg"
              placeholder={t("welcome.confirm")}
              bind:value={confirmPass}
              autocomplete="new-password"
            />
            {#if confirmPass && !passMatch}<span class="hint bad">{t("welcome.errMismatch")}</span>{/if}
          </div>
          {#if err}<div class="err">{err}</div>{/if}
          <button class="btn btn-primary cta" disabled={loading} onclick={restore}>
            {loading ? t("welcome.creating") : t("restore.cta")}
          </button>
          <div class="links">
            <button class="ghost-link" onclick={backToCreate}>{t("restore.back")}</button>
          </div>
        </div>
      </div>
    {/if}

    <p class="security-note">{@html t("welcome.securityNote")}</p>
    <div class="lang-row"><LanguageSelect /></div>
  </div>
</div>

<style>
  /* Inscription — niveau banque : blanc net, typo héros, un seul accent teal.
     Zéro fond de particules, zéro rail aurora (retirés). */
  .welcome {
    min-height: 100vh; display: flex; background: var(--canvas);
    padding: 44px 24px; overflow-y: auto;
  }
  .wrap {
    width: 100%; max-width: 416px; margin: auto;
    display: flex; flex-direction: column; gap: 16px;
    animation: welcomeRise var(--dur-med) var(--ease-out);
  }
  @keyframes welcomeRise {
    from { opacity: 0; transform: translateY(12px); }
    to   { opacity: 1; transform: none; }
  }
  @media (prefers-reduced-motion: reduce) { .wrap { animation: none; } }

  /* Le moment de marque — sur le fond clair, pas dans une carte */
  .hero { padding: 2px 2px 2px; }
  .brand { display: flex; align-items: center; gap: 11px; margin-bottom: 22px; }
  .wordmark {
    font-size: 15px; font-weight: 800; letter-spacing: 0.12em; color: var(--color-text-0);
  }
  .headline {
    font-size: 30px; font-weight: 700; letter-spacing: -0.032em; line-height: 1.08;
    color: var(--color-text-0); margin: 0 0 9px; text-wrap: balance;
  }
  .sub { font-size: 15px; color: var(--color-text-2); line-height: 1.5; margin: 0; }

  /* Panneaux — cartes blanches (.card global) ; agencement bank-grade */
  .panel { padding: 24px; }
  .form { display: flex; flex-direction: column; gap: 14px; }
  .fg { display: flex; flex-direction: column; gap: 6px; }
  .input-lg { padding: 13px 15px; font-size: 15px; border-radius: 11px; }
  .cta { width: 100%; padding: 13px; font-size: 15px; margin-top: 4px; }
  .links { display: flex; flex-direction: column; align-items: center; gap: 9px; margin-top: 8px; }
  .ghost-link {
    background: none; border: 0; padding: 0; cursor: pointer; font-family: inherit;
    font-size: 13px; color: var(--color-text-2); transition: color var(--dur-fast) ease;
  }
  .ghost-link:hover { color: var(--color-accent-hover); }
  .err {
    font-size: 13px; color: var(--color-red);
    background: rgba(229,72,77,0.06); border: 1px solid rgba(229,72,77,0.16);
    padding: 10px 13px; border-radius: 10px;
  }
  .security-note {
    font-size: 12px; color: var(--color-text-3); text-align: center;
    line-height: 1.5; margin: 4px 8px 0;
  }
  .lang-row { display: flex; justify-content: center; margin-top: 2px; }

  /* Validité du pseudo */
  .hint-row { min-height: 18px; margin-top: 4px; }
  .hint { font-size: 12px; color: var(--color-text-2); }
  .hint.good { color: var(--color-green); font-weight: 600; }
  .hint.bad { color: var(--color-red); }

  /* Étapes backup / verify / restore */
  .step-title { font-size: 20px; font-weight: 700; margin: 0 0 6px; letter-spacing: -0.02em; color: var(--color-text-0); }
  .step-intro { font-size: 13.5px; color: var(--color-text-2); margin: 0 0 16px; line-height: 1.5; }

  .phrase {
    display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px;
    background: var(--color-bg-1); border: 1px solid var(--color-border);
    border-radius: 12px; padding: 14px; margin-bottom: 12px;
  }
  .word {
    font-family: var(--font-mono); font-size: 13px;
    display: flex; align-items: baseline; gap: 6px; word-break: break-all; color: var(--color-text-0);
  }
  .word b { color: var(--color-text-3); font-weight: 600; font-size: 11px; min-width: 16px; }
  .copybtn { width: 100%; margin-bottom: 12px; }
  .phrase-input {
    width: 100%; resize: vertical; font-family: var(--font-mono);
    font-size: 14px; line-height: 1.6; margin-bottom: 12px;
  }
  .warn {
    font-size: 12px; color: var(--color-text-1); background: var(--color-bg-2);
    border-radius: 8px; padding: 10px 12px; margin: 0 0 12px; line-height: 1.5;
  }
  .ack { display: flex; align-items: center; gap: 9px; font-size: 13px; margin-bottom: 16px; cursor: pointer; color: var(--color-text-1); }
  .ack input { width: 16px; height: 16px; accent-color: var(--color-accent); }
</style>
