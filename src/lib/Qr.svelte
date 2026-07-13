<script lang="ts">
  // QR code rendered fully offline (qrcode-generator, bundled — zero network).
  // Black modules on a white rounded card: the highest-contrast, most
  // scannable form, the exact convention every mobile wallet user knows.
  import qrcode from "qrcode-generator";

  let { data, size = 172 } = $props<{ data: string; size?: number }>();

  const svg = $derived.by(() => {
    try {
      const qr = qrcode(0, "M");
      qr.addData(data);
      qr.make();
      return qr.createSvgTag({ cellSize: 4, margin: 0, scalable: true });
    } catch {
      return "";
    }
  });
</script>

{#if svg}
  <div class="qr-card" style="width:{size}px;height:{size}px;" role="img" aria-label={data}>
    {@html svg}
  </div>
{/if}

<style>
  .qr-card {
    background: #fff;
    border-radius: 14px;
    padding: 12px;
    box-shadow: 0 4px 18px rgba(48, 40, 30, 0.10), 0 1px 3px rgba(48, 40, 30, 0.06);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .qr-card :global(svg) {
    width: 100%;
    height: 100%;
    display: block;
  }
</style>
