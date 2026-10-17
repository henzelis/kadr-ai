<script lang="ts">
  import { applySortPlan, errorMessage, getRunFailures, getRunLog, undoSortPlan } from "$lib/api";
  import { formatSize } from "$lib/format";
  import { noteText, t } from "$lib/i18n";
  import type { RunFailure, RunSummary } from "$lib/types";

  let { running, onMessage }: { running: boolean; onMessage: (text: string) => void } = $props();

  let runs = $state<RunSummary[]>([]);
  let failures = $state<Record<number, RunFailure[]>>({});
  let undoTarget = $state<RunSummary | null>(null);
  let dialog: HTMLDialogElement | undefined = $state();

  async function load() {
    try {
      runs = (await getRunLog()).runs;
    } catch (e) {
      onMessage(errorMessage(e));
    }
  }

  $effect(() => {
    void running;
    void load();
  });

  async function toggleFailures(id: number) {
    if (failures[id]) {
      const { [id]: _, ...rest } = failures;
      failures = rest;
      return;
    }
    failures = { ...failures, [id]: await getRunFailures(id).catch(() => []) };
  }

  function askUndo(r: RunSummary) {
    undoTarget = r;
    queueMicrotask(() => dialog?.showModal());
  }

  async function undo() {
    const r = undoTarget;
    dialog?.close();
    if (r) await undoSortPlan(r.plan_id).catch((e) => onMessage(errorMessage(e)));
  }

  async function resume(r: RunSummary) {
    await applySortPlan(r.plan_id, r.mode, 0).catch((e) => onMessage(errorMessage(e)));
  }

  const when = (ms: number) =>
    new Date(ms).toLocaleString(t.locale, { day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit" });
</script>

{#if runs.length === 0}
  <div class="empty"><strong>{t.log.empty}</strong>{t.log.emptyHint}</div>
{:else}
  <ul class="runs">
    {#each runs as r (r.plan_id)}
      <li class="run">
        <div class="head">
          <span class="chip" class:review={r.state === "partial"} class:missing={r.state === "undone"}>{t.log.state[r.state] ?? r.state}</span>
          <strong>{r.mode === "move" ? t.log.move : t.log.copy}</strong>
          <span class="muted">{when(r.created_at)}</span>
        </div>
        <div class="dest" title={r.destination_root}>→ {r.destination_root}</div>
        <div class="muted">{r.state === "undone" ? t.log.undoneCount(r.undone) : t.log.counts(r.done, r.failed, r.pending, formatSize(r.bytes_done))}</div>
        <div class="actions">
          {#if r.state === "partial" && (r.pending > 0 || r.failed > 0)}
            <button class="primary small" disabled={running} onclick={() => resume(r)}>{r.failed ? t.log.retry : t.planView.continue}</button>
          {/if}
          {#if r.failed > 0}
            <button class="secondary small" onclick={() => toggleFailures(r.plan_id)}>{failures[r.plan_id] ? t.log.hideFailures : `${t.log.showFailures} (${r.failed})`}</button>
          {/if}
          {#if (r.state === "done" || r.state === "partial") && r.done > 0}
            <button class="secondary small" disabled={running} onclick={() => askUndo(r)}>↺ {t.log.undo}</button>
          {/if}
        </div>
        {#if failures[r.plan_id]}
          <ul class="fails">
            {#each failures[r.plan_id] as f, i (i)}
              <li><span class="path">{f.source}</span>{#if f.reason}<span class="why">{noteText(f.reason)}</span>{/if}</li>
            {/each}
          </ul>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

{#if undoTarget}
  <dialog bind:this={dialog} onclose={() => (undoTarget = null)} aria-labelledby="undo-title">
    <div class="dcard">
      <div class="dtitle" id="undo-title">{t.log.undo}</div>
      <p class="dtext">{t.log.undoConfirm(undoTarget.mode)}</p>
      <div class="dactions">
        <button class="secondary" onclick={() => dialog?.close()}>{t.confirm.cancel}</button>
        <button class="primary" onclick={undo}>{t.log.undo}</button>
      </div>
    </div>
  </dialog>
{/if}

<style>
  dialog { padding: 0; border: 1px solid var(--line); border-radius: 16px; background: var(--surface); color: var(--text); width: min(480px, calc(100vw - 32px)); box-shadow: 0 24px 80px rgb(0 0 0 / 28%); }
  dialog::backdrop { background: rgb(12 16 12 / 52%); }
  .dcard { padding: 22px; }
  .dtitle { font-size: 18px; font-weight: 770; }
  .dtext { font-size: 13px; line-height: 1.5; }
  .dactions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
  .empty { border: 1px dashed var(--line); border-radius: 12px; padding: 45px 20px; text-align: center; color: var(--muted); background: var(--surface); line-height: 1.5; }
  .empty strong { display: block; color: var(--text); margin-bottom: 7px; font-size: 14px; }
  .runs { list-style: none; margin: 0; padding: 0; display: grid; gap: 10px; }
  .run { background: var(--surface); border: 1px solid var(--line); border-radius: 12px; padding: 12px 14px; display: grid; gap: 6px; }
  .head { display: flex; gap: 10px; align-items: center; }
  .muted { color: var(--muted); font-size: 11px; }
  .dest { font-size: 11px; overflow-wrap: anywhere; }
  .actions { display: flex; gap: 8px; flex-wrap: wrap; margin-top: 4px; }
  .small { min-height: 30px; font-size: 11px; }
  .fails { list-style: none; margin: 6px 0 0; padding: 0; display: grid; gap: 5px; font-size: 11px; }
  .fails li { display: grid; gap: 2px; background: var(--surface-soft); border-radius: 7px; padding: 7px 9px; }
  .path { overflow-wrap: anywhere; }
  .why { color: var(--red); }
</style>
