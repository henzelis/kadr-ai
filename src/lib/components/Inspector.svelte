<script lang="ts">
  import { errorMessage, setManualDate, setManualPlace } from "$lib/api";
  import { formatCaptured, formatCoords, formatDuration, formatSize } from "$lib/format";
  import { t } from "$lib/i18n/uk";
  import EditField from "./EditField.svelte";
  import Thumb from "./Thumb.svelte";
  import type { MediaView } from "$lib/types";

  let { item, selectedIds, demo, destination, onChanged, onMessage }: {
    item: MediaView | null;
    selectedIds: number[];
    demo: boolean;
    destination: string | null;
    onChanged: (rows: MediaView[]) => void;
    onMessage: (text: string) => void;
  } = $props();

  const count = $derived(selectedIds.length);
  let editing = $state<"date" | "place" | null>(null);
  let busy = $state(false);
  // Leave edit mode when the selection changes.
  $effect(() => {
    void selectedIds;
    editing = null;
  });

  async function apply(kind: "date" | "place", value: string | null) {
    if (demo) return onMessage(t.demoHint);
    busy = true;
    try {
      const rows = kind === "date" ? await setManualDate(selectedIds, value) : await setManualPlace(selectedIds, value);
      onChanged(rows);
      editing = null;
      onMessage(t.editSaved);
    } catch (e) {
      onMessage(errorMessage(e));
    } finally {
      busy = false;
    }
  }

  const dateInput = (local: string | undefined) => (local ? local.slice(0, 16) : "");

  const kindLabel = { photo: "Зображення", raw_photo: "RAW", video: "Відео", sidecar: "Sidecar", unsupported: "Непідтримуваний формат" };
</script>

