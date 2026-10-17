<script lang="ts">
  import { errorMessage, getLibrarySummary, getSettings, inTauri, updateSettings } from "$lib/api";
  import Gallery, { type KindFilter, type SortKey } from "$lib/components/Gallery.svelte";
  import Inspector from "$lib/components/Inspector.svelte";
  import PlanPreview from "$lib/components/PlanPreview.svelte";
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import Sidebar, { type Section } from "$lib/components/Sidebar.svelte";
  import Titlebar from "$lib/components/Titlebar.svelte";
  import { DEMO_DESTINATION, DEMO_SOURCE, demoMedia } from "$lib/demo";
  import { demoDestination, formatCount, formatShortDate, monthFolder } from "$lib/format";
  import { t } from "$lib/i18n/uk";
  import type { AppSettings, LibrarySummary, MediaView } from "$lib/types";

  let settings: AppSettings | null = $state(null);
  let summary: LibrarySummary | null = $state(null);
  let section = $state<Section>("all");
  let kind = $state<KindFilter>("all");
  let query = $state("");
  let sort = $state<SortKey>("date");
  let selected = $state(new Set<number>([1]));
  let settingsOpen = $state(false);
  let toast: string | null = $state(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  const media = demoMedia;

  const duplicateIds = $derived.by(() => {
    const byHash = new Map<string, number[]>();
    for (const m of media) if (m.sha256) byHash.set(m.sha256, [...(byHash.get(m.sha256) ?? []), m.id]);
    return new Set([...byHash.values()].filter((ids) => ids.length > 1).flat());
  });

  const inSection: Record<"all" | "review" | "nodate" | "nogps" | "duplicates", (m: MediaView) => boolean> = {
    all: () => true,
    review: (m) => (m.captured !== null && m.captured.status !== "confirmed") || m.place_status === "gps_no_place",
    nodate: (m) => m.captured === null,
    nogps: (m) => m.place_status !== "resolved",
    duplicates: (m) => duplicateIds.has(m.id),
  };

  const counts = $derived({
    all: media.length,
    review: media.filter(inSection.review).length,
    nodate: media.filter(inSection.nodate).length,
    nogps: media.filter(inSection.nogps).length,
    duplicates: media.filter(inSection.duplicates).length,
  });

  const librarySection = $derived(section === "plan" || section === "history" ? "all" : section);
  const sectionItems = $derived(media.filter(inSection[librarySection]));

  const visible = $derived.by(() => {
    const q = query.trim().toLocaleLowerCase("uk");
    const list = sectionItems.filter((m) => {
      if (kind === "photo" && m.kind !== "photo" && m.kind !== "raw_photo") return false;
      if (kind === "video" && m.kind !== "video") return false;
      if (!q) return true;
      const date = m.captured ? formatShortDate(m.captured.local) : "";
      return [m.file_name, m.path, m.place_label ?? "", date].some((s) => s.toLocaleLowerCase("uk").includes(q));
    });
    const byDate = (m: MediaView) => m.captured?.local ?? "";
    return list.toSorted((a, b) =>
      sort === "name" ? a.file_name.localeCompare(b.file_name, "uk")
      : sort === "size" ? b.size_bytes - a.size_bytes
      : byDate(b).localeCompare(byDate(a)),
    );
  });

  const focused = $derived(selected.size === 1 ? (media.find((m) => selected.has(m.id)) ?? null) : null);
  const withDate = $derived(media.filter((m) => m.captured && m.captured.status !== "file_system_fallback").length);
  const withGps = $derived(media.filter((m) => m.gps).length);

  function percent(n: number) {
    return `${((n / Math.max(media.length, 1)) * 100).toLocaleString("uk-UA", { maximumFractionDigits: 1 })}%`;
  }

  function destinationOf(m: MediaView) {
    return settings ? demoDestination(m, settings, DEMO_DESTINATION) : "";
  }

  function applyTheme(s: AppSettings) {
    const root = document.documentElement;
    if (s.theme === "system") delete root.dataset.theme;
    else root.dataset.theme = s.theme;
  }

  function notify(text: string) {
    toast = text;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), 3200);
  }

  function pick(id: number, additive: boolean) {
    const next = new Set(additive ? selected : []);
    if (additive && next.has(id)) next.delete(id);
    else next.add(id);
    selected = next;
  }

  async function toggleTheme() {
    if (!settings) return;
    const dark = settings.theme === "dark"
      || (settings.theme === "system" && matchMedia("(prefers-color-scheme: dark)").matches);
    try {
      settings = await updateSettings({ ...settings, theme: dark ? "light" : "dark" });
      applyTheme(settings);
    } catch (e) {
      notify(errorMessage(e));
    }
  }

  $effect(() => {
    void (async () => {
      try {
        settings = await getSettings();
        applyTheme(settings);
        summary = await getLibrarySummary();
      } catch (e) {
        notify(errorMessage(e));
      }
    })();
  });

  const title = $derived(section === "all" ? "Ваша медіатека" : t.sections[section]);
</script>

