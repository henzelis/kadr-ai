<script lang="ts">
  import { formatShortDate, metaState } from "$lib/format";
  import { t } from "$lib/i18n/uk";
  import type { PagedLibrary } from "$lib/library.svelte";
  import type { KindFilter, MediaView, SortKey } from "$lib/types";
  import Thumb from "./Thumb.svelte";
  import { formatDuration } from "$lib/format";

  let {
    library, demo, scanning = false, selected, scroller, kind = $bindable(), query = $bindable(), sort = $bindable(), title, onPick,
  }: {
    library: PagedLibrary;
    demo: boolean;
    scanning?: boolean;
    selected: Set<number>;
    /** The element that scrolls (the main column). */
    scroller: HTMLElement | undefined;
    kind: KindFilter;
    query: string;
    sort: SortKey;
    title: string;
    onPick: (item: MediaView, additive: boolean) => void;
  } = $props();

  const MIN_CARD = 210;
  const GAP = 13;
  /** Height of the text part under the picture; fixed so rows are uniform. */
  const INFO_H = 96;
  const OVERSCAN_ROWS = 3;

  let grid: HTMLDivElement | undefined = $state();
  let width = $state(0);
  let scrollTop = $state(0);
  let viewport = $state(800);
  let gridTop = $state(0);

  const cols = $derived(Math.max(1, Math.floor((width + GAP) / (MIN_CARD + GAP))));
  const cardW = $derived(cols > 0 ? (width - GAP * (cols - 1)) / cols : MIN_CARD);
  const rowH = $derived(Math.round(cardW / 1.52) + INFO_H + 2);
  const stride = $derived(rowH + GAP);
  const rows = $derived(Math.ceil(library.total / cols));
  const firstRow = $derived(Math.max(0, Math.floor((scrollTop - gridTop) / stride) - OVERSCAN_ROWS));
  const lastRow = $derived(Math.min(rows, Math.ceil((scrollTop - gridTop + viewport) / stride) + OVERSCAN_ROWS));
  const start = $derived(firstRow * cols);
  const end = $derived(Math.min(library.total, lastRow * cols));
  const indices = $derived(Array.from({ length: Math.max(0, end - start) }, (_, i) => start + i));

  $effect(() => library.ensure(start, end));

  function measure() {
    if (!scroller || !grid) return;
    scrollTop = scroller.scrollTop;
    viewport = scroller.clientHeight;
    gridTop = grid.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop;
  }

  $effect(() => {
    if (!scroller || !grid) return;
    const el = scroller;
    measure();
    const onScroll = () => measure();
    el.addEventListener("scroll", onScroll, { passive: true });
    const ro = new ResizeObserver(() => {
      width = grid?.clientWidth ?? width;
      measure();
    });
    ro.observe(grid);
    ro.observe(el);
    return () => {
      el.removeEventListener("scroll", onScroll);
      ro.disconnect();
    };
  });

  function pick(event: MouseEvent, item: MediaView) {
    onPick(item, event.ctrlKey || event.metaKey || event.shiftKey);
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

  <div class="section-heading"><span>{title}</span><small>{t.totalFiles(library.total)}</small></div>

  {#if library.total === 0 && !library.loading}
    {#if scanning && !query}
      <div class="empty-state"><strong>{t.scanning}</strong>{t.scanningHint}</div>
    {:else}
      <div class="empty-state"><strong>{t.nothingFound}</strong>{t.nothingFoundHint}</div>
    {/if}
  {/if}

  <div class="vgrid" bind:this={grid} style:height="{Math.max(0, rows * stride - GAP)}px">
    <div class="window" style:transform="translateY({firstRow * stride}px)"
      style:grid-template-columns="repeat({cols}, minmax(0, 1fr))" style:grid-auto-rows="{rowH}px">
      {#each indices as index (index)}
        {@const item = library.item(index)}
        {#if item}
          {@const state = metaState(item)}
          <button class="media-card" class:selected={selected.has(item.id)} aria-pressed={selected.has(item.id)}
            onclick={(e) => pick(e, item)}>
            <div class="media-visual">
              <Thumb {item} size="grid" {demo} />
              <span class="kind-badge"><span class="tiny-dot"></span>{t.kindBadge[item.kind]}{#if item.companions.some((c) => c.role === "live_video")}<span class="live">LIVE</span>{/if}</span>
              <span class="selection-check" aria-hidden="true">✓</span>
              {#if item.duration_ms !== null}<span class="duration">{formatDuration(item.duration_ms)}</span>{/if}
            </div>
            <div class="media-info">
              <div class="media-name-row">
                <span class="media-name" title={item.file_name}>{item.file_name}</span>
                {#if item.duplicates.length}<span class="dup-tag" title={t.otherCopies}>{t.copies(item.duplicates.length)}</span>{/if}
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
          <div class="media-card skeleton" aria-hidden="true"></div>
        {/if}
      {/each}
    </div>
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
  .vgrid { position: relative; }
  .window { display: grid; gap: 13px; will-change: transform; }
  .media-card {
    padding: 0; text-align: left; background: var(--surface); border: 1px solid var(--line); border-radius: 12px;
    min-width: 0; overflow: hidden; position: relative; display: flex; flex-direction: column;
    transition: border-color .14s, box-shadow .14s;
  }
  .media-card:hover { border-color: light-dark(#bdcbb5, #50644f); box-shadow: var(--shadow); }
  .media-card.selected { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent); }
  .skeleton { background: var(--surface-soft); border-style: dashed; }
  .media-visual { position: relative; aspect-ratio: 1.52 / 1; overflow: hidden; background: var(--surface-soft); flex: 0 0 auto; }
  .kind-badge { position: absolute; top: 8px; left: 8px; padding: 5px 7px; display: flex; gap: 5px; align-items: center; border-radius: 6px; background: rgb(22 29 22 / 76%); color: #fff; font-size: 9px; font-weight: 700; }
  .live { margin-left: 3px; padding-left: 5px; border-left: 1px solid rgb(255 255 255 / 35%); }
  .tiny-dot { width: 5px; height: 5px; border-radius: 50%; background: #c8e2bb; }
  .selection-check { position: absolute; top: 8px; right: 8px; border-radius: 6px; color: #fff; width: 22px; height: 22px; display: grid; place-items: center; font-size: 12px; opacity: 0; background: var(--accent); }
  .media-card.selected .selection-check { opacity: 1; }
  .duration { position: absolute; bottom: 8px; right: 8px; border-radius: 5px; background: rgb(20 24 21 / 79%); color: #fff; padding: 4px 6px; font-size: 9px; font-weight: 700; }
  .media-info { padding: 10px 11px 11px; min-height: 0; overflow: hidden; }
  .media-name-row { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .media-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; font-weight: 720; flex: 1; }
  .dup-tag { color: var(--amber); background: var(--amber-soft); border-radius: 4px; padding: 2px 4px; font-size: 8px; font-weight: 750; white-space: nowrap; }
  .format-tag { color: var(--muted); border: 1px solid var(--line); border-radius: 4px; padding: 2px 4px; font-size: 8px; font-weight: 750; }
  .media-sub { display: flex; gap: 6px; color: var(--muted); font-size: 10px; margin-top: 8px; min-width: 0; }
  .place { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .state { margin-top: 10px; max-width: 100%; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .state-dot { width: 5px; height: 5px; border-radius: 50%; background: currentColor; flex: 0 0 auto; }
  .empty-state { border: 1px dashed var(--line); border-radius: 12px; padding: 45px 20px; text-align: center; color: var(--muted); background: var(--surface); }
  .empty-state strong { display: block; color: var(--text); margin-bottom: 7px; font-size: 14px; }
</style>
