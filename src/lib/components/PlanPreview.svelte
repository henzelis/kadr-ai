<script lang="ts">
  import { effectiveDate, formatCount, formatShortDate } from "$lib/format";
  import { t } from "$lib/i18n";
  import type { AppSettings, MediaView, OperationMode } from "$lib/types";

  let { items, settings, destinationOf, duplicateIds, onBack }: {
    items: MediaView[];
    settings: AppSettings;
    destinationOf: (item: MediaView) => string;
    duplicateIds: Set<number>;
    onBack: () => void;
  } = $props();

  // Never remembered between plans: every plan starts in Copy mode.
  let mode = $state<OperationMode>("copy");

  const incomplete = $derived(
    items.filter((i) => !duplicateIds.has(i.id) && (!effectiveDate(i, settings) || !i.place_label)).length,
  );
  const duplicates = $derived(items.filter((i) => duplicateIds.has(i.id)).length);
  const ready = $derived(items.length - incomplete - duplicates);

  function warning(item: MediaView): string | null {
    if (duplicateIds.has(item.id)) return t.plan.statusDuplicate;
    const date = effectiveDate(item, settings);
    if (!date) return t.state.noDate;
    if (date.status !== "confirmed") return t.state.review;
    if (item.place_status === "gps_no_place") return t.state.gpsNoPlace;
    if (!item.place_label) return t.state.noGps;
    return null;
  }
</script>

<section>
  <div class="plan-head">
    <div class="section-heading">{t.plan.heading}</div>
    <div class="sub">{t.plan.sub}</div>
  </div>
  <div class="plan-summary">
    <div class="plan-stat"><div class="num">{formatCount(Math.max(ready, 0))}</div><div class="label">{t.plan.ready}</div></div>
    <div class="plan-stat"><div class="num">{formatCount(incomplete)}</div><div class="label">{t.plan.incomplete}</div></div>
    <div class="plan-stat"><div class="num">{formatCount(duplicates)}</div><div class="label">{t.plan.conflicts}</div></div>
  </div>
  <div class="plan-note" role="note">ⓘ {t.plan.note}</div>
  <div class="table-wrap">
    <table>
      <thead><tr><th>{t.plan.colSource}</th><th>{t.plan.colDate}</th><th>{t.plan.colPlace}</th><th>{t.plan.colDest}</th><th>{t.plan.colStatus}</th></tr></thead>
      <tbody>
        {#each items as item (item.id)}
          {@const warn = warning(item)}
          {@const date = effectiveDate(item, settings)}
          <tr>
            <td><div class="file"><span class="file-mini">{item.extension.toUpperCase().slice(0, 4)}</span>{item.file_name}</div></td>
            <td>{date ? formatShortDate(date.local) : "—"}</td>
            <td>{item.place_label ?? "—"}</td>
            <td class="dest">{destinationOf(item)}</td>
            <td><span class="chip" class:review={warn !== null}>{warn ?? `✓ ${t.plan.statusReady}`}</span></td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
  <div class="plan-bottom">
    <div class="mode" role="radiogroup" aria-label={t.plan.mode}>
      <span class="mode-label">{t.plan.mode}</span>
      <button class="mode-btn" class:active={mode === "copy"} role="radio" aria-checked={mode === "copy"} onclick={() => (mode = "copy")}>⧉ {t.plan.copy}</button>
      <button class="mode-btn move" class:active={mode === "move"} role="radio" aria-checked={mode === "move"} onclick={() => (mode = "move")}>↗ {t.plan.move}</button>
      <span class="mode-hint" class:warn={mode === "move"}>{mode === "copy" ? t.plan.copyHint : t.plan.moveHint}</span>
    </div>
    <div class="actions">
      <button class="secondary" onclick={onBack}>← {t.plan.back}</button>
      <button class="primary" disabled title={t.plan.applyDisabled}>{t.plan.apply} →</button>
    </div>
  </div>
</section>

<style>
  .plan-head { margin-bottom: 17px; }
  .section-heading { font-size: 14px; font-weight: 720; }
  .sub { font-size: 11px; color: var(--muted); margin-top: 5px; }
  .plan-summary { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px; margin-bottom: 15px; }
  .plan-stat { border: 1px solid var(--line); background: var(--surface); border-radius: 10px; padding: 13px; }
  .num { font-size: 19px; font-weight: 760; }
  .label { color: var(--muted); font-size: 10px; margin-top: 5px; }
  .plan-note {
    padding: 12px 13px; border: 1px solid light-dark(#e8d9b9, #51432b); background: var(--amber-soft);
    color: var(--amber); border-radius: 10px; line-height: 1.5; font-size: 11px; margin-bottom: 14px;
  }
  .table-wrap { background: var(--surface); border: 1px solid var(--line); border-radius: 11px; overflow-x: auto; }
  table { width: 100%; border-collapse: collapse; min-width: 700px; }
  th { text-align: left; padding: 11px 12px; color: var(--muted); background: var(--surface-soft); text-transform: uppercase; letter-spacing: .06em; font-size: 9px; font-weight: 750; }
  td { padding: 12px; border-top: 1px solid var(--line); vertical-align: top; font-size: 11px; line-height: 1.5; }
  .file { display: flex; gap: 8px; align-items: center; font-weight: 700; }
  .file-mini { width: 32px; height: 25px; border-radius: 5px; display: grid; place-items: center; background: var(--surface-soft); color: var(--accent-strong); font-size: 8px; font-weight: 800; flex: 0 0 auto; }
  .dest { color: var(--muted); overflow-wrap: anywhere; min-width: 220px; }
  .plan-bottom { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-top: 15px; flex-wrap: wrap; }
  .mode { display: flex; gap: 7px; align-items: center; flex-wrap: wrap; }
  .mode-label { font-size: 11px; color: var(--muted); margin-right: 3px; }
  .mode-btn { border: 1px solid var(--line); border-radius: 8px; padding: 8px 10px; background: var(--surface); color: var(--muted); font-size: 11px; font-weight: 700; }
  .mode-btn.active { background: var(--accent-soft); border-color: light-dark(#c5d7bd, #486047); color: var(--accent-strong); }
  .mode-btn.move.active { background: var(--amber-soft); border-color: var(--amber); color: var(--amber); }
  .mode-hint { font-size: 10px; color: var(--muted); max-width: 320px; }
  .mode-hint.warn { color: var(--amber); }
  .actions { display: flex; gap: 8px; }
</style>
