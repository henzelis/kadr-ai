<script lang="ts">
  import { errorMessage, getToolStatus, inTauri, isAppError, updateSettings } from "$lib/api";
  import { t } from "$lib/i18n/uk";
  import type { AppSettings, ToolStatus } from "$lib/types";

  let { settings, onClose, onSaved }: {
    settings: AppSettings;
    onClose: () => void;
    onSaved: (s: AppSettings) => void;
  } = $props();

  // Edit a copy taken once on open; stored settings change only after Rust accepts them.
  // svelte-ignore state_referenced_locally
  let draft: AppSettings = $state(structuredClone($state.snapshot(settings)));
  let tools: ToolStatus[] | null = $state(null);
  let error: string | null = $state(null);
  let errorField: string | null = $state(null);
  let saving = $state(false);
  let dialog: HTMLDialogElement | undefined = $state();

  $effect(() => {
    dialog?.showModal();
    void recheck();
  });

  async function recheck() {
    tools = null;
    try {
      tools = await getToolStatus();
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function save() {
    saving = true;
    error = null;
    errorField = null;
    try {
      const clean = {
        ...draft,
        exiftool_path: draft.exiftool_path?.trim() || null,
        ffprobe_path: draft.ffprobe_path?.trim() || null,
      };
      onSaved(await updateSettings(clean));
    } catch (e) {
      error = errorMessage(e);
      errorField = isAppError(e) ? e.field : null;
    } finally {
      saving = false;
    }
  }

  function toolName(tool: ToolStatus["tool"]) {
    return tool === "exiftool" ? "ExifTool" : "FFprobe";
  }
</script>

<dialog bind:this={dialog} aria-labelledby="settings-title" onclose={onClose} onclick={(e) => e.target === dialog && dialog?.close()}>
  <form method="dialog" class="card" onsubmit={(e) => { e.preventDefault(); void save(); }}>
    <div class="head">
      <div>
        <div class="title" id="settings-title">{t.settingsDialog.title}</div>
        <div class="subtitle">{t.settingsDialog.subtitle}</div>
      </div>
      <button type="button" class="icon-button" aria-label={t.close} onclick={() => dialog?.close()}>✕</button>
    </div>
    {#if !inTauri}<div class="browser-note">{t.settingsDialog.browserNote}</div>{/if}

    <fieldset>
      <legend>{t.settingsDialog.sorting}</legend>
      <div class="grid">
        <label class="row">{t.settingsDialog.monthFormat}
          <select bind:value={draft.month_format}>
            <option value="number_name">08 - Серпень</option>
            <option value="number">08</option>
            <option value="name">Серпень</option>
          </select>
        </label>
        <label class="row">{t.settingsDialog.granularity}
          <select bind:value={draft.place_granularity}>
            <option value="city">{t.settingsDialog.city}</option>
            <option value="region">{t.settingsDialog.region}</option>
            <option value="country">{t.settingsDialog.country}</option>
          </select>
          <span class="hint">{t.settingsDialog.granularityHint}</span>
        </label>
        <label class="row">{t.settingsDialog.unknownPlace}
          <input bind:value={draft.unknown_place_label} aria-invalid={errorField === "unknown_place_label"} />
        </label>
        <label class="row">{t.settingsDialog.unknownDate}
          <input bind:value={draft.unknown_date_label} aria-invalid={errorField === "unknown_date_label"} />
        </label>
        <label class="row wide">{t.settingsDialog.conflict}
          <select bind:value={draft.conflict_policy}>
            <option value="add_suffix">{t.settingsDialog.conflictSuffix}</option>
            <option value="skip">{t.settingsDialog.conflictSkip}</option>
            <option value="ask_user">{t.settingsDialog.conflictAsk}</option>
          </select>
        </label>
      </div>
      <label class="check"><input type="checkbox" bind:checked={draft.filesystem_date_fallback} />
        <span><strong>{t.settingsDialog.fsFallback}</strong><br />{t.settingsDialog.fsFallbackHint}</span></label>
      <label class="check"><input type="checkbox" bind:checked={draft.coordinates_as_place} />
        <span><strong>{t.settingsDialog.coordsAsPlace}</strong><br />{t.settingsDialog.coordsAsPlaceHint}</span></label>
      <label class="check"><input type="checkbox" bind:checked={draft.offline_geocoding} />
        <span><strong>{t.settingsDialog.offlineGeo}</strong><br />{t.settingsDialog.offlineGeoHint}</span></label>
    </fieldset>

    <fieldset>
      <legend>{t.settingsDialog.scanning}</legend>
      <label class="check"><input type="checkbox" bind:checked={draft.skip_system_dirs} />
        <span><strong>{t.settingsDialog.skipSystem}</strong></span></label>
      <label class="check"><input type="checkbox" bind:checked={draft.follow_symlinks} />
        <span><strong>{t.settingsDialog.followSymlinks}</strong><br />{t.settingsDialog.followSymlinksHint}</span></label>
      <div class="grid">
        <label class="row">{t.settingsDialog.metadataWorkers}
          <input type="number" min="1" max="32" bind:value={draft.metadata_workers} aria-invalid={errorField === "metadata_workers"} /></label>
        <label class="row">{t.settingsDialog.thumbnailWorkers}
          <input type="number" min="1" max="32" bind:value={draft.thumbnail_workers} aria-invalid={errorField === "thumbnail_workers"} /></label>
      </div>
    </fieldset>

    <fieldset>
      <legend>{t.settingsDialog.helpers}</legend>
      <div class="tools">
        {#if tools === null}
          <div class="hint">{t.settingsDialog.checking}</div>
        {:else}
          {#each tools as tool (tool.tool)}
            <div class="tool">
              <strong>{toolName(tool.tool)}</strong>
              {#if tool.state === "available"}
                <span class="chip">✓ {t.settingsDialog.available(tool.version)}</span>
              {:else if tool.state === "missing"}
                <span class="chip error">✕ {t.settingsDialog.missing}</span>
              {:else}
                <span class="chip review" title={tool.reason}>! {t.settingsDialog.broken}</span>
              {/if}
            </div>
            {#if tool.state !== "available"}<div class="hint">{t.settingsDialog.missingHint}</div>{/if}
          {/each}
        {/if}
      </div>
      <div class="grid">
        <label class="row">{t.settingsDialog.exiftoolPath}
          <input bind:value={draft.exiftool_path} placeholder={t.settingsDialog.pathPlaceholder} /></label>
        <label class="row">{t.settingsDialog.ffprobePath}
          <input bind:value={draft.ffprobe_path} placeholder={t.settingsDialog.pathPlaceholder} /></label>
      </div>
      <button type="button" class="secondary" onclick={recheck}>↻ {t.settingsDialog.recheck}</button>
    </fieldset>

    <fieldset>
      <legend>{t.settingsDialog.appearance}</legend>
      <label class="row">{t.settingsDialog.theme}
        <select bind:value={draft.theme}>
          <option value="system">{t.settingsDialog.themeSystem}</option>
          <option value="light">{t.settingsDialog.themeLight}</option>
          <option value="dark">{t.settingsDialog.themeDark}</option>
        </select>
      </label>
    </fieldset>

    {#if error}<div class="error" role="alert">{error}</div>{/if}
    <div class="actions">
      <button type="button" class="secondary" onclick={() => dialog?.close()}>{t.settingsDialog.cancel}</button>
      <button type="submit" class="primary" disabled={saving}>{t.settingsDialog.save}</button>
    </div>
  </form>
</dialog>

<style>
  dialog {
    padding: 0; border: 1px solid var(--line); border-radius: 16px; background: var(--surface); color: var(--text);
    width: min(620px, calc(100vw - 32px)); max-height: 90vh; box-shadow: 0 24px 80px rgb(0 0 0 / 28%);
  }
  dialog::backdrop { background: rgb(12 16 12 / 52%); }
  .card { padding: 22px; }
  .head { display: flex; justify-content: space-between; gap: 15px; margin-bottom: 10px; }
  .title { font-size: 19px; font-weight: 770; letter-spacing: -.6px; }
  .subtitle { color: var(--muted); font-size: 11px; margin-top: 6px; }
  .browser-note { font-size: 11px; color: var(--amber); background: var(--amber-soft); padding: 8px 10px; border-radius: 8px; }
  fieldset { border: 0; border-top: 1px solid var(--line); padding: 14px 0 6px; margin: 12px 0 0; }
  legend { font-size: 10px; font-weight: 750; letter-spacing: .08em; text-transform: uppercase; color: var(--muted); padding-right: 8px; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 0 14px; }
  .row { display: flex; flex-direction: column; gap: 7px; font-size: 11px; font-weight: 700; margin: 10px 0; }
  .wide { grid-column: 1 / -1; }
  .row select, .row input {
    height: 36px; border: 1px solid var(--line); background: var(--surface-raised); border-radius: 8px; padding: 0 10px; font-weight: 400;
  }
  .row input[aria-invalid="true"] { border-color: var(--red); }
  .hint { font-size: 10px; color: var(--muted); font-weight: 400; line-height: 1.4; }
  .check { display: flex; gap: 10px; align-items: flex-start; font-size: 11px; line-height: 1.5; color: var(--muted); margin: 10px 0; }
  .check strong { color: var(--text); }
  .check input { accent-color: var(--accent); margin-top: 3px; }
  .tools { display: grid; gap: 6px; margin: 6px 0 4px; }
  .tool { display: flex; gap: 10px; align-items: center; font-size: 12px; }
  .error { margin-top: 14px; font-size: 11px; color: var(--red); background: var(--red-soft); border-radius: 8px; padding: 9px 11px; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }
</style>
