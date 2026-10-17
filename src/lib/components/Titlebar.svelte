<script lang="ts">
  import { t } from "$lib/i18n/uk";

  let { onToggleTheme, onOpenSettings, onAddFolders }: {
    onToggleTheme: () => void;
    onOpenSettings: () => void;
    onAddFolders: () => void;
  } = $props();
</script>

<header class="titlebar">
  <div class="brand">
    <div class="brand-mark" aria-hidden="true">k.</div>
    <div>
      <div class="brand-name">kadr</div>
      <div class="brand-sub">{t.brandSub}</div>
    </div>
  </div>
  <div class="top-center">
    <span class="online-dot" aria-hidden="true"></span>
    <span>{t.localMode}</span><span class="sep">·</span><span>{t.noCloud}</span>
  </div>
  <div class="top-right">
    <span class="demo-tag" title={t.demoHint}>{t.demoData}</span>
    <button class="icon-button" title={t.toggleTheme} aria-label={t.toggleTheme} onclick={onToggleTheme}>◐</button>
    <button class="icon-button" title={t.settings} aria-label={t.settings} onclick={onOpenSettings}>⚙</button>
    <button class="primary" onclick={onAddFolders}><span aria-hidden="true">＋</span> {t.addFolders}</button>
  </div>
</header>

<style>
  .titlebar {
    height: 64px; flex: 0 0 64px; display: flex; align-items: center; justify-content: space-between;
    gap: 18px; padding: 0 22px; background: var(--surface); border-bottom: 1px solid var(--line);
  }
  .brand { display: flex; align-items: center; gap: 11px; min-width: 215px; }
  .brand-mark {
    width: 32px; height: 32px; border-radius: 10px; background: var(--accent); color: var(--on-accent);
    display: grid; place-items: center; font-size: 18px; font-weight: 800; letter-spacing: -1.8px;
  }
  .brand-name { font-size: 16px; line-height: 1.1; font-weight: 750; letter-spacing: -.5px; }
  .brand-sub { font-size: 10px; color: var(--muted); margin-top: 4px; letter-spacing: .11em; text-transform: uppercase; }
  .top-center { display: flex; align-items: center; gap: 10px; color: var(--muted); font-size: 12px; }
  .sep { opacity: .45; }
  .online-dot {
    width: 7px; height: 7px; border-radius: 50%; background: var(--accent);
    display: inline-block; box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .top-right { display: flex; align-items: center; gap: 9px; }
  @media (max-width: 1250px) { .top-center { display: none; } .brand { min-width: 0; } }
</style>