<aside class="inspector" aria-label={t.inspector}>
  <div class="inspector-head">
    <div class="inspector-title">{count > 1 ? t.bulkTitle(count) : t.inspector}</div>
    <div class="inspector-kicker">{t.inspectorKicker}</div>
  </div>

  {#if count > 1}
    <p class="hint">{t.bulkHint}</p>
    <div class="detail-section">
      <div class="detail-section-title"><span>{t.editDateTitle} · {t.forFiles(count)}</span>
        {#if editing !== "date"}<button class="edit-text" onclick={() => (editing = "date")}>{t.edit}</button>{/if}</div>
      {#if editing === "date"}
        <EditField kind="date" initial="" canReset={true} {busy} onSave={(v) => apply("date", v)} onReset={() => apply("date", null)} onCancel={() => (editing = null)} />
      {/if}
    </div>
    <div class="detail-section">
      <div class="detail-section-title"><span>{t.editPlaceTitle} · {t.forFiles(count)}</span>
        {#if editing !== "place"}<button class="edit-text" onclick={() => (editing = "place")}>{t.edit}</button>{/if}</div>
      {#if editing === "place"}
        <EditField kind="place" initial="" canReset={true} {busy} onSave={(v) => apply("place", v)} onReset={() => apply("place", null)} onCancel={() => (editing = null)} />
      {/if}
    </div>
  {:else if !item}
    <p class="hint">{t.nothingSelected}</p>
  {:else}
    <div class="photo-preview">
      <Thumb {item} size="preview" {demo} fit="contain" />
    </div>
    <div class="detail-filename">{item.file_name}</div>
    <div class="detail-path">{item.path}</div>

    {#if item.review_reasons.length || item.last_error || item.warnings.length || item.duplicates.length}
      <div class="problems">
        {#if item.last_error}
          <div class="problem error"><strong>{t.readError}</strong>{item.last_error}</div>
        {/if}
        {#if item.review_reasons.length}
          <div class="problem review"><strong>{t.reviewReasons}</strong>
            <ul>{#each item.review_reasons as reason, i (i)}<li>{reason}</li>{/each}</ul></div>
        {/if}
        {#if item.duplicates.length}
          <div class="problem info"><strong>{t.otherCopies}</strong>
            <ul>{#each item.duplicates as d (d.id)}<li>{d.path}</li>{/each}</ul>
            <div>{t.otherCopiesHint}</div></div>
        {/if}
        {#if item.warnings.length}
          <div class="problem info"><strong>{t.warnings}</strong>
            <ul>{#each item.warnings as warning, i (i)}<li>{warning}</li>{/each}</ul></div>
        {/if}
      </div>
    {/if}

    <div class="detail-section">
      <div class="detail-section-title"><span>{t.dateAndTime}</span>
        {#if editing !== "date"}<button class="edit-text" onclick={() => (editing = "date")}>{t.edit}</button>{/if}</div>
      {#if editing === "date"}
        <EditField kind="date" initial={dateInput(item.captured?.local)} canReset={item.captured?.source === "manual"} {busy}
          onSave={(v) => apply("date", v)} onReset={() => apply("date", null)} onCancel={() => (editing = null)} />
      {/if}
      <div class="detail-row"><div class="detail-label">{t.capturedAt}</div>
        <div class="detail-value">{item.captured ? formatCaptured(item.captured.local, item.captured.offset_minutes) : t.unknown}</div></div>
      {#if item.captured}
        <div class="detail-row"><div class="detail-label">{t.dateSource}</div>
          <div class="detail-value">
            <span class="chip" class:review={item.captured.status !== "confirmed"}>
              {item.captured.status === "confirmed" ? "✓" : "!"} {t.dateSources[item.captured.source]}
            </span>
          </div></div>
        <div class="detail-row"><div class="detail-label">{t.rawValue}</div><div class="detail-value mono small">{item.captured.raw}</div></div>
        {#if item.captured.note}<div class="note">{item.captured.note}</div>{/if}
      {:else}
        <div class="note">{t.state.noDate}: файл потрапить у папку для файлів без дати.</div>
      {/if}
    </div>

    <div class="detail-section">
      <div class="detail-section-title"><span>{t.placeTitle}</span>
        {#if editing !== "place"}<button class="edit-text" onclick={() => (editing = "place")}>{t.edit}</button>{/if}</div>
      {#if editing === "place"}
        <EditField kind="place" initial={item.place_label ?? ""} canReset={item.location_source === "manual"} {busy}
          onSave={(v) => apply("place", v)} onReset={() => apply("place", null)} onCancel={() => (editing = null)} />
      {/if}
      <div class="detail-row"><div class="detail-label">{t.detectedPlace}</div>
        <div class="detail-value">{item.place_label ?? (item.place_status === "gps_no_place" ? t.state.gpsNoPlace : t.state.noGps)}</div></div>
      <div class="detail-row"><div class="detail-label">{t.placeSource}</div><div class="detail-value">{t.locationSources[item.location_source]}</div></div>
      <div class="gps-panel">
        <div class="gps-pin" aria-hidden="true">⌖</div>
        <div>
          <div class="gps-coord">{item.gps ? formatCoords(item.gps.latitude, item.gps.longitude) : "—"}</div>
          <div class="gps-caption">{item.gps ? t.gpsFromMetadata : t.noGpsInFile}</div>
        </div>
      </div>
    </div>

    <div class="detail-section">
      <div class="detail-section-title"><span>{t.file}</span></div>
      <div class="detail-row"><div class="detail-label">{t.format}</div><div class="detail-value">{item.extension.toUpperCase() || "—"} · {kindLabel[item.kind]}</div></div>
      {#if item.detected_type && item.warnings.some((w) => w.startsWith("Розширення"))}
        <div class="detail-row"><div class="detail-label">{t.detectedType}</div><div class="detail-value">{item.detected_type}</div></div>
      {/if}
      <div class="detail-row"><div class="detail-label">{t.size}</div><div class="detail-value">{formatSize(item.size_bytes)}</div></div>
      {#if item.width && item.height}
        <div class="detail-row"><div class="detail-label">{t.resolution}</div><div class="detail-value">{item.width} × {item.height}</div></div>
      {/if}
      {#if item.duration_ms !== null}
        <div class="detail-row"><div class="detail-label">{t.duration}</div><div class="detail-value">{formatDuration(item.duration_ms)}</div></div>
      {/if}
      <div class="detail-row"><div class="detail-label">SHA-256</div>
        <div class="detail-value mono small">{item.sha256 ? `${item.sha256.slice(0, 8)}…${item.sha256.slice(-4)}` : "—"}</div></div>
      {#if item.companions.length}
        <div class="detail-row"><div class="detail-label">{t.sidecars}</div>
          <div class="detail-value">
            {#each item.companions as c (c.id)}
              <div>{c.file_name} <span class="muted">· {t.companionRole[c.role]} · {formatSize(c.size_bytes)}</span></div>
            {/each}
            <div class="muted small-note">{t.companionsHint}</div>
          </div></div>
      {/if}
      {#if item.video_codec || item.audio_codec}
        <div class="detail-row"><div class="detail-label">{t.codecs}</div><div class="detail-value">{[item.video_codec, item.audio_codec].filter(Boolean).join(" · ")}</div></div>
      {/if}
      <div class="detail-row"><div class="detail-label">{t.metadataBy}</div>
        <div class="detail-value">{t.metadataStatus[item.metadata_status]}{item.metadata_provider && !demo ? ` · ${item.metadata_provider.split("|").filter((p) => p !== "-" && !/^[ph]\d/.test(p)).join(", ")}` : ""}</div></div>
    </div>

    {#if destination}
      <div class="destination-box">
        <div class="destination-head"><span>{t.destinationPath}</span><span>✓ {t.previewOnly}</span></div>
        <div class="destination-path">{destination}</div>
        <div class="destination-hint">{t.destinationHint}</div>
      </div>
    {/if}
  {/if}
</aside>

<style>
  .inspector { background: var(--surface); border-left: 1px solid var(--line); padding: 20px 16px 24px; min-width: 0; overflow-y: auto; }
  .inspector-head { margin: 1px 1px 15px; }
  .inspector-title { font-size: 13px; font-weight: 750; }
  .inspector-kicker { color: var(--muted); font-size: 10px; margin-top: 3px; }
  .hint { color: var(--muted); font-size: 11px; line-height: 1.5; }
  .photo-preview { aspect-ratio: 1.58 / 1; overflow: hidden; background: var(--surface-soft); border-radius: 10px; border: 1px solid var(--line); margin-bottom: 14px; }
  .photo-preview { background: light-dark(#e9ebe5, #121612); }
  .problems { display: grid; gap: 8px; margin-top: 12px; }
  .problem { font-size: 10px; line-height: 1.45; border-radius: 8px; padding: 8px 10px; overflow-wrap: anywhere; }
  .problem strong { display: block; margin-bottom: 3px; font-size: 10px; }
  .problem ul { margin: 0; padding-left: 16px; }
  .problem.error { background: var(--red-soft); color: var(--red); }
  .problem.review { background: var(--amber-soft); color: var(--amber); }
  .problem.info { background: var(--surface-soft); color: var(--muted); }
  .detail-filename { font-weight: 750; font-size: 13px; overflow-wrap: anywhere; }
  .detail-path { margin-top: 5px; font-size: 10px; line-height: 1.5; color: var(--muted); overflow-wrap: anywhere; }
  .detail-section { padding: 15px 0 13px; border-bottom: 1px solid var(--line); }
  .detail-section-title {
    display: flex; justify-content: space-between; color: var(--muted); font-size: 10px; font-weight: 750;
    letter-spacing: .06em; text-transform: uppercase; margin-bottom: 11px;
  }
  .edit-text { border: 0; background: transparent; color: var(--accent-strong); font-size: 10px; font-weight: 700; padding: 0; text-transform: none; letter-spacing: 0; }
  .detail-row { display: grid; grid-template-columns: 90px minmax(0, 1fr); gap: 8px; margin: 9px 0; }
  .detail-label { color: var(--muted); font-size: 10px; }
  .detail-value { font-size: 11px; line-height: 1.4; overflow-wrap: anywhere; font-weight: 570; }
  .small { font-size: 10px; }
  .muted { color: var(--muted); font-weight: 500; }
  .small-note { font-size: 9px; margin-top: 4px; line-height: 1.4; }
  .note { font-size: 10px; color: var(--amber); background: var(--amber-soft); border-radius: 7px; padding: 7px 9px; line-height: 1.45; }
  .gps-panel { display: flex; gap: 9px; align-items: center; padding: 10px; border-radius: 9px; background: var(--surface-soft); margin-top: 10px; }
  .gps-pin { width: 27px; height: 27px; border-radius: 8px; display: grid; place-items: center; color: var(--accent); background: var(--surface); font-size: 15px; }
  .gps-coord { font-size: 10px; font-weight: 650; }
  .gps-caption { font-size: 9px; color: var(--muted); margin-top: 4px; }
  .destination-box { background: var(--accent-soft); border: 1px solid light-dark(#d5e4ce, #354a35); border-radius: 10px; padding: 12px; margin-top: 16px; }
  .destination-head { display: flex; justify-content: space-between; margin-bottom: 8px; color: var(--accent-strong); font-size: 10px; font-weight: 750; }
  .destination-path { font-size: 10px; line-height: 1.55; overflow-wrap: anywhere; font-weight: 620; }
  .destination-hint { margin-top: 8px; font-size: 9px; color: var(--muted); line-height: 1.4; }
</style>
