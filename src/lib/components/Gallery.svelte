<script lang="ts">
  import { formatDuration, formatShortDate, metaState } from "$lib/format";
  import { t } from "$lib/i18n/uk";
  import { sceneSvg } from "$lib/scenes";
  import type { MediaView } from "$lib/types";

  export type KindFilter = "all" | "photo" | "video";
  export type SortKey = "date" | "name" | "size";

  let {
    items, total, selected, kind = $bindable(), query = $bindable(), sort = $bindable(), title, onPick,
  }: {
    items: MediaView[];
    total: number;
    selected: Set<number>;
    kind: KindFilter;
    query: string;
    sort: SortKey;
    title: string;
    onPick: (id: number, additive: boolean) => void;
  } = $props();

  function pick(event: MouseEvent | KeyboardEvent, id: number) {
    onPick(id, event.ctrlKey || event.metaKey || event.shiftKey);
  }
</script>

<section>
  <div class="view-controls">
    <div class="filter-pills" role="group" aria-label={t.format}>
      {#each [["all", t.kindAll], ["photo", t.kindPhoto], ["video", t.kindVideo]] as [value, label] (value)}
        <button class="filter-pill" class:active={kind === value} aria-pressed={kind === value}
          onclick={() => (kind = value as KindFilter)}>{label}</button>
      {/each}
    </div>
    <div class="view-right">
      <label class="search-wrap">
        <span class="visually-hidden">{t.searchPlaceholder}</span>
        <span class="search-icon" aria-hidden="true">⌕</span>
        <input class="search" type="search" placeholder={t.searchPlaceholder} bind:value={query} />
      </label>
      <select class="sort-select" aria-label={t.sortByDate} bind:value={sort}>
        <option value="date">{t.sortByDate}</option>
        <option value="name">{t.sortByName}</option>
        <option value="size">{t.sortBySize}</option>
      </select>
    </div>
  </div>

  <div class="section-heading"><span>{title}</span><small>{t.shown(items.length, total)}</small></div>

  <div class="media-grid">
    {#each items as item (item.id)}
      {@const state = metaState(item)}
      <button class="media-card" class:selected={selected.has(item.id)} aria-pressed={selected.has(item.id)}
        onclick={(e) => pick(e, item.id)}>
        <div class="media-visual">
          <!-- generated placeholder artwork, no user content -->
          {@html sceneSvg(item.scene)}
          <span class="kind-badge"><span class="tiny-dot"></span>{t.kindBadge[item.kind]}</span>
          <span class="selection-check" aria-hidden="true">✓</span>
          {#if item.duration_ms !== null}<span class="duration">{formatDuration(item.duration_ms)}</span>{/if}
        </div>
        <div class="media-info">
          <div class="media-name-row">
            <span class="media-name">{item.file_name}</span>
            <span class="format-tag">{item.extension.toUpperCase()}</span>
          </div>
          <div class="media-sub">
            <span>{item.captured ? formatShortDate(item.captured.local) : t.unknown}</span>
            <span aria-hidden="true">·</span>
            <span class="place">{item.place_label ?? "—"}</span>
          </div>
          <span class="chip state" class:review={state.tone === "review"} class:missing={state.tone === "missing"}>
            <span class="state-dot" aria-hidden="true"></span>{state.label}
          </span>
        </div>
      </button>
    {:else}
      <div class="empty-state"><strong>{t.nothingFound}</strong>{t.nothingFoundHint}</div>
    {/each}
  </div>
</section>

<style>
  .view-controls { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 10px; margin-bottom: 17px; }
  .filter-pills { display: flex; padding: 3px; gap: 2px; border: 1px solid var(--line); background: var(--surface); border-radius: 9px; }
  .filter-pill { border: 0; border-radius: 6px; min-height: 29px; padding: 0 12px; background: transparent; color: var(--muted); font-size: 11px; font-weight: 600; }
  .filter-pill.active { color: var(--text); background: var(--surface-soft); box-shadow: 0 1px 2px rgb(0 0 0 / 5%); }
  .view-right { display: flex; gap: 8px; align-items: center; }
  .search-wrap { position: relative; }
  .search-icon { position: absolute; left: 11px; top: 50%; transform: translateY(-50%); color: var(--muted); font-size: 15px; pointer-events: none; }
  .search { width: 210px; height: 35px; padding: 0 10px 0 33px; border-radius: 9px; border: 1px solid var(--line); background: var(--surface); }
  .search::placeholder { color: var(--muted); }
  .sort-select { height: 35px; border-radius: 9px; padding: 0 8px; background: var(--surface); border: 1px solid var(--line); }
  .section-heading { display: flex; align-items: baseline; gap: 9px; margin: 0 0 13px; font-size: 14px; font-weight: 720; }
  .section-heading small { font-size: 11px; color: var(--muted); font-weight: 500; }
  .media-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(210px, 1fr)); gap: 13px; }
  .media-card {
    padding: 0; text-align: left; background: var(--surface); border: 1px solid var(--line); border-radius: 12px;
    min-width: 0; overflow: hidden; position: relative; transition: border-color .14s, box-shadow .14s, transform .14s;
  }
  .media-card:hover { border-color: light-dark(#bdcbb5, #50644f); transform: translateY(-1px); box-shadow: var(--shadow); }
  .media-card.selected { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent); }
  .media-visual { position: relative; aspect-ratio: 1.52 / 1; overflow: hidden; background: var(--surface-soft); }
  .media-visual :global(svg) { display: block; width: 100%; height: 100%; }
  .kind-badge {
    position: absolute; top: 8px; left: 8px; padding: 5px 7px; display: flex; gap: 5px; align-items: center;
    border-radius: 6px; background: rgb(22 29 22 / 76%); color: #fff; font-size: 9px; font-weight: 700;
  }
  .tiny-dot { width: 5px; height: 5px; border-radius: 50%; background: #c8e2bb; }
  .selection-check {
    position: absolute; top: 8px; right: 8px; border-radius: 6px; color: #fff; width: 22px; height: 22px;
    display: grid; place-items: center; font-size: 12px; opacity: 0; background: var(--accent);
  }
  .media-card.selected .selection-check { opacity: 1; }
  .duration { position: absolute; bottom: 8px; right: 8px; border-radius: 5px; background: rgb(20 24 21 / 79%); color: #fff; padding: 4px 6px; font-size: 9px; font-weight: 700; }
  .media-info { padding: 10px 11px 11px; }
  .media-name-row { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .media-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; font-weight: 720; flex: 1; }
  .format-tag { color: var(--muted); border: 1px solid var(--line); border-radius: 4px; padding: 2px 4px; font-size: 8px; font-weight: 750; }
  .media-sub { display: flex; gap: 6px; color: var(--muted); font-size: 10px; margin-top: 8px; min-width: 0; }
  .place { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .state { margin-top: 10px; }
  .state-dot { width: 5px; height: 5px; border-radius: 50%; background: currentColor; }
  .empty-state {
    grid-column: 1 / -1; border: 1px dashed var(--line); border-radius: 12px; padding: 45px 20px;
    text-align: center; color: var(--muted); background: var(--surface);
  }
  .empty-state strong { display: block; color: var(--text); margin-bottom: 7px; font-size: 14px; }
</style>
