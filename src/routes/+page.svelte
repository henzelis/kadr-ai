<script lang="ts">
  import { onDestroy } from "svelte";
  import {
    cancelScan, errorMessage, getFolders, getLibrarySummary, getScanStatus, getSectionCounts, getSettings,
    inTauri, onScanEvents, pickDestinationFolder, pickSourceFolders, queryMedia, removeSourceRoot, startScan,
    updateSettings,
  } from "$lib/api";
  import Gallery from "$lib/components/Gallery.svelte";
  import Inspector from "$lib/components/Inspector.svelte";
  import PlanPreview from "$lib/components/PlanPreview.svelte";
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import Sidebar, { type Section } from "$lib/components/Sidebar.svelte";
  import Titlebar from "$lib/components/Titlebar.svelte";
  import { demoMedia } from "$lib/demo";
  import { demoDestination, formatCount, monthFolder } from "$lib/format";
  import { t } from "$lib/i18n/uk";
  import type {
    AppSettings, Folders, KindFilter, LastScan, LibrarySection, LibrarySummary, MediaView, ScanProgress, SectionCounts, SortKey,
  } from "$lib/types";

  const PAGE = 120;
  const demo = !inTauri;
  const toQuerySection: Record<"all" | "review" | "nodate" | "nogps" | "duplicates", LibrarySection> = {
    all: "all", review: "review", nodate: "no_date", nogps: "no_gps", duplicates: "duplicates",
  };

  let settings = $state<AppSettings | null>(null);
  let summary = $state<LibrarySummary | null>(null);
  let folders = $state<Folders | null>(null);
  let counts = $state<SectionCounts>({ all: 0, review: 0, nodate: 0, nogps: 0, duplicates: 0, with_date: 0, with_gps: 0 });
  let section = $state<Section>("all");
  let kind = $state<KindFilter>("all");
  let query = $state("");
  let sort = $state<SortKey>("date");
  let items = $state<MediaView[]>([]);
  let total = $state(0);
  let selected = $state(new Set<number>());
  let scan = $state<ScanProgress | null>(null);
  let lastScan = $state<LastScan | null>(null);
  let scanning = $state(false);
  let cancelling = $state(false);
  let settingsOpen = $state(false);
  let toast = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  let requestSeq = 0;

  const librarySection = $derived(section === "plan" || section === "history" ? "all" : section);
  const focused = $derived(selected.size === 1 ? (items.find((m) => selected.has(m.id)) ?? null) : null);
  const hasSources = $derived((folders?.sources.length ?? 0) > 0);

  function notify(text: string) {
    toast = text;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), 3600);
  }

  async function load(append = false) {
    const seq = ++requestSeq;
    try {
      const page = await queryMedia({
        section: toQuerySection[librarySection], kind, search: query, sort,
        offset: append ? items.length : 0, limit: PAGE,
      });
      if (seq !== requestSeq) return; // a newer query superseded this one
      items = append ? [...items, ...page.items] : page.items;
      total = page.total;
      if (!append && selected.size === 0 && items.length) selected = new Set([items[0].id]);
    } catch (e) {
      notify(errorMessage(e));
    }
  }

  async function refreshTotal() {
    try {
      const page = await queryMedia({ section: toQuerySection[librarySection], kind, search: query, sort, offset: 0, limit: 1 });
      total = page.total;
    } catch {
      // the next progress event will try again
    }
  }

  async function refreshCounts() {
    try {
      counts = await getSectionCounts();
      summary = await getLibrarySummary();
    } catch (e) {
      notify(errorMessage(e));
    }
  }

  // Re-query whenever a filter changes; typing in search is debounced.
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    void [librarySection, kind, sort];
    const q = query;
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => { void q; void load(); }, q ? 200 : 0);
  });

  function applyTheme(s: AppSettings) {
    const root = document.documentElement;
    if (s.theme === "system") delete root.dataset.theme;
    else root.dataset.theme = s.theme;
  }

  function pick(id: number, additive: boolean) {
    const next = new Set(additive ? selected : []);
    if (additive && next.has(id)) next.delete(id);
    else next.add(id);
    selected = next;
  }

  async function toggleTheme() {
    if (!settings) return;
    const dark = settings.theme === "dark" || (settings.theme === "system" && matchMedia("(prefers-color-scheme: dark)").matches);
    try {
      settings = await updateSettings({ ...settings, theme: dark ? "light" : "dark" });
      applyTheme(settings);
    } catch (e) {
      notify(errorMessage(e));
    }
  }

  async function addSources() {
    if (demo) return notify(t.demoHint);
    try {
      const next = await pickSourceFolders();
      if (next) folders = next;
    } catch (e) {
      notify(errorMessage(e));
    }
  }

  async function changeDestination() {
    if (demo) return notify(t.demoHint);
    try {
      const next = await pickDestinationFolder();
      if (next) {
        folders = next;
        settings = await getSettings();
      }
    } catch (e) {
      notify(errorMessage(e));
    }
  }

  async function removeSource(id: number) {
    try {
      folders = await removeSourceRoot(id);
      selected = new Set();
      await Promise.all([load(), refreshCounts()]);
    } catch (e) {
      notify(errorMessage(e));
    }
  }

  async function scanNow() {
    if (demo) return notify(t.demoHint);
    try {
      scan = null;
      scanning = true;
      cancelling = false;
      await startScan();
    } catch (e) {
      scanning = false;
      notify(errorMessage(e));
    }
  }

  async function stopScan() {
    cancelling = true;
    await cancelScan().catch((e) => notify(errorMessage(e)));
  }

  let unlisten: (() => void) | undefined;
  let lastRefresh = 0;
  $effect(() => {
    void (async () => {
      try {
        settings = await getSettings();
        applyTheme(settings);
        folders = await getFolders();
        await refreshCounts();
        const status = await getScanStatus();
        scanning = status.running;
        scan = status.progress;
        lastScan = status.last;
        unlisten = await onScanEvents({
          progress: (p) => {
            scan = p;
            scanning = true;
            // Show new files as they arrive, at most every 2 s.
            if (Date.now() - lastRefresh > 2000) {
              lastRefresh = Date.now();
              void refreshCounts();
              // A short list is simply reloaded; a longer one keeps the
              // user's place and only updates its total.
              if (items.length < PAGE) void load();
              else void refreshTotal();
            }
          },
          finished: (p) => {
            scan = p;
            scanning = false;
            cancelling = false;
            notify(p.phase === "cancelled" ? t.scanCancelled : t.scanFinished(p.processed, p.errors));
            void refreshCounts();
            void load();
            void getFolders().then((f) => (folders = f));
            void getScanStatus().then((st) => (lastScan = st.last));
          },
          failed: (e) => {
            scanning = false;
            cancelling = false;
            notify(e.message);
          },
        });
      } catch (e) {
        notify(errorMessage(e));
      }
    })();
  });
  onDestroy(() => unlisten?.());

  function destinationOf(m: MediaView) {
    return settings?.destination_root ? demoDestination(m, settings, settings.destination_root) : "";
  }

  const demoDuplicates = $derived.by(() => {
    const seen = new Map<string, number>();
    for (const m of demoMedia) if (m.sha256) seen.set(m.sha256, (seen.get(m.sha256) ?? 0) + 1);
    return new Set(demoMedia.filter((m) => m.sha256 && (seen.get(m.sha256) ?? 0) > 1).map((m) => m.id));
  });

  function percent(n: number) {
    return `${((n / Math.max(counts.all, 1)) * 100).toLocaleString("uk-UA", { maximumFractionDigits: 1 })}%`;
  }

  const title = $derived(section === "all" ? "Ваша медіатека" : t.sections[section]);
