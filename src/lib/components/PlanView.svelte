<script lang="ts">
  import {
    applySortPlan, buildSortPlan, cancelRun, errorMessage, getPlanItems, getPlanSummary, setPlanIncluded, setPlanMode,
  } from "$lib/api";
  import { formatCount, formatShortDate, formatSize, monthFolder } from "$lib/format";
  import { noteText, t } from "$lib/i18n";
  import type { AppSettings, OperationMode, PlanFilter, PlanRow, PlanSummary, RunProgress } from "$lib/types";

  let { settings, run, running, onChooseDestination, onMessage, onOpenLog }: {
    settings: AppSettings | null;
    run: RunProgress | null;
    running: boolean;
    onChooseDestination: () => void;
    onMessage: (text: string) => void;
    onOpenLog: () => void;
  } = $props();

  const PAGE = 200;
  let summary = $state<PlanSummary | null>(null);
  let rows = $state<PlanRow[]>([]);
  let total = $state(0);
  let filter = $state<PlanFilter>("all");
  let search = $state("");
  let mode = $state<OperationMode>("copy");
  let building = $state(false);
  let confirmOpen = $state(false);
  let moveAck = $state(false);
  let stopping = $state(false);
  let dialog: HTMLDialogElement | undefined = $state();

  const root = $derived(settings?.destination_root ?? null);
  const isDraft = $derived(summary?.state === "draft");
  const canContinue = $derived(summary?.state === "partial");
  const verb = $derived(mode === "move" ? t.planView.move : t.planView.copy);

  async function loadRows(append = false) {
    if (!summary) return;
    try {
      const page = await getPlanItems(summary.id, filter, search, append ? rows.length : 0, PAGE);
      rows = append ? [...rows, ...page.items] : page.items;
      total = page.total;
    } catch (e) {
      onMessage(errorMessage(e));
    }
  }

  async function refresh() {
    try {
      summary = await getPlanSummary();
      if (summary) mode = summary.mode;
      await loadRows();
    } catch (e) {
      onMessage(errorMessage(e));
    }
  }

  // Load the latest plan when the screen opens, and after a run ends.
  $effect(() => {
    void running;
    void refresh();
  });

  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    void filter;
    const q = search;
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => { void q; void loadRows(); }, q ? 200 : 0);
  });

  async function build() {
    building = true;
    try {
      summary = await buildSortPlan(mode);
      filter = "all";
      await loadRows();
    } catch (e) {
      onMessage(errorMessage(e));
    } finally {
      building = false;
    }
  }

  async function changeMode(next: OperationMode) {
    mode = next;
    if (summary && isDraft) {
      try {
        summary = (await setPlanMode(summary.id, next)) ?? summary;
      } catch (e) {
        onMessage(errorMessage(e));
      }
    }
  }

  async function toggle(row: PlanRow) {
    if (!summary || !isDraft) return;
    try {
      summary = (await setPlanIncluded(summary.id, [row.id], !row.included)) ?? summary;
      rows = rows.map((r) => (r.id === row.id ? { ...r, included: !r.included } : r));
    } catch (e) {
      onMessage(errorMessage(e));
    }
  }

  function openConfirm() {
    moveAck = false;
    confirmOpen = true;
    queueMicrotask(() => dialog?.showModal());
  }

  async function start() {
    if (!summary) return;
    dialog?.close();
    stopping = false;
    try {
      await applySortPlan(summary.id, mode, summary.included_files);
    } catch (e) {
      onMessage(errorMessage(e));
    }
  }

  async function stop() {
    stopping = true;
    await cancelRun().catch((e) => onMessage(errorMessage(e)));
  }

  function relative(dest: string): string {
    if (root && dest.startsWith(root)) return dest.slice(root.length).replace(/^[\\/]/, "");
    return dest;
  }

  function name(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  function folder(path: string): string {
    const parts = path.split(/[\\/]/);
    parts.pop();
    return parts.slice(-2).join("/");
  }

  const example = $derived(
    settings ? `2025 / ${monthFolder(settings.month_format, 8, settings.folder_language ?? undefined)} / ${t.place}` : "",
  );
  const percent = $derived(run && run.bytes_total > 0 ? Math.round((run.bytes_done / run.bytes_total) * 100) : 0);
</script>

{#if !root}
  <div class="empty">
    <strong>{t.planView.noDestinationTitle}</strong>{t.planView.noDestinationHint}
    <div class="empty-action"><button class="primary" onclick={onChooseDestination}>{t.planView.chooseDestination}</button></div>
  </div>
{:else}
  <div class="setup">
    <div class="setup-row">
      <div class="field">
        <div class="label">{t.planView.destination}</div>
        <div class="value" title={root}>{root}</div>
      </div>
      <button class="secondary small" disabled={running} onclick={onChooseDestination}>{t.changeDest}</button>
    </div>
    <div class="setup-row">
      <div class="field">
        <div class="label">{t.planView.structure}</div>
        <div class="value">{example}</div>
      </div>
    </div>
    <div class="setup-row mode" role="radiogroup" aria-label={t.planView.mode}>
      <div class="label">{t.planView.mode}</div>
      <div class="mode-buttons">
        <button class="mode-btn" class:active={mode === "copy"} role="radio" aria-checked={mode === "copy"} disabled={running || canContinue} onclick={() => changeMode("copy")}>
          ⧉ {t.planView.copy}
        </button>
        <button class="mode-btn move" class:active={mode === "move"} role="radio" aria-checked={mode === "move"} disabled={running || canContinue} onclick={() => changeMode("move")}>
          ↗ {t.planView.move}
        </button>
      </div>
      <div class="mode-hint" class:warn={mode === "move"}>{mode === "move" ? t.planView.moveHint : t.planView.copyHint}</div>
    </div>
    <div class="setup-actions">
      <button class="primary" disabled={building || running} onclick={build}>
        {building ? t.planView.building : summary ? t.planView.rebuild : t.planView.build}
      </button>
    </div>
  </div>

  {#if !summary}
    <div class="empty"><strong>{t.planView.noPlanTitle}</strong>{t.planView.noPlanHint}</div>
  {:else}
    <div class="cards">
      <div class="card main">
        <div class="num">{formatCount(summary.included_files)}</div>
        <div class="lbl">{isDraft ? (mode === "move" ? t.planView.willMove : t.planView.willCopy) : (mode === "move" ? t.planView.moved : t.planView.copied)} · {formatSize(summary.included_bytes)}</div>
      </div>
      {#each [
        [summary.conflicts, t.planView.conflicts, "warn"],
        [summary.duplicates, t.planView.duplicates, "warn"],
        [summary.already_archived, t.planView.archived, ""],
        [summary.already_in_place, t.planView.inPlace, ""],
        [summary.no_date, t.planView.noDate, "warn"],
        [summary.no_place, t.planView.noPlace, ""],
      ] as [n, label, tone] (label)}
        {#if Number(n) > 0}
          <div class="card" class:warn={tone === "warn"}><div class="num">{formatCount(Number(n))}</div><div class="lbl">{label}</div></div>
        {/if}
      {/each}
    </div>

    {#if isDraft}<div class="note" role="note">ⓘ {t.planView.safeNote}</div>{/if}

    {#if running && run}
      <div class="progress-box" role="status">
        <div class="progress-head">
          <strong>{run.phase === "undoing" ? t.runView.undoing : t.runView.running}</strong>
          <span>{t.runView.progress(run.done + run.failed + run.skipped, run.total)} · {formatSize(run.bytes_done)} / {formatSize(run.bytes_total)}</span>
        </div>
        <div class="bar"><div class="fill" style:width="{percent}%"></div></div>
        <div class="progress-foot">
          <span class="current">{run.current ?? ""}</span>
          <button class="secondary small" disabled={stopping} onclick={stop}>{stopping ? t.runView.stopping : t.runView.stop}</button>
        </div>
      </div>
    {/if}

    <div class="controls">
      <div class="pills" role="group">
        {#each Object.entries(t.planView.filters) as [value, label] (value)}
          <button class="pill" class:active={filter === value} aria-pressed={filter === value} onclick={() => (filter = value as PlanFilter)}>{label}</button>
        {/each}
      </div>
      <input class="search" type="search" placeholder={t.planView.search} bind:value={search} />
    </div>

    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th class="c-inc"><span class="visually-hidden">{t.planView.colInclude}</span></th>
            <th>{t.planView.colFile}</th><th>{t.planView.colDate}</th><th>{t.planView.colPlace}</th>
            <th>{t.planView.colDest}</th><th>{t.planView.colStatus}</th>
          </tr>
        </thead>
        <tbody>
          {#each rows as row (row.id)}
            <tr class:off={!row.included}>
              <td class="c-inc">
                <input type="checkbox" checked={row.included} aria-label={t.planView.colInclude}
                  disabled={!isDraft || row.status === "already_archived" || row.status === "already_in_place"}
                  onchange={() => toggle(row)} />
              </td>
              <td>
                <div class="fname">{name(row.source)}</div>
                <div class="sub" title={row.source}>{folder(row.source)}</div>
                {#if row.companions.length}
                  <div class="sub">{t.planView.with} {row.companions.map((c) => c.file_name).join(", ")}</div>
                {/if}
              </td>
              <td>{row.captured_local ? formatShortDate(row.captured_local) : "—"}</td>
              <td>{row.place ?? "—"}</td>
              <td class="dest" title={row.destination}>{relative(row.destination)}</td>
              <td>
                {#if !isDraft && t.planView.itemStatus[row.item_status]}
                  <span class="chip" class:error={row.item_status === "failed"} class:review={row.item_status === "interrupted"}>
                    {t.planView.itemStatus[row.item_status]}
                  </span>
                {:else}
                  <span class="chip" class:review={row.status === "conflict" || row.status === "duplicate"} class:missing={row.status === "already_archived" || row.status === "already_in_place"}>
                    {t.planView.status[row.status]}
                  </span>
                {/if}
                {#each row.notes as n, i (i)}
                  {#if !["plan_no_date", "plan_no_place"].includes(n.code)}<div class="nnote">{noteText(n)}</div>{/if}
                {/each}
              </td>
            </tr>
          {:else}
            <tr><td colspan="6" class="none">{t.nothingFound}</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
    {#if rows.length < total}
      <div class="more"><button class="secondary" onclick={() => loadRows(true)}>{t.planView.more(total - rows.length)}</button></div>
    {/if}

    <div class="runbar">
      {#if !isDraft && !canContinue}
        <button class="secondary" onclick={onOpenLog}>{t.log.title} →</button>
      {:else}
        <span class="runinfo">{t.planView.filesAndSize(formatCount(summary.included_files), formatSize(summary.included_bytes))}</span>
        <button class="primary big" class:danger={mode === "move"} disabled={running || building || (isDraft && summary.included_files === 0)} onclick={canContinue ? start : openConfirm}>
          {canContinue ? t.planView.continue : t.planView.run(verb)}
        </button>
      {/if}
    </div>
  {/if}
{/if}

{#if confirmOpen && summary}
  <dialog bind:this={dialog} onclose={() => (confirmOpen = false)} aria-labelledby="confirm-title">
    <div class="dcard">
      <div class="dtitle" id="confirm-title">{t.confirm.title}</div>
      <p class="dtext">
        {(mode === "move" ? t.confirm.moveText : t.confirm.copyText)(formatCount(summary.included_files), formatSize(summary.included_bytes), summary.destination_root)}
      </p>
      <p class="dhint" class:warn={mode === "move"}>{mode === "move" ? t.confirm.moveWarn : t.confirm.copySafe}</p>
      {#if mode === "move"}
        <label class="ack"><input type="checkbox" bind:checked={moveAck} /> {t.confirm.moveAck}</label>
      {/if}
      <div class="dactions">
        <button class="secondary" onclick={() => dialog?.close()}>{t.confirm.cancel}</button>
        <button class="primary" class:danger={mode === "move"} disabled={mode === "move" && !moveAck} onclick={start}>{t.confirm.start}</button>
      </div>
    </div>
  </dialog>
{/if}

<style>
  .empty { border: 1px dashed var(--line); border-radius: 12px; padding: 40px 20px; text-align: center; color: var(--muted); background: var(--surface); line-height: 1.5; margin-bottom: 14px; }
  .empty strong { display: block; color: var(--text); margin-bottom: 7px; font-size: 14px; }
  .empty-action { margin-top: 16px; }
  .setup { display: grid; gap: 10px; background: var(--surface); border: 1px solid var(--line); border-radius: 12px; padding: 14px 16px; margin-bottom: 14px; }
  .setup-row { display: flex; align-items: center; gap: 12px; min-width: 0; }
  .field { min-width: 0; flex: 1; }
  .label { font-size: 10px; color: var(--muted); font-weight: 650; margin-bottom: 3px; }
  .value { font-size: 12px; font-weight: 650; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .mode { flex-wrap: wrap; }
  .mode .label { width: 100%; margin: 0; }
  .mode-buttons { display: flex; gap: 6px; }
  .mode-btn { border: 1px solid var(--line); border-radius: 8px; padding: 8px 12px; background: var(--surface-raised); color: var(--muted); font-size: 12px; font-weight: 700; }
  .mode-btn.active { background: var(--accent-soft); border-color: var(--accent); color: var(--accent-strong); }
  .mode-btn.move.active { background: var(--amber-soft); border-color: var(--amber); color: var(--amber); }
  .mode-hint { font-size: 11px; color: var(--muted); }
  .mode-hint.warn { color: var(--amber); }
  .setup-actions { display: flex; justify-content: flex-end; }
  .small { min-height: 30px; font-size: 11px; }
  .cards { display: flex; flex-wrap: wrap; gap: 10px; margin-bottom: 12px; }
  .card { border: 1px solid var(--line); background: var(--surface); border-radius: 10px; padding: 11px 14px; min-width: 120px; }
  .card.main { border-color: var(--accent); background: var(--accent-soft); }
  .card.warn .num { color: var(--amber); }
  .num { font-size: 19px; font-weight: 760; }
  .lbl { color: var(--muted); font-size: 10px; margin-top: 4px; }
  .note { padding: 10px 12px; border-radius: 10px; background: var(--surface-soft); color: var(--muted); font-size: 11px; line-height: 1.5; margin-bottom: 12px; }
  .progress-box { border: 1px solid var(--accent); background: var(--surface); border-radius: 12px; padding: 12px 14px; margin-bottom: 12px; }
  .progress-head, .progress-foot { display: flex; justify-content: space-between; gap: 10px; align-items: center; font-size: 12px; }
  .progress-head span { color: var(--muted); font-size: 11px; }
  .bar { height: 6px; border-radius: 4px; background: var(--line); overflow: hidden; margin: 9px 0; }
  .fill { height: 100%; background: var(--accent); transition: width .2s; }
  .current { font-size: 10px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .controls { display: flex; justify-content: space-between; gap: 10px; flex-wrap: wrap; margin-bottom: 10px; }
  .pills { display: flex; padding: 3px; gap: 2px; border: 1px solid var(--line); background: var(--surface); border-radius: 9px; }
  .pill { border: 0; border-radius: 6px; min-height: 28px; padding: 0 10px; background: transparent; color: var(--muted); font-size: 11px; font-weight: 600; }
  .pill.active { color: var(--text); background: var(--surface-soft); }
  .search { width: 220px; height: 34px; padding: 0 10px; border-radius: 9px; border: 1px solid var(--line); background: var(--surface); }
  .table-wrap { background: var(--surface); border: 1px solid var(--line); border-radius: 11px; overflow-x: auto; }
  table { width: 100%; border-collapse: collapse; min-width: 760px; }
  th { text-align: left; padding: 9px 10px; color: var(--muted); background: var(--surface-soft); text-transform: uppercase; letter-spacing: .06em; font-size: 9px; font-weight: 750; }
  td { padding: 9px 10px; border-top: 1px solid var(--line); vertical-align: top; font-size: 11px; line-height: 1.45; }
  tr.off td { opacity: .55; }
  .c-inc { width: 34px; }
  .fname { font-weight: 700; overflow-wrap: anywhere; }
  .sub { color: var(--muted); font-size: 10px; overflow-wrap: anywhere; }
  .dest { color: var(--muted); overflow-wrap: anywhere; min-width: 220px; }
  .nnote { font-size: 10px; color: var(--amber); margin-top: 4px; overflow-wrap: anywhere; }
  .none { text-align: center; color: var(--muted); padding: 24px; }
  .more { display: flex; justify-content: center; margin-top: 12px; }
  .runbar { position: sticky; bottom: -30px; margin: 14px -24px -30px; padding: 12px 24px; display: flex; justify-content: flex-end; align-items: center; gap: 14px; background: var(--surface); border-top: 1px solid var(--line); }
  .runinfo { color: var(--muted); font-size: 12px; }
  .big { min-height: 40px; padding: 0 20px; font-size: 13px; }
  .danger { background: var(--amber); color: #fff; }
  dialog { padding: 0; border: 1px solid var(--line); border-radius: 16px; background: var(--surface); color: var(--text); width: min(520px, calc(100vw - 32px)); box-shadow: 0 24px 80px rgb(0 0 0 / 28%); }
  dialog::backdrop { background: rgb(12 16 12 / 52%); }
  .dcard { padding: 22px; }
  .dtitle { font-size: 18px; font-weight: 770; }
  .dtext { font-size: 13px; line-height: 1.5; overflow-wrap: anywhere; }
  .dhint { font-size: 11px; color: var(--muted); line-height: 1.5; }
  .dhint.warn { color: var(--amber); }
  .ack { display: flex; gap: 8px; align-items: flex-start; font-size: 12px; margin: 12px 0; line-height: 1.4; }
  .ack input { accent-color: var(--amber); margin-top: 2px; }
  .dactions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
</style>
