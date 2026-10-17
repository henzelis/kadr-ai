<script lang="ts">
  import { errorMessage, getScanReport } from "$lib/api";
  import { formatCount } from "$lib/format";
  import { t } from "$lib/i18n/uk";
  import type { ScanReport } from "$lib/types";

  let { onClose }: { onClose: () => void } = $props();

  let report = $state<ScanReport | null>(null);
  let loaded = $state(false);
  let error = $state<string | null>(null);
  let dialog: HTMLDialogElement | undefined = $state();

  $effect(() => {
    dialog?.showModal();
    getScanReport()
      .then((r) => (report = r))
      .catch((e) => (error = errorMessage(e)))
      .finally(() => (loaded = true));
  });

  const notMediaTotal = $derived(report?.not_media_by_extension.reduce((a, x) => a + x.count, 0) ?? 0);
</script>

<dialog bind:this={dialog} aria-labelledby="report-title" onclose={onClose} onclick={(e) => e.target === dialog && dialog?.close()}>
  <div class="card">
    <div class="head">
      <div>
        <div class="title" id="report-title">{t.report.title}</div>
        <div class="subtitle">{t.report.subtitle}</div>
      </div>
      <button class="icon-button" aria-label={t.report.close} onclick={() => dialog?.close()}>✕</button>
    </div>

    {#if error}
      <div class="error">{error}</div>
    {:else if loaded && !report}
      <p class="muted">{t.report.none}</p>
    {:else if report}
      <table class="numbers">
        <tbody>
          <tr><th>{t.report.found}</th><td>{formatCount(report.summary.found)}</td></tr>
          <tr class="sub"><th>{t.report.read}</th><td>{formatCount(report.summary.processed)}</td></tr>
          <tr class="sub"><th>{t.report.unchanged}</th><td>{formatCount(report.summary.unchanged)}</td></tr>
          <tr><th>{t.report.inLibrary}</th><td>{formatCount(report.in_library)}</td></tr>
          <tr class="sub"><th>{t.report.companions}</th><td>{formatCount(report.companions)}</td></tr>
          {#if report.missing}<tr class="sub"><th>{t.report.missing}</th><td>{formatCount(report.missing)}</td></tr>{/if}
          <tr><th>{t.report.notMedia}</th><td>{formatCount(notMediaTotal)}</td></tr>
        </tbody>
      </table>

      {#if report.not_media_by_extension.length}
        <div class="exts">
          {#each report.not_media_by_extension as x (x.extension)}
            <span class="chip missing">{x.extension ? `.${x.extension}` : t.report.noExtension} × {formatCount(x.count)}</span>
          {/each}
        </div>
        <p class="hint">{t.report.notMediaHint}</p>
      {/if}

      <h3>{t.report.problems}</h3>
      {#if report.issues.length === 0}
        <p class="muted">{t.report.noProblems}</p>
      {:else}
        <ul class="issues">
          {#each report.issues as issue, i (i)}
            <li>
              <span class="chip" class:error={issue.kind.endsWith("error") || issue.kind === "root_unavailable"} class:missing={!issue.kind.endsWith("error")}>
                {t.report.kinds[issue.kind] ?? issue.kind}
              </span>
              <span class="path">{issue.path}</span>
              {#if issue.message}<span class="msg">{issue.message}</span>{/if}
            </li>
          {/each}
        </ul>
        {#if report.issues.length >= 2000}<p class="hint">{t.report.shownFirst(2000)}</p>{/if}
      {/if}
    {/if}
  </div>
</dialog>

<style>
  dialog { padding: 0; border: 1px solid var(--line); border-radius: 16px; background: var(--surface); color: var(--text); width: min(720px, calc(100vw - 32px)); max-height: 88vh; box-shadow: 0 24px 80px rgb(0 0 0 / 28%); }
  dialog::backdrop { background: rgb(12 16 12 / 52%); }
  .card { padding: 22px; }
  .head { display: flex; justify-content: space-between; gap: 15px; margin-bottom: 14px; }
  .title { font-size: 19px; font-weight: 770; letter-spacing: -.6px; }
  .subtitle, .muted, .hint { color: var(--muted); font-size: 11px; margin-top: 6px; line-height: 1.45; }
  .numbers { width: 100%; border-collapse: collapse; font-size: 12px; }
  .numbers th { text-align: left; font-weight: 650; padding: 7px 0; border-top: 1px solid var(--line); }
  .numbers td { text-align: right; font-weight: 750; padding: 7px 0; border-top: 1px solid var(--line); font-variant-numeric: tabular-nums; }
  .numbers tr.sub th { font-weight: 500; color: var(--muted); padding-left: 16px; }
  .numbers tr.sub td { color: var(--muted); font-weight: 600; }
  .exts { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 12px; }
  h3 { font-size: 12px; margin: 18px 0 8px; }
  .issues { list-style: none; padding: 0; margin: 0; display: grid; gap: 6px; font-size: 11px; }
  .issues li { display: grid; gap: 3px; padding: 8px 10px; background: var(--surface-soft); border-radius: 8px; }
  .issues .chip { justify-self: start; }
  .path { overflow-wrap: anywhere; }
  .msg { color: var(--muted); overflow-wrap: anywhere; }
  .error { color: var(--red); }
</style>
