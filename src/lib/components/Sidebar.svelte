<script lang="ts">
  import { formatCount } from "$lib/format";
  import { t } from "$lib/i18n/uk";

  export type Section = "all" | "review" | "nodate" | "nogps" | "duplicates" | "plan" | "history";

  let { section, counts, source, destination, onSelect, onChangeFolders }: {
    section: Section;
    counts: Record<"all" | "review" | "nodate" | "nogps" | "duplicates", number>;
    source: string | null;
    destination: string | null;
    onSelect: (s: Section) => void;
    onChangeFolders: () => void;
  } = $props();

  const library: { id: Section & keyof typeof counts; icon: string }[] = [
    { id: "all", icon: "▦" },
    { id: "review", icon: "◷" },
    { id: "nodate", icon: "◌" },
    { id: "nogps", icon: "⌖" },
    { id: "duplicates", icon: "▧" },
  ];
</script>

<aside class="sidebar">
  <div class="workspace-label">{t.library}</div>
  <nav class="nav-list" aria-label={t.library}>
    {#each library as item (item.id)}
      <button
        class="nav-item"
        class:active={section === item.id}
        aria-current={section === item.id ? "page" : undefined}
        onclick={() => onSelect(item.id)}
      >
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
      <span class="nav-count">{formatCount(counts.all)}</span>
    </button>
    <button class="nav-item" class:active={section === "history"} aria-current={section === "history" ? "page" : undefined} onclick={() => onSelect("history")}>
      <span class="nav-icon" aria-hidden="true">↺</span><span class="nav-label">{t.sections.history}</span>
    </button>
  </nav>
  <div class="divider"></div>
  <div class="folder-card">
    <div class="folder-card-head">{t.folders}</div>
    <div class="folder-row">
      <span aria-hidden="true">↳</span>
      <div><div class="folder-label">{t.source}</div><div class="folder-path">{source ?? t.notChosen}</div></div>
    </div>
    <div class="folder-row">
      <span aria-hidden="true">↳</span>
      <div><div class="folder-label">{t.destination}</div><div class="folder-path">{destination ?? t.notChosen}</div></div>
    </div>
    <div class="folder-actions">
      <button class="tiny-button" onclick={onChangeFolders}>{t.changeSource}</button>
      <button class="tiny-button" onclick={onChangeFolders}>{t.changeDest}</button>
    </div>
  </div>
  <div class="sidebar-bottom">
    <div class="scan-line"><span>{t.indexTitle}</span><strong>—</strong></div>
    <div class="progress-track" role="progressbar" aria-label={t.indexTitle} aria-valuemin="0" aria-valuemax="100" aria-valuenow="0"><div class="progress-fill"></div></div>
    <div class="scan-caption">{t.indexEmpty}</div>
  </div>
</aside>

<style>
  .sidebar {
    background: var(--surface); border-right: 1px solid var(--line); padding: 18px 12px 14px;
    display: flex; flex-direction: column; min-width: 0; overflow-y: auto;
  }
  .workspace-label {
    padding: 0 10px 9px; font-size: 10px; color: var(--muted); font-weight: 750;
    letter-spacing: .12em; text-transform: uppercase;
  }
  .nav-list { display: flex; flex-direction: column; gap: 3px; }
  .nav-item {
    width: 100%; border: 0; background: transparent; border-radius: 9px; min-height: 39px;
    display: flex; gap: 10px; align-items: center; padding: 0 10px; color: var(--muted); text-align: left;
  }
  .nav-item:hover { background: var(--surface-soft); color: var(--text); }
  .nav-item.active { background: var(--accent-soft); color: var(--accent-strong); font-weight: 700; }
  .nav-icon { display: inline-grid; width: 18px; place-items: center; font-size: 16px; }
  .nav-label { flex: 1; }
  .nav-count {
    min-width: 23px; text-align: center; border-radius: 6px; padding: 3px 5px;
    background: var(--surface-soft); font-size: 10px; color: var(--muted);
  }
  .nav-item.active .nav-count { background: light-dark(#d7e5cf, #344b35); color: var(--accent-strong); }
  .divider { height: 1px; background: var(--line); margin: 17px 4px 15px; }
  .folder-card { margin: 0 3px; padding: 12px; border: 1px solid var(--line); background: var(--surface-raised); border-radius: 11px; }
  .folder-card-head { font-weight: 700; font-size: 11px; margin-bottom: 9px; }
  .folder-row { display: flex; gap: 8px; align-items: flex-start; color: var(--muted); font-size: 11px; line-height: 1.5; }
  .folder-row + .folder-row { margin-top: 8px; }
  .folder-label { font-size: 10px; margin-bottom: 3px; }
  .folder-path { overflow-wrap: anywhere; color: var(--text); font-size: 11px; }
  .folder-actions { display: flex; gap: 7px; margin-top: 12px; }
  .tiny-button {
    flex: 1; min-height: 28px; border-radius: 7px; border: 1px solid var(--line);
    background: var(--surface); color: var(--muted); font-size: 10px; font-weight: 650;
  }
  .tiny-button:hover { color: var(--text); background: var(--surface-soft); }
  .sidebar-bottom { margin-top: auto; padding: 16px 8px 2px; }
  .scan-line { display: flex; justify-content: space-between; color: var(--muted); font-size: 10px; margin-bottom: 8px; }
  .progress-track { height: 4px; border-radius: 5px; overflow: hidden; background: var(--line); }
  .progress-fill { height: 100%; width: 0; background: var(--accent); }
  .scan-caption { margin-top: 9px; font-size: 10px; color: var(--muted); }
</style>
