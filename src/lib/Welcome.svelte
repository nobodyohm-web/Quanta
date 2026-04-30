<script lang="ts">
  let { onContinue } = $props<{ onContinue: () => void }>();
</script>

<div class="welcome">
  <div class="w-content">
    <span class="w-logo">SOVA</span>
    <h1 class="w-headline">Votre argent.<br/>Votre réseau.<br/>Votre souveraineté.</h1>
    <p class="w-sub">Aucun cloud. Aucun intermédiaire. Vous êtes le serveur.</p>
    <button class="btn btn-primary w-cta" onclick={onContinue}>Commencer</button>
  </div>
</div>

<style>
  .welcome {
    height: 100vh;
    display: flex; align-items: center; justify-content: center;
    background: var(--color-bg-0);
    padding: 24px;
  }
  .w-content {
    text-align: center;
    max-width: 380px;
    animation: fadeIn 0.15s ease-out;
  }
  .w-logo {
    display: block;
    font-size: 14px; font-weight: 700;
    letter-spacing: 0.15em;
    color: var(--color-text-2);
    margin-bottom: 32px;
  }
  .w-headline {
    font-size: 32px; font-weight: 700;
    letter-spacing: -0.03em;
    line-height: 1.2;
    margin-bottom: 16px;
  }
  .w-sub {
    font-size: 15px;
    color: var(--color-text-1);
    line-height: 1.6;
    margin-bottom: 40px;
  }
  .w-cta {
    padding: 14px 48px;
    font-size: 16px;
    border-radius: var(--radius-sm);
  }
</style>
