<script lang="ts">
  import { t } from "$lib/i18n/uk";

  let { kind, initial, canReset, busy, onSave, onReset, onCancel }: {
    kind: "date" | "place";
    initial: string;
    canReset: boolean;
    busy: boolean;
    onSave: (value: string) => void;
    onReset: () => void;
    onCancel: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let value = $state(initial);
  let input: HTMLInputElement | undefined = $state();
  $effect(() => input?.focus());

  function escape(e: KeyboardEvent) {
    if (e.key === "Escape") onCancel();
  }
</script>

<form class="edit" onsubmit={(e) => { e.preventDefault(); if (value.trim()) onSave(value.trim()); }}>
  <label class="visually-hidden" for="edit-{kind}">{kind === "date" ? t.editDateTitle : t.editPlaceTitle}</label>
  {#if kind === "date"}
    <input id="edit-{kind}" bind:this={input} type="datetime-local" step="60" min="1826-01-01T00:00" max="2200-12-31T23:59" bind:value required onkeydown={escape} />
  {:else}
    <input id="edit-{kind}" bind:this={input} type="text" maxlength="80" placeholder={t.placePlaceholder} bind:value required onkeydown={escape} />
  {/if}
  <div class="row">
    <button type="submit" class="primary small" disabled={busy || !value.trim()}>{t.save}</button>
    <button type="button" class="quiet small" disabled={busy} onclick={onCancel}>{t.cancelEdit}</button>
  </div>
  {#if canReset}
    <button type="button" class="link" disabled={busy} onclick={onReset}>↺ {t.resetToMetadata}</button>
  {/if}
  <div class="hint">{t.editHint}</div>
</form>

<style>
  .edit { display: grid; gap: 8px; margin: 4px 0 6px; }
  input { height: 34px; border: 1px solid var(--line); background: var(--surface-raised); border-radius: 8px; padding: 0 9px; font-size: 12px; width: 100%; }
  .row { display: flex; gap: 6px; }
  .small { min-height: 30px; font-size: 11px; padding: 0 11px; }
  .link { border: 0; background: transparent; color: var(--accent-strong); font-size: 11px; padding: 0; text-align: left; font-weight: 650; }
  .hint { font-size: 10px; color: var(--muted); line-height: 1.4; }
</style>