<div class="app-root">
  <Titlebar onToggleTheme={toggleTheme} onOpenSettings={() => (settingsOpen = true)} onAddFolders={() => notify(t.foldersLater)} />
  <div class="layout" class:no-inspector={section === "plan" || section === "history"}>
    <Sidebar {section} {counts} source={DEMO_SOURCE} destination={DEMO_DESTINATION}
      onSelect={(s) => (section = s)} onChangeFolders={() => notify(t.foldersLater)} />

    <main class="main">
      <div class="main-header">
        <div>
          <div class="eyebrow"><span>{t.workspace}</span><span aria-hidden="true">›</span><span>{t.sections[section]}</span></div>
          <h1>{title}</h1>
          <div class="subtitle">{t.subtitles[section]}</div>
        </div>
        <div class="header-actions">
          <button class="secondary" onclick={() => notify(t.scanLater)}>↻ {t.scan}</button>
          {#if section !== "plan"}<button class="primary" onclick={() => (section = "plan")}>{t.viewPlan} →</button>{/if}
        </div>
      </div>

      {#if section === "plan"}
        {#if settings}<PlanPreview items={media} {settings} {destinationOf} {duplicateIds} onBack={() => (section = "all")} />{/if}
      {:else if section === "history"}
        <div class="empty"><strong>{t.history.empty}</strong>{t.history.emptyHint}</div>
      {:else}
        <div class="stats-row">
          <div class="stat-chip"><div class="stat-symbol">▦</div><div><div class="stat-main">{formatCount(media.length)}</div><div class="stat-label">{t.filesIndexed}</div></div></div>
          <div class="stat-chip"><div class="stat-symbol">◷</div><div><div class="stat-main">{percent(withDate)}</div><div class="stat-label">{t.withDate}</div></div></div>
          <div class="stat-chip"><div class="stat-symbol">⌖</div><div><div class="stat-main">{percent(withGps)}</div><div class="stat-label">{t.withGps}</div></div></div>
        </div>
        <div class="source-bar">
          <div class="source-icon" aria-hidden="true">▣</div>
          <div class="source-details"><div class="source-label">{t.source}</div><div class="source-path">{DEMO_SOURCE}</div></div>
          <div class="source-arrow" aria-hidden="true">→</div>
          <div class="source-icon" aria-hidden="true">▤</div>
          <div class="source-details">
            <div class="source-label">{t.structure}</div>
            <div class="source-path">{t.year} / {t.month} ({settings ? monthFolder(settings.month_format, 8) : "…"}) / {t.place}</div>
          </div>
          <button class="icon-button" title={t.settings} aria-label={t.settings} onclick={() => (settingsOpen = true)}>⚙</button>
        </div>
        <Gallery items={visible} total={sectionItems.length} {selected} bind:kind bind:query bind:sort
          title={t.sections[librarySection]} onPick={pick} />
      {/if}
    </main>

    {#if section !== "plan" && section !== "history"}
      <Inspector item={focused} count={selected.size} destination={focused ? destinationOf(focused) : null}
        onEdit={() => notify(t.editLater)} />
    {/if}
  </div>
  <footer class="footer">
    <div class="footer-left">
      <span><span class="ok-dot" aria-hidden="true">●</span> <strong>{t.footerReady}</strong></span>
      <span>·</span>
      <span>{summary ? t.footerIndex(summary.media_count) : t.footerBrowser}</span>
    </div>
    <div class="footer-right">
      {#if summary}<span>{t.footerSqlite(summary.schema_version)}</span><span>·</span><span>{t.version(summary.app_version)}</span>{/if}
      {#if !inTauri}<span class="demo-tag">BROWSER PREVIEW</span>{/if}
    </div>
  </footer>
</div>

{#if settingsOpen && settings}
  <SettingsDialog {settings} onClose={() => (settingsOpen = false)}
    onSaved={(s) => { settings = s; applyTheme(s); settingsOpen = false; notify(t.settingsDialog.saved); }} />
{/if}

<div class="toast" class:show={toast !== null} role="status" aria-live="polite">{toast ?? ""}</div>

<style>
  .app-root { height: 100vh; display: flex; flex-direction: column; }
  .layout { display: grid; grid-template-columns: 224px minmax(400px, 1fr) 306px; flex: 1; min-height: 0; }
  .layout.no-inspector { grid-template-columns: 224px minmax(400px, 1fr); }
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
  .source-arrow { color: var(--muted); padding: 0 4px; }
  .empty { border: 1px dashed var(--line); border-radius: 12px; padding: 45px 20px; text-align: center; color: var(--muted); background: var(--surface); }
  .empty strong { display: block; color: var(--text); margin-bottom: 7px; font-size: 14px; }
  .footer {
    height: 32px; flex: 0 0 32px; display: flex; justify-content: space-between; align-items: center; gap: 10px;
    padding: 0 18px; background: var(--surface); border-top: 1px solid var(--line); color: var(--muted); font-size: 10px;
  }
  .footer-left, .footer-right { display: flex; gap: 10px; align-items: center; }
  .footer strong { color: var(--accent-strong); }
  .ok-dot { color: var(--accent); }
  .toast {
    position: fixed; left: 50%; bottom: 47px; transform: translate(-50%, 16px); opacity: 0; pointer-events: none;
    transition: opacity .2s, transform .2s; z-index: 40; background: var(--text); color: var(--surface);
    border-radius: 10px; padding: 12px 16px; box-shadow: var(--shadow); font-size: 11px; font-weight: 650;
  }
  .toast.show { opacity: 1; transform: translate(-50%, 0); }
  /* Below 1250 px the inspector narrows; the window minimum is 1100 px. */
  @media (max-width: 1250px) {
    .layout { grid-template-columns: 205px minmax(350px, 1fr) 280px; }
    .layout.no-inspector { grid-template-columns: 205px minmax(350px, 1fr); }
    .main { padding-left: 18px; padding-right: 18px; }
  }
</style>
