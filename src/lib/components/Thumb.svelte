<script lang="ts">
  import { thumbUrl } from "$lib/api";
  import { t } from "$lib/i18n/uk";
  import { sceneSvg } from "$lib/scenes";
  import type { MediaView } from "$lib/types";

  let { item, size, demo, fit = "cover" }: {
    item: MediaView;
    size: "grid" | "preview";
    demo: boolean;
    fit?: "cover" | "contain";
  } = $props();

  let failed = $state(false);
  let loaded = $state(false);
  // A different item (or a changed file) gets a fresh attempt.
  $effect(() => {
    void [item.id, item.mtime, size];
    failed = false;
    loaded = false;
  });
</script>

{#if demo}
  <!-- generated placeholder artwork, no user content -->
  {@html sceneSvg(item.id)}
{:else if failed}
  <div class="placeholder" title={t.noThumb}>
    <span class="icon" aria-hidden="true">{item.kind === "video" ? "▶" : "▣"}</span>
    <span class="ext">{item.extension.toUpperCase() || "?"}</span>
    {#if size === "preview"}<small>{t.noThumb}</small>{/if}
  </div>
{:else}
  <img
    src={thumbUrl(item.id, size, item.mtime)}
    alt={item.file_name}
    loading="lazy"
    decoding="async"
    draggable="false"
    class:contain={fit === "contain"}
    class:loaded
    onload={() => (loaded = true)}
    onerror={() => (failed = true)}
  />
{/if}

<style>
  img { display: block; width: 100%; height: 100%; object-fit: cover; opacity: 0; transition: opacity .15s; }
  img.contain { object-fit: contain; }
  img.loaded { opacity: 1; }
  .placeholder {
    width: 100%; height: 100%; display: grid; place-content: center; justify-items: center; gap: 6px;
    color: var(--muted); background: repeating-linear-gradient(135deg, var(--surface-soft) 0 12px, transparent 12px 24px);
  }
  .icon { font-size: 24px; opacity: .6; }
  .ext { font-size: 11px; font-weight: 750; letter-spacing: .06em; }
  small { font-size: 10px; }
  :global(svg) { display: block; width: 100%; height: 100%; }
</style>
