<script lang="ts" module>
  export type Section = "all" | "review" | "nodate" | "nogps" | "duplicates" | "plan" | "history";
</script>

<script lang="ts">
  import { formatCount } from "$lib/format";
  import { t } from "$lib/i18n/uk";
  import type { Folders, LastScan, ScanProgress, SectionCounts } from "$lib/types";

  let { section, counts, folders, scan, lastScan, scanning, cancelling, onSelect, onAddSources, onRemoveSource, onChangeDestination, onCancelScan, onReport }: {
    section: Section;
    counts: SectionCounts;
    folders: Folders | null;
    scan: ScanProgress | null;
    lastScan: LastScan | null;
    scanning: boolean;
    cancelling: boolean;
    onSelect: (s: Section) => void;
    onAddSources: () => void;
    onRemoveSource: (id: number) => void;
    onChangeDestination: () => void;
    onCancelScan: () => void;
    onReport: () => void;
  } = $props();

  const library: { id: "all" | "review" | "nodate" | "nogps" | "duplicates"; icon: string }[] = [
    { id: "all", icon: "▦" },
    { id: "review", icon: "◷" },
    { id: "nodate", icon: "◌" },
    { id: "nogps", icon: "⌖" },
    { id: "duplicates", icon: "▧" },
  ];

  /** `…/Photos/2025` for long paths; the full path is in the tooltip. */
  function shortPath(path: string): string {
    const parts = path.split(/[\\/]/).filter(Boolean);
    return parts.length > 3 ? `…/${parts.slice(-2).join("/")}` : path;
  }

  const when = (ms: number) =>
    new Date(ms).toLocaleString("uk-UA", { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" });

  const percent = $derived.by(() => {
    if (!scan) return lastScan?.state === "done" ? 100 : 0;
    if (scan.phase === "done") return 100;
    const total = scan.processed + scan.pending;
    return (scan.phase === "reading" || scan.phase === "cancelled") && total > 0 ? Math.round((scan.processed / total) * 100) : 0;
  });
</script>

<aside class="sidebar">
  <div class="workspace-label">{t.library}</div>
  <nav class="nav-list" aria-label={t.library}>
    {#each library as item (item.id)}
      <button class="nav-item" class:active={section === item.id} aria-current={section === item.id ? "page" : undefined} onclick={() => onSelect(item.id)}>
        <span class="nav-icon" aria-hidden="true">{item.icon}</span>
        <span class="nav-label">{t.sections[item.id]}</span>
        <span class="nav-count">{formatCount(counts[item.id])}</span>
      </button>
    {/each}
  </nav>
  <div class="divider"></div>
  <div class="workspace-label">{t.workflow}</div>
  <nav class="nav-list" aria-label={t.workflow}>
    <button class="nav-item" class:active={section === "plan"} aria-current={section === "plan" ? "page" : undefined} onclick={() => onSelect("plan")}>
      <span class="nav-icon" aria-hidden="true">⇢</span><span class="nav-label">{t.sections.plan}</span>
    </button>
    <button class="nav-item" class:active={section === "history"} aria-current={section === "history" ? "page" : undefined} onclick={() => onSelect("history")}>
      <span class="nav-icon" aria-hidden="true">↺</span><span class="nav-label">{t.sections.history}</span>
    </button>
  </nav>
  <div class="divider"></div>
  <div class="folder-card">
    <div class="folder-card-head">{t.sources}</div>
    {#if folders && folders.sources.length}
      <ul class="sources">
        {#each folders.sources as root (root.id)}
          <li class="folder-row">
            <span class="folder-path" class:unavailable={!root.is_available} title={root.is_available ? root.path : `${t.sourceUnavailable}: ${root.path}`}>
              {#if !root.is_available}<span aria-hidden="true">⚠ </span>{/if}{shortPath(root.path)}
            </span>
            <button class="remove" aria-label={t.removeSource(root.path)} title={t.removeSource(root.path)} disabled={scanning} onclick={() => onRemoveSource(root.id)}>✕</button>
          </li>
        {/each}
      </ul>
    {:else}
      <div class="folder-empty">{t.notChosen}</div>
    {/if}
    <div class="folder-label">{t.destination}</div>
    <div class="folder-path dest" title={folders?.destination ?? ""}>{folders?.destination ? shortPath(folders.destination) : t.notChosen}</div>
    {#if folders?.destination_inside_source}<div class="warn">⚠ {t.destInsideSource}</div>{/if}
    <div class="folder-actions">
      <button class="tiny-button" onclick={onAddSources}>＋ {t.addSource}</button>
      <button class="tiny-button" onclick={onChangeDestination}>{folders?.destination ? t.changeDest : t.chooseDestination}</button>
    </div>
  </div>
  <div class="sidebar-bottom">
    <div class="scan-line">
      <span>{scanning ? (scan ? t.scanPhase[scan.phase] : t.scanning) : t.indexTitle}</span>
      <strong>{scan || lastScan ? `${percent}%` : "—"}</strong>
    </div>
    <div class="progress-track" role="progressbar" aria-label={t.indexTitle} aria-valuemin="0" aria-valuemax="100" aria-valuenow={percent}>
      <div class="progress-fill" class:indeterminate={scanning && scan?.phase === "walking"} style:width="{percent}%"></div>
    </div>
    <div class="scan-caption">
      {#if scan}{t.scanCounts(scan)}{:else if lastScan}{t.lastScan(when(lastScan.finished_at), lastScan.state)} · {t.scanCounts(lastScan)}{:else}{t.indexEmpty}{/if}
    </div>
    {#if !scanning && (scan || lastScan)}
      <button class="report-link" onclick={onReport}>{t.report.open} →</button>
    {/if}
    {#if scanning}
      <div class="scan-current" title={scan?.current ?? ""}>{scan?.current?.split(/[\\/]/).pop() ?? ""}</div>
      <button class="tiny-button cancel" disabled={cancelling} onclick={onCancelScan}>{cancelling ? t.cancelling : t.cancel}</button>
    {/if}
  </div>
</aside>

<style>
  .sidebar { background: var(--surface); border-right: 1px solid var(--line); padding: 18px 12px 14px; display: flex; flex-direction: column; min-width: 0; overflow-y: auto; }
  .workspace-label { padding: 0 10px 9px; font-size: 10px; color: var(--muted); font-weight: 750; letter-spacing: .12em; text-transform: uppercase; }
  .nav-list { display: flex; flex-direction: column; gap: 3px; }
  .nav-item { width: 100%; border: 0; background: transparent; border-radius: 9px; min-height: 39px; display: flex; gap: 10px; align-items: center; padding: 0 10px; color: var(--muted); text-align: left; }
  .nav-item:hover { background: var(--surface-soft); color: var(--text); }
  .nav-item.active { background: var(--accent-soft); color: var(--accent-strong); font-weight: 700; }
  .nav-icon { display: inline-grid; width: 18px; place-items: center; font-size: 16px; }
  .nav-label { flex: 1; }
  .nav-count { min-width: 23px; text-align: center; border-radius: 6px; padding: 3px 5px; background: var(--surface-soft); font-size: 10px; color: var(--muted); }
  .nav-item.active .nav-count { background: light-dark(#d7e5cf, #344b35); color: var(--accent-strong); }
  .divider { height: 1px; background: var(--line); margin: 17px 4px 15px; }
  .folder-card { margin: 0 3px; padding: 12px; border: 1px solid var(--line); background: var(--surface-raised); border-radius: 11px; }
  .folder-card-head { font-weight: 700; font-size: 11px; margin-bottom: 8px; }
  .sources { list-style: none; margin: 0 0 10px; padding: 0; display: grid; gap: 6px; max-height: 132px; overflow-y: auto; }
  .folder-row { display: flex; gap: 6px; align-items: flex-start; }
  .folder-path { flex: 1; overflow-wrap: anywhere; font-size: 11px; line-height: 1.45; }
  .folder-path.unavailable { color: var(--amber); }
  .folder-path.dest { margin-bottom: 4px; }
  .folder-empty { color: var(--muted); font-size: 11px; margin-bottom: 10px; }
  .folder-label { font-size: 10px; color: var(--muted); margin-bottom: 3px; }
  .remove { border: 0; background: transparent; color: var(--muted); font-size: 11px; padding: 1px 3px; border-radius: 4px; }
  .remove:hover:not(:disabled) { color: var(--red); background: var(--red-soft); }
  .warn { font-size: 10px; color: var(--amber); background: var(--amber-soft); border-radius: 6px; padding: 6px 7px; margin-top: 6px; line-height: 1.4; }
  .folder-actions { display: flex; gap: 7px; margin-top: 12px; }
  .tiny-button { flex: 1; min-height: 28px; border-radius: 7px; border: 1px solid var(--line); background: var(--surface); color: var(--muted); font-size: 10px; font-weight: 650; padding: 0 6px; }
  .tiny-button:hover:not(:disabled) { color: var(--text); background: var(--surface-soft); }
  .sidebar-bottom { margin-top: auto; padding: 16px 8px 2px; }
  .scan-line { display: flex; justify-content: space-between; color: var(--muted); font-size: 10px; margin-bottom: 8px; }
  .scan-line strong { color: var(--accent-strong); }
  .progress-track { height: 4px; border-radius: 5px; overflow: hidden; background: var(--line); }
  .progress-fill { height: 100%; background: var(--accent); transition: width .2s; }
  .progress-fill.indeterminate { width: 30% !important; animation: slide 1.1s ease-in-out infinite; }
  @keyframes slide { from { transform: translateX(-100%); } to { transform: translateX(340%); } }
  @media (prefers-reduced-motion: reduce) { .progress-fill.indeterminate { animation: none; } }
  .scan-caption { margin-top: 9px; font-size: 10px; color: var(--muted); line-height: 1.4; }
  .scan-current { margin-top: 5px; font-size: 9px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .cancel { margin-top: 9px; width: 100%; }
  .report-link { margin-top: 7px; border: 0; background: transparent; padding: 0; color: var(--accent-strong); font-size: 10px; font-weight: 700; }
</style>