</script>

<div class="app-root">
  <Titlebar {demo} onToggleTheme={toggleTheme} onOpenSettings={() => (settingsOpen = true)} onAddFolders={addSources} />
  <div class="layout" class:no-inspector={section === "plan" || section === "history"}>
    <Sidebar {section} {counts} {folders} {scan} {lastScan} {scanning} {cancelling}
      onSelect={(s) => { section = s; selected = new Set(); }}
      onAddSources={addSources} onRemoveSource={removeSource} onChangeDestination={changeDestination} onCancelScan={stopScan} />

    <main class="main">
      <div class="main-header">
        <div>
          <div class="eyebrow"><span>{t.workspace}</span><span aria-hidden="true">›</span><span>{t.sections[section]}</span></div>
          <h1>{title}</h1>
          <div class="subtitle">{t.subtitles[section]}</div>
        </div>
        <div class="header-actions">
          {#if scanning}
            <button class="secondary" disabled={cancelling} onclick={stopScan}>■ {cancelling ? t.cancelling : t.cancel}</button>
          {:else}
            <button class="secondary" disabled={!demo && !hasSources} onclick={scanNow}>↻ {t.scan}</button>
          {/if}
          {#if section !== "plan"}<button class="primary" onclick={() => (section = "plan")}>{t.viewPlan} →</button>{/if}
        </div>
      </div>

      {#if section === "plan"}
        {#if demo && settings}
          <PlanPreview items={demoMedia} {settings} {destinationOf} duplicateIds={demoDuplicates} onBack={() => (section = "all")} />
        {:else}
          <div class="empty"><strong>{t.planLaterTitle}</strong>{t.planLaterHint}</div>
        {/if}
      {:else if section === "history"}
        <div class="empty"><strong>{t.history.empty}</strong>{t.history.emptyHint}</div>
      {:else if !demo && !hasSources}
        <div class="empty">
          <strong>{t.noSourcesTitle}</strong>{t.noSourcesHint}
          <div class="empty-action"><button class="primary" onclick={addSources}>＋ {t.addFolders}</button></div>
        </div>
      {:else if !demo && counts.all === 0 && !scanning}
        <div class="empty">
          <strong>{t.notScannedTitle}</strong>{t.notScannedHint}
          <div class="empty-action"><button class="primary" onclick={scanNow}>↻ {t.scan}</button></div>
        </div>
      {:else}
        <div class="stats-row">
          <div class="stat-chip"><div class="stat-symbol">▦</div><div><div class="stat-main">{formatCount(counts.all)}</div><div class="stat-label">{t.filesIndexed}</div></div></div>
          <div class="stat-chip"><div class="stat-symbol">◷</div><div><div class="stat-main">{percent(counts.with_date)}</div><div class="stat-label">{t.withDate}</div></div></div>
          <div class="stat-chip"><div class="stat-symbol">⌖</div><div><div class="stat-main">{percent(counts.with_gps)}</div><div class="stat-label">{t.withGps}</div></div></div>
        </div>
        <div class="source-bar">
          <div class="source-icon" aria-hidden="true">▣</div>
          <div class="source-details">
            <div class="source-label">{t.source}</div>
            <div class="source-path">{folders?.sources[0]?.path ?? t.notChosen}{#if (folders?.sources.length ?? 0) > 1}<span class="more-src"> +{(folders?.sources.length ?? 1) - 1}</span>{/if}</div>
          </div>
          <div class="source-arrow" aria-hidden="true">→</div>
          <div class="source-icon" aria-hidden="true">▤</div>
          <div class="source-details">
            <div class="source-label">{t.structure}</div>
            <div class="source-path">{t.year} / {t.month} ({settings ? monthFolder(settings.month_format, 8) : "…"}) / {t.place}</div>
          </div>
          <button class="icon-button" title={t.settings} aria-label={t.settings} onclick={() => (settingsOpen = true)}>⚙</button>
        </div>
        <Gallery {items} {total} {demo} {scanning} {selected} bind:kind bind:query bind:sort
          title={t.sections[librarySection]} onPick={pick} onMore={() => load(true)} />
      {/if}
    </main>

    {#if section !== "plan" && section !== "history"}
      <Inspector item={focused} {demo} count={selected.size} destination={focused ? destinationOf(focused) || null : null}
        onEdit={() => notify(t.editLater)} />
    {/if}
  </div>
  <footer class="footer">
    <div class="footer-left">
      <span><span class="ok-dot" aria-hidden="true">●</span> <strong>{scanning ? t.scanning : t.footerReady}</strong></span>
      <span>·</span>
      <span>{summary ? t.footerIndex(summary.media_count) : t.footerBrowser}</span>
    </div>
    <div class="footer-right">
      {#if summary}<span>{t.footerSqlite(summary.schema_version)}</span><span>·</span><span>{t.version(summary.app_version)}</span>{/if}
      {#if demo}<span class="demo-tag">BROWSER PREVIEW</span>{/if}
    </div>
  </footer>
</div>

{#if settingsOpen && settings}
  <SettingsDialog {settings} onClose={() => (settingsOpen = false)} onLibraryChanged={() => { void refreshCounts(); void load(); }}
    onSaved={(s) => { settings = s; applyTheme(s); settingsOpen = false; notify(t.settingsDialog.saved); void refreshCounts(); void load(); }} />
{/if}

<div class="toast" class:show={toast !== null} role="status" aria-live="polite">{toast ?? ""}</div>

<style>
  .app-root { height: 100vh; display: flex; flex-direction: column; }
  .layout { display: grid; grid-template-columns: 236px minmax(400px, 1fr) 306px; flex: 1; min-height: 0; }
  .layout.no-inspector { grid-template-columns: 236px minmax(400px, 1fr); }
  .main { min-width: 0; padding: 26px 24px 30px; overflow-y: auto; }
  .main-header { display: flex; align-items: flex-start; justify-content: space-between; gap: 18px; margin-bottom: 21px; }
  .eyebrow { color: var(--muted); font-size: 11px; margin-bottom: 7px; display: flex; gap: 7px; }
  h1 { margin: 0; font-size: clamp(22px, 2vw, 27px); font-weight: 730; letter-spacing: -.9px; line-height: 1.15; }
  .subtitle { color: var(--muted); margin-top: 8px; font-size: 12px; }
  .header-actions { display: flex; gap: 8px; }
  .stats-row { display: flex; gap: 9px; flex-wrap: wrap; margin-bottom: 19px; }
  .stat-chip { background: var(--surface); border: 1px solid var(--line); border-radius: 9px; display: flex; align-items: center; gap: 9px; min-height: 43px; padding: 7px 12px; }
  .stat-symbol { display: grid; place-items: center; width: 26px; height: 26px; border-radius: 7px; background: var(--surface-soft); color: var(--accent); font-size: 14px; }
  .stat-main { font-weight: 750; font-size: 13px; }
  .stat-label { color: var(--muted); margin-top: 3px; font-size: 10px; }
  .source-bar { display: flex; align-items: center; gap: 9px; border: 1px solid var(--line); background: var(--surface); border-radius: 10px; padding: 10px 12px; margin-bottom: 17px; }
  .source-bar .icon-button { margin-left: auto; flex: 0 0 32px; }
  .source-icon { flex: 0 0 27px; height: 27px; background: var(--surface-soft); border-radius: 7px; display: grid; place-items: center; color: var(--muted); }
  .source-details { min-width: 0; display: grid; gap: 3px; }
  .source-label { font-size: 10px; color: var(--muted); }
  .source-path { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; font-weight: 650; font-size: 11px; }
  .more-src { color: var(--muted); font-weight: 500; }
  .source-arrow { color: var(--muted); padding: 0 4px; }
  .empty { border: 1px dashed var(--line); border-radius: 12px; padding: 45px 20px; text-align: center; color: var(--muted); background: var(--surface); line-height: 1.5; }
  .empty strong { display: block; color: var(--text); margin-bottom: 7px; font-size: 14px; }
  .empty-action { margin-top: 16px; }
  .footer { height: 32px; flex: 0 0 32px; display: flex; justify-content: space-between; align-items: center; gap: 10px; padding: 0 18px; background: var(--surface); border-top: 1px solid var(--line); color: var(--muted); font-size: 10px; }
  .footer-left, .footer-right { display: flex; gap: 10px; align-items: center; }
  .footer strong { color: var(--accent-strong); }
  .ok-dot { color: var(--accent); }
  .toast { position: fixed; left: 50%; bottom: 47px; transform: translate(-50%, 16px); opacity: 0; pointer-events: none; transition: opacity .2s, transform .2s; z-index: 40; background: var(--text); color: var(--surface); border-radius: 10px; padding: 12px 16px; box-shadow: var(--shadow); font-size: 11px; font-weight: 650; max-width: calc(100vw - 40px); }
  .toast.show { opacity: 1; transform: translate(-50%, 0); }
  @media (max-width: 1250px) {
    .layout { grid-template-columns: 215px minmax(350px, 1fr) 280px; }
    .layout.no-inspector { grid-template-columns: 215px minmax(350px, 1fr); }
    .main { padding-left: 18px; padding-right: 18px; }
  }
</style>
