<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { i18n, t, type Lang } from "$lib/i18n.svelte";
  import Icon from "$lib/Icon.svelte";
  import icWand from "@material-symbols/svg-400/outlined/wand_stars.svg?raw";
  import icSend from "@material-symbols/svg-400/outlined/send.svg?raw";
  import icHistory from "@material-symbols/svg-400/outlined/history.svg?raw";
  import icSettings from "@material-symbols/svg-400/outlined/settings.svg?raw";
  import icClose from "@material-symbols/svg-400/outlined/close.svg?raw";
  import icPlay from "@material-symbols/svg-400/outlined/play_arrow.svg?raw";
  import icSpinner from "@material-symbols/svg-400/outlined/progress_activity.svg?raw";
  import icKeyboard from "@material-symbols/svg-400/outlined/keyboard.svg?raw";

  type Preset = {
    id: string;
    label: string;
    description: string;
    target_ext: string;
    category: string;
  };
  type AiSuggestion = { target_ext: string; options: string[]; explanation: string };
  type ProgressEvent = {
    input: string;
    stage: "start" | "line" | "done" | "error" | "cancelled";
    message: string;
    output: string | null;
  };
  type DownloadEvent = {
    stage: "start" | "progress" | "extract" | "done" | "error";
    message: string;
    progress: number | null;
  };
  type Settings = {
    provider: string;
    openrouter_model: string;
    ollama_url: string;
    ollama_model: string;
    output_dir: string | null;
    filename_template: string;
    theme: string;
    language: string;
    hotkey: string;
    hide_on_blur: boolean;
  };
  type HistoryEntry = {
    ts: number;
    kind: "preset" | "ai";
    label: string;
    target_ext: string;
    files: number;
  };
  type View = "list" | "confirm" | "running" | "settings" | "history";

  let context = $state<string[]>([]);
  let recs = $state<Preset[]>([]);
  let ffmpegOk = $state(true);
  let query = $state("");
  let view = $state<View>("list");

  // Подтверждение (пресет или ИИ).
  let confirmMode = $state<"preset" | "ai">("preset");
  let selected = $state<Preset | null>(null);
  let aiSuggestion = $state<AiSuggestion | null>(null);
  let previews = $state<string[]>([]);
  let aiLoading = $state(false);

  // Выполнение.
  let logLines = $state<string[]>([]);
  let doneCount = $state(0);
  let errorCount = $state(0);
  let cancelledCount = $state(0);
  let totalJobs = $state(0);

  // Авто-исправление ИИ-команды: всего попыток (1 генерация + 2 фикса от ИИ).
  const MAX_AI_ATTEMPTS = 3;
  let aiAttempt = $state(1);
  let retrying = $state(false);
  let retryAborted = false;
  let failedFiles: { input: string; error: string }[] = [];

  // Загрузка ffmpeg.
  let downloading = $state(false);
  let downloadPct = $state<number | null>(null);
  let downloadMsg = $state("");

  // Настройки.
  let settings = $state<Settings>({
    provider: "openrouter",
    openrouter_model: "openai/gpt-4o-mini",
    ollama_url: "http://localhost:11434",
    ollama_model: "qwen2.5:7b",
    output_dir: null,
    filename_template: "{name}_converted",
    theme: "system",
    language: "ru",
    hotkey: "ctrl+alt+space",
    hide_on_blur: false,
  });
  let apiKeyInput = $state("");
  let apiKeyPresent = $state(false);
  let ctxMenuOn = $state(false);
  let autostartOn = $state(false);

  // История.
  let history = $state<HistoryEntry[]>([]);

  let notice = $state("");
  let dragActive = $state(false);
  let capturingHotkey = $state(false);
  let pop = $state(false);

  /// Перезапустить анимацию появления палитры (класс снимается и вешается заново).
  function replayPop() {
    pop = false;
    requestAnimationFrame(() => requestAnimationFrame(() => (pop = true)));
  }

  // ── Захват хоткея нажатием клавиш ──────────────────────────────────────────

  /// KeyboardEvent → строка вида "ctrl+alt+space" (формат parse_hotkey в Rust).
  /// null — комбинация неполная (нет модификатора или неподдерживаемая клавиша).
  function hotkeySpecFromEvent(e: KeyboardEvent): string | null {
    const mods: string[] = [];
    if (e.ctrlKey) mods.push("ctrl");
    if (e.altKey) mods.push("alt");
    if (e.shiftKey) mods.push("shift");
    if (e.metaKey) mods.push("win");
    const c = e.code;
    let key: string | null = null;
    if (c === "Space") key = "space";
    else if (/^Key[A-Z]$/.test(c)) key = c.slice(3).toLowerCase();
    else if (/^Digit[0-9]$/.test(c)) key = c.slice(5);
    else if (/^F([1-9]|1[0-2])$/.test(c)) key = c.toLowerCase();
    if (!key || mods.length === 0) return null;
    return [...mods, key].join("+");
  }

  function onHotkeyCapture(e: KeyboardEvent) {
    if (!capturingHotkey) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      capturingHotkey = false;
      return;
    }
    const spec = hotkeySpecFromEvent(e);
    if (spec) {
      settings.hotkey = spec;
      capturingHotkey = false;
    }
  }

  const fmtHotkey = (spec: string) => spec.toUpperCase().replaceAll("+", " + ");

  const basename = (p: string) => p.replace(/\\/g, "/").split("/").pop() || p;

  // ── Тема и язык ────────────────────────────────────────────────────────────

  const media = window.matchMedia("(prefers-color-scheme: dark)");

  function applyAppearance() {
    const dark =
      settings.theme === "dark" || (settings.theme === "system" && media.matches);
    document.documentElement.dataset.theme = dark ? "dark" : "light";
    i18n.lang = (settings.language === "en" ? "en" : "ru") as Lang;
  }

  async function loadSettings() {
    settings = await invoke<Settings>("get_settings");
    applyAppearance();
  }

  // ── Контекст и рекомендации ────────────────────────────────────────────────

  async function refresh() {
    context = await invoke<string[]>("get_context");
    recs = await invoke<Preset[]>("get_recommendations", { paths: context });
    ffmpegOk = await invoke<boolean>("ffmpeg_available");
    view = "list";
    selected = null;
    aiSuggestion = null;
  }

  /// Добавить в контекст файлы, брошенные перетаскиванием.
  async function addDroppedPaths(paths: string[]) {
    if (!paths || paths.length === 0) return;
    const merged = [...context];
    for (const p of paths) if (!merged.includes(p)) merged.push(p);
    context = merged;
    recs = await invoke<Preset[]>("get_recommendations", { paths: context });
    view = "list";
    selected = null;
    aiSuggestion = null;
    notice = "";
  }

  async function selectPreset(p: Preset) {
    confirmMode = "preset";
    selected = p;
    previews = await invoke<string[]>("command_preview", { presetId: p.id, paths: context });
    view = "confirm";
  }

  async function askAi() {
    if (!query.trim()) {
      notice = t("n_enter_query");
      return;
    }
    if (context.length === 0) {
      notice = t("n_no_files");
      return;
    }
    aiLoading = true;
    notice = "";
    try {
      const s = await invoke<AiSuggestion>("ai_generate", { query, paths: context });
      aiSuggestion = s;
      confirmMode = "ai";
      previews = await invoke<string[]>("ai_command_preview", {
        options: s.options,
        targetExt: s.target_ext,
        paths: context,
      });
      view = "confirm";
    } catch (e) {
      notice = `ИИ: ${e}`;
    } finally {
      aiLoading = false;
    }
  }

  async function runConfirm() {
    if (!ffmpegOk) {
      notice = t("n_need_ffmpeg");
      return;
    }
    logLines = [];
    doneCount = 0;
    errorCount = 0;
    cancelledCount = 0;
    totalJobs = context.length;
    aiAttempt = 1;
    retryAborted = false;
    failedFiles = [];
    view = "running";
    try {
      if (confirmMode === "preset" && selected) {
        await invoke("start_conversion", { presetId: selected.id, paths: context });
        await invoke("add_history", {
          kind: "preset",
          label: selected.label,
          targetExt: selected.target_ext,
          files: context.length,
        });
      } else if (confirmMode === "ai" && aiSuggestion) {
        await invoke("run_ai_conversion", {
          options: aiSuggestion.options,
          targetExt: aiSuggestion.target_ext,
          paths: context,
        });
        await invoke("add_history", {
          kind: "ai",
          label: query,
          targetExt: aiSuggestion.target_ext,
          files: context.length,
        });
      }
    } catch (e) {
      notice = `${e}`;
      view = "confirm";
    }
  }

  async function cancel() {
    retryAborted = true;
    await invoke("cancel_conversion");
  }

  /// Отправить ошибку ffmpeg обратно ИИ и перезапустить неудавшиеся файлы.
  async function aiRetry() {
    if (retrying || !aiSuggestion || failedFiles.length === 0) return;
    retrying = true;
    const failed = [...failedFiles];
    const inputs = failed.map((f) => f.input);
    const errText = failed
      .map((f) => f.error)
      .join("\n")
      .slice(0, 2000);
    logLines = [...logLines, `🔁 ${t("ai_fixing")} (${aiAttempt + 1}/${MAX_AI_ATTEMPTS})…`];
    try {
      const s = await invoke<AiSuggestion>("ai_fix", {
        query,
        paths: inputs,
        prevOptions: aiSuggestion.options,
        targetExt: aiSuggestion.target_ext,
        error: errText,
      });
      if (retryAborted) return;
      aiSuggestion = s;
      aiAttempt += 1;
      failedFiles = [];
      doneCount = 0;
      errorCount = 0;
      cancelledCount = 0;
      totalJobs = inputs.length;
      if (s.explanation) logLines = [...logLines, `✨ ${s.explanation}`];
      await invoke("run_ai_conversion", {
        options: s.options,
        targetExt: s.target_ext,
        paths: inputs,
      });
    } catch (e) {
      logLines = [...logLines, `❌ ${t("ai_fix_failed")}: ${e}`];
      aiAttempt = MAX_AI_ATTEMPTS; // больше не пытаемся
    } finally {
      retrying = false;
    }
  }

  // ── Настройки ──────────────────────────────────────────────────────────────

  async function openSettings() {
    settings = await invoke<Settings>("get_settings");
    apiKeyPresent = await invoke<boolean>("has_api_key", { provider: settings.provider });
    ctxMenuOn = await invoke<boolean>("context_menu_status");
    autostartOn = await invoke<boolean>("autostart_status");
    apiKeyInput = "";
    view = "settings";
  }

  async function saveSettings() {
    try {
      const warn = await invoke<string | null>("save_settings", { newSettings: settings });
      if (apiKeyInput.trim()) {
        await invoke("set_api_key", { provider: settings.provider, key: apiKeyInput.trim() });
      }
      await invoke("set_context_menu", { enabled: ctxMenuOn });
      await invoke("set_autostart", { enabled: autostartOn });
      apiKeyPresent = await invoke<boolean>("has_api_key", { provider: settings.provider });
      applyAppearance();
      notice = warn ?? t("n_saved");
      view = "list";
    } catch (e) {
      notice = `${e}`;
    }
  }

  // ── История ────────────────────────────────────────────────────────────────

  async function openHistory() {
    history = await invoke<HistoryEntry[]>("get_history");
    view = "history";
  }

  async function clearHistory() {
    await invoke("clear_history");
    history = [];
  }

  function useHistoryEntry(h: HistoryEntry) {
    if (h.kind === "ai") {
      query = h.label;
      view = "list";
    } else {
      const p = recs.find((r) => r.label === h.label);
      if (p) selectPreset(p);
    }
  }

  function fmtDate(ts: number): string {
    return new Date(ts * 1000).toLocaleString(i18n.lang === "ru" ? "ru-RU" : "en-US", {
      day: "2-digit",
      month: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  // ── ffmpeg ─────────────────────────────────────────────────────────────────

  async function downloadFfmpeg() {
    downloading = true;
    downloadPct = 0;
    downloadMsg = t("preparing");
    await invoke("download_ffmpeg");
  }

  function onKey(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    if (view === "confirm" || view === "settings" || view === "history") {
      view = "list";
      selected = null;
      aiSuggestion = null;
    } else if (view !== "running") {
      invoke("hide_palette");
    }
  }

  onMount(() => {
    loadSettings();
    refresh();
    const unlisteners: Promise<UnlistenFn>[] = [];

    unlisteners.push(listen("context-updated", () => refresh()));
    unlisteners.push(listen("open-settings", () => openSettings()));
    unlisteners.push(listen("palette-shown", () => replayPop()));
    replayPop();

    // Перетаскивание файлов в окно (нативное событие Tauri, содержит полные пути).
    try {
      unlisteners.push(
      getCurrentWebview().onDragDropEvent((event) => {
        const kind = event.payload.type;
        if (kind === "enter" || kind === "over") {
          dragActive = true;
        } else if (kind === "leave") {
          dragActive = false;
        } else if (kind === "drop") {
          dragActive = false;
          addDroppedPaths(event.payload.paths);
        }
      }),
      );
    } catch {
      // Вне Tauri (dev в обычном браузере) DnD-события окна недоступны.
    }

    unlisteners.push(
      listen<ProgressEvent>("conversion://progress", (e) => {
        const p = e.payload;
        if (p.stage === "line") {
          logLines = [...logLines.slice(-40), `${basename(p.input)}: ${p.message}`];
        } else if (p.stage === "done") {
          doneCount += 1;
          logLines = [...logLines, `✅ ${p.message}`];
        } else if (p.stage === "error") {
          errorCount += 1;
          failedFiles.push({ input: p.input, error: p.message });
          logLines = [...logLines, `❌ ${basename(p.input)}: ${p.message}`];
        } else if (p.stage === "cancelled") {
          cancelledCount += 1;
          logLines = [...logLines, `⛔ ${basename(p.input)}: ${t("sum_cancel")}`];
        }
      }),
    );

    unlisteners.push(
      listen<DownloadEvent>("ffmpeg://download", (e) => {
        const d = e.payload;
        downloadMsg = d.message;
        downloadPct = d.progress;
        if (d.stage === "done") {
          downloading = false;
          ffmpegOk = true;
          notice = t("n_ffmpeg_ok");
        } else if (d.stage === "error") {
          downloading = false;
          notice = `${t("n_dl_err")}: ${d.message}`;
        }
      }),
    );

    // Смена системной темы на лету (для theme=system).
    const onMedia = () => applyAppearance();
    media.addEventListener("change", onMedia);

    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      media.removeEventListener("change", onMedia);
      unlisteners.forEach((u) => u.then((fn) => fn()));
    };
  });

  const finished = $derived(
    view === "running" && !retrying && doneCount + errorCount + cancelledCount >= totalJobs,
  );

  // Авто-ретрай: пачка ИИ-конвертации завершилась с ошибками — просим ИИ
  // исправить опции и перезапускаем только неудавшиеся файлы.
  $effect(() => {
    if (
      finished &&
      confirmMode === "ai" &&
      errorCount > 0 &&
      cancelledCount === 0 &&
      aiAttempt < MAX_AI_ATTEMPTS &&
      !retryAborted
    ) {
      aiRetry();
    }
  });
</script>

<div class="palette" class:drag={dragActive} class:pop>
  <div class="topbar" data-tauri-drag-region>
    <span class="search-icon" data-tauri-drag-region>
      {#if aiLoading}<Icon svg={icSpinner} size={18} spin />{:else}<Icon svg={icWand} size={18} />{/if}
    </span>
    <input
      class="query"
      placeholder={t("ph_query")}
      bind:value={query}
      disabled={aiLoading}
      onkeydown={(e) => e.key === "Enter" && askAi()}
    />
    <button class="icon-btn" title={t("ask_ai")} onclick={askAi} disabled={aiLoading} aria-label={t("ask_ai")}><Icon svg={icSend} size={16} /></button>
    <button class="icon-btn" title={t("history")} onclick={openHistory} aria-label={t("history")}><Icon svg={icHistory} size={17} /></button>
    <button class="icon-btn" title={t("settings")} onclick={openSettings} aria-label={t("settings")}><Icon svg={icSettings} size={17} /></button>
  </div>

  <div class="context" data-tauri-drag-region>
    {#if context.length === 0}
      <span class="muted" data-tauri-drag-region>{t("ctx_empty")}</span>
    {:else}
      <span class="ctx-count">{context.length} {t("files_n")}:</span>
      {#each context.slice(0, 4) as path}
        <span class="chip">{basename(path)}</span>
      {/each}
      {#if context.length > 4}<span class="muted">+{context.length - 4}</span>{/if}
    {/if}
  </div>

  {#if !ffmpegOk}
    <div class="banner warn">
      <span>{t("ffmpeg_missing")}</span>
      {#if downloading}
        <span class="muted">{downloadMsg}{downloadPct !== null ? ` (${downloadPct.toFixed(0)}%)` : ""}</span>
      {:else}
        <button class="mini-btn" onclick={downloadFfmpeg}>{t("ffmpeg_download")}</button>
      {/if}
    </div>
  {/if}

  {#if notice}
    <div class="banner info" role="status">
      {notice}
      <button class="close" onclick={() => (notice = "")} aria-label="×"><Icon svg={icClose} size={15} /></button>
    </div>
  {/if}

  <div class="body">
    {#if view === "list"}
      {#if recs.length === 0}
        <div class="empty">
          {#if context.length === 0}{t("waiting_files")}{:else}{t("no_recs")}{/if}
        </div>
      {:else}
        <div class="grid">
          {#each recs as p}
            <button class="card" onclick={() => selectPreset(p)}>
              <div class="card-title">{p.label}</div>
              <div class="card-desc">{p.description}</div>
              <div class="card-ext">→ .{p.target_ext}</div>
            </button>
          {/each}
        </div>
      {/if}
    {:else if view === "confirm"}
      <div class="confirm">
        <div class="confirm-head">
          {#if confirmMode === "ai" && aiSuggestion}
            <strong>{t("ai_suggests")}</strong>
            <span class="muted">→ .{aiSuggestion.target_ext}</span>
          {:else if selected}
            <strong>{selected.label}</strong>
          {/if}
          <span class="muted">· {context.length} {t("files_n")}</span>
        </div>
        {#if confirmMode === "ai" && aiSuggestion?.explanation}
          <div class="explanation">{aiSuggestion.explanation}</div>
        {/if}
        <div class="preview">
          {#each previews.slice(0, 6) as line}<code>{line}</code>{/each}
          {#if previews.length > 6}<span class="muted">…+{previews.length - 6}</span>{/if}
        </div>
        <div class="actions">
          <button class="btn ghost" onclick={() => (view = "list")}>{t("back")}</button>
          <button class="btn primary" onclick={runConfirm}>{t("convert")}</button>
        </div>
      </div>
    {:else if view === "running"}
      <div class="running">
        <div class="status">
          {#if retrying}
            {t("ai_fixing")} ({aiAttempt + 1}/{MAX_AI_ATTEMPTS})…
          {:else if finished}
            {t("sum_done")} {doneCount} {t("sum_ok")}{errorCount ? `, ${errorCount} ${t("sum_err")}` : ""}{cancelledCount ? `, ${cancelledCount} ${t("sum_cancel")}` : ""}.
          {:else}
            {t("converting")} {doneCount + errorCount + cancelledCount}/{totalJobs}
          {/if}
        </div>
        <div class="log">
          {#each logLines as l}<div class="log-line">{l}</div>{/each}
        </div>
        <div class="actions">
          {#if finished}
            <button class="btn ghost" onclick={refresh}>{t("btn_done")}</button>
            <button class="btn primary" onclick={() => invoke("hide_palette")}>{t("btn_close")}</button>
          {:else}
            <button class="btn ghost" onclick={cancel}>{t("btn_cancel")}</button>
          {/if}
        </div>
      </div>
    {:else if view === "history"}
      <div class="history">
        {#if history.length === 0}
          <div class="empty">{t("h_empty")}</div>
        {:else}
          {#each history as h}
            <button class="hist-row" onclick={() => useHistoryEntry(h)}>
              <span class="hist-kind"><Icon svg={h.kind === "ai" ? icWand : icPlay} size={15} /></span>
              <span class="hist-label">{h.label}</span>
              <span class="hist-meta">.{h.target_ext} · {h.files} {t("h_files")} · {fmtDate(h.ts)}</span>
            </button>
          {/each}
        {/if}
        <div class="actions">
          <button class="btn ghost" onclick={() => (view = "list")}>{t("back")}</button>
          {#if history.length > 0}
            <button class="btn ghost" onclick={clearHistory}>{t("h_clear")}</button>
          {/if}
        </div>
      </div>
    {:else if view === "settings"}
      <div class="settings">
        <label class="field">
          <span>{t("s_provider")}</span>
          <select bind:value={settings.provider}>
            <option value="openrouter">{t("s_or")}</option>
            <option value="ollama">{t("s_ollama")}</option>
          </select>
        </label>

        {#if settings.provider === "openrouter"}
          <label class="field">
            <span>{t("s_or_model")}</span>
            <input bind:value={settings.openrouter_model} placeholder="openai/gpt-4o-mini" />
          </label>
          <label class="field">
            <span>{t("s_or_key")} {apiKeyPresent ? t("s_saved_mark") : ""}</span>
            <input type="password" bind:value={apiKeyInput} placeholder={apiKeyPresent ? t("s_key_ph_saved") : "sk-or-…"} />
          </label>
        {:else}
          <label class="field">
            <span>{t("s_ollama_url")}</span>
            <input bind:value={settings.ollama_url} placeholder="http://localhost:11434" />
          </label>
          <label class="field">
            <span>{t("s_ollama_model")}</span>
            <input bind:value={settings.ollama_model} placeholder="qwen2.5:7b" />
          </label>
        {/if}

        <div class="row2">
          <label class="field">
            <span>{t("s_outdir")}</span>
            <input
              value={settings.output_dir ?? ""}
              oninput={(e) => (settings.output_dir = e.currentTarget.value.trim() || null)}
              placeholder={t("s_outdir_ph")}
            />
          </label>
          <label class="field">
            <span>{t("s_template")}</span>
            <input bind:value={settings.filename_template} placeholder={t("s_template_ph")} />
          </label>
        </div>

        <div class="row2">
          <label class="field">
            <span>{t("s_theme")}</span>
            <select bind:value={settings.theme}>
              <option value="system">{t("s_theme_system")}</option>
              <option value="dark">{t("s_theme_dark")}</option>
              <option value="light">{t("s_theme_light")}</option>
            </select>
          </label>
          <label class="field">
            <span>{t("s_lang")}</span>
            <select bind:value={settings.language}>
              <option value="ru">Русский</option>
              <option value="en">English</option>
            </select>
          </label>
        </div>

        <div class="field">
          <span>{t("s_hotkey")}</span>
          <button
            type="button"
            class="hotkey-btn"
            class:capturing={capturingHotkey}
            onclick={() => (capturingHotkey = !capturingHotkey)}
            onkeydown={onHotkeyCapture}
            onblur={() => (capturingHotkey = false)}
          >
            <Icon svg={icKeyboard} size={16} />
            {capturingHotkey ? t("s_hotkey_press") : fmtHotkey(settings.hotkey)}
          </button>
        </div>

        <label class="check">
          <input type="checkbox" bind:checked={ctxMenuOn} />
          <span>{t("s_ctxmenu")} <em class="muted">{t("s_ctxmenu_hint")}</em></span>
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={autostartOn} />
          <span>{t("s_autostart")}</span>
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={settings.hide_on_blur} />
          <span>{t("s_hideblur")} <em class="muted">{t("s_hideblur_hint")}</em></span>
        </label>

        <div class="actions">
          <button class="btn ghost" onclick={() => (view = "list")}>{t("s_cancel")}</button>
          <button class="btn primary" onclick={saveSettings}>{t("s_save")}</button>
        </div>
      </div>
    {/if}
  </div>

  <div class="footer" data-tauri-drag-region>
    <span class="muted" data-tauri-drag-region>{t("footer_hint")}</span>
    <span class="brand" data-tauri-drag-region>ConvPalette</span>
  </div>

  {#if dragActive}
    <div class="drop-hint">{t("drop_hint")}</div>
  {/if}
</div>

<style>
  /* Палитра тем: тёмная — по умолчанию, светлая — через data-theme=light. */
  :global(:root) {
    --panel-bg: rgba(28, 28, 32, 0.98);
    --panel-border: rgba(255, 255, 255, 0.08);
    --divider: rgba(255, 255, 255, 0.06);
    --text: #eaeaf0;
    --text-strong: #ffffff;
    --text-muted: rgba(255, 255, 255, 0.4);
    --text-soft: rgba(255, 255, 255, 0.5);
    --chip-bg: rgba(255, 255, 255, 0.08);
    --surface: rgba(255, 255, 255, 0.04);
    --surface-border: rgba(255, 255, 255, 0.07);
    --surface-hover: rgba(120, 130, 255, 0.14);
    --input-bg: rgba(255, 255, 255, 0.06);
    --input-border: rgba(255, 255, 255, 0.1);
    --btn-bg: rgba(255, 255, 255, 0.06);
    --btn-bg-hover: rgba(255, 255, 255, 0.14);
    --btn-fg: #cfcfe0;
    --ghost-bg: rgba(255, 255, 255, 0.08);
    --ghost-bg-hover: rgba(255, 255, 255, 0.16);
    --ghost-fg: #dcdcea;
    --accent: #5b64ff;
    --accent-hover: #6f77ff;
    --accent-soft: rgba(120, 130, 255, 0.12);
    --accent-fg: #cdd4ff;
    --link: #8fb4ff;
    --ok: #b8f0c8;
    --log-bg: rgba(0, 0, 0, 0.35);
    --warn-bg: rgba(180, 120, 20, 0.18);
    --warn-fg: #f2c879;
    --info-bg: rgba(70, 100, 220, 0.18);
    --info-fg: #b9c6ff;
    --shadow: 0 18px 60px rgba(0, 0, 0, 0.55);
  }
  :global(:root[data-theme="light"]) {
    --panel-bg: rgba(248, 248, 252, 0.99);
    --panel-border: rgba(0, 0, 0, 0.1);
    --divider: rgba(0, 0, 0, 0.07);
    --text: #26262e;
    --text-strong: #101016;
    --text-muted: rgba(0, 0, 0, 0.4);
    --text-soft: rgba(0, 0, 0, 0.5);
    --chip-bg: rgba(0, 0, 0, 0.07);
    --surface: rgba(0, 0, 0, 0.03);
    --surface-border: rgba(0, 0, 0, 0.08);
    --surface-hover: rgba(91, 100, 255, 0.1);
    --input-bg: rgba(0, 0, 0, 0.04);
    --input-border: rgba(0, 0, 0, 0.12);
    --btn-bg: rgba(0, 0, 0, 0.05);
    --btn-bg-hover: rgba(0, 0, 0, 0.1);
    --btn-fg: #3a3a46;
    --ghost-bg: rgba(0, 0, 0, 0.06);
    --ghost-bg-hover: rgba(0, 0, 0, 0.12);
    --ghost-fg: #2e2e38;
    --accent: #4a53f0;
    --accent-hover: #5b64ff;
    --accent-soft: rgba(91, 100, 255, 0.1);
    --accent-fg: #3a43c8;
    --link: #3556c9;
    --ok: #1d7a3d;
    --log-bg: rgba(0, 0, 0, 0.05);
    --warn-bg: rgba(200, 140, 20, 0.14);
    --warn-fg: #8a5f08;
    --info-bg: rgba(70, 100, 220, 0.12);
    --info-fg: #2f47b0;
    --shadow: 0 18px 60px rgba(0, 0, 0, 0.25);
  }

  :global(body) {
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .palette {
    position: relative;
    width: 640px;
    max-height: 460px;
    display: flex;
    flex-direction: column;
    background: var(--panel-bg);
    color: var(--text);
    border: 1px solid var(--panel-border);
    border-radius: 14px;
    box-shadow: var(--shadow);
    overflow: hidden;
    font-family: "Segoe UI", Inter, system-ui, sans-serif;
    font-size: 14px;
  }
  .palette.drag { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent), var(--shadow); }
  .palette.pop { animation: pop-in 0.16s ease-out; }
  @keyframes pop-in {
    from { opacity: 0; transform: scale(0.97) translateY(8px); }
    to { opacity: 1; transform: none; }
  }
  @media (prefers-reduced-motion: reduce) {
    .palette.pop { animation: none; }
  }
  .drop-hint {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(91, 100, 255, 0.18);
    color: var(--accent-fg);
    font-size: 16px;
    font-weight: 600;
    pointer-events: none;
    z-index: 5;
  }
  .topbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--divider);
    cursor: default;
  }
  .search-icon { display: inline-flex; align-items: center; opacity: 0.8; color: var(--accent-fg); }
  .query {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    color: var(--text-strong);
    font-size: 15px;
  }
  .query::placeholder { color: var(--text-muted); }
  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: var(--btn-bg);
    border: none;
    color: var(--btn-fg);
    width: 30px;
    height: 30px;
    border-radius: 8px;
    cursor: pointer;
    font-size: 15px;
  }
  .icon-btn:hover { background: var(--btn-bg-hover); }
  .icon-btn:disabled { opacity: 0.5; cursor: default; }
  .context {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    padding: 8px 14px;
    border-bottom: 1px solid var(--divider);
  }
  .ctx-count { color: var(--link); font-weight: 600; }
  .chip {
    background: var(--chip-bg);
    padding: 2px 8px;
    border-radius: 6px;
    font-size: 12px;
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .muted { color: var(--text-muted); }
  .banner {
    padding: 8px 14px;
    font-size: 13px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .banner.warn { background: var(--warn-bg); color: var(--warn-fg); }
  .banner.info { background: var(--info-bg); color: var(--info-fg); }
  .mini-btn {
    background: var(--btn-bg-hover);
    border: none;
    color: var(--text-strong);
    padding: 3px 10px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 12px;
  }
  .mini-btn:hover { background: var(--ghost-bg-hover); }
  .close { margin-left: auto; background: none; border: none; color: inherit; cursor: pointer; display: inline-flex; align-items: center; }
  .body { flex: 1; overflow-y: auto; padding: 12px 14px; }
  .empty { text-align: center; color: var(--text-muted); padding: 30px 0; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .card {
    text-align: left;
    background: var(--surface);
    border: 1px solid var(--surface-border);
    border-radius: 10px;
    padding: 12px;
    cursor: pointer;
    color: inherit;
    transition: background 0.12s, border-color 0.12s, transform 0.08s;
  }
  .card:hover {
    background: var(--surface-hover);
    border-color: var(--accent);
    transform: translateY(-1px);
  }
  .card-title { font-weight: 600; font-size: 14px; }
  .card-desc { color: var(--text-soft); font-size: 12px; margin-top: 3px; }
  .card-ext { margin-top: 8px; font-size: 12px; color: var(--link); font-family: ui-monospace, monospace; }
  .confirm-head { display: flex; align-items: baseline; gap: 8px; margin-bottom: 10px; }
  .explanation {
    background: var(--accent-soft);
    border-radius: 8px;
    padding: 8px 10px;
    margin-bottom: 10px;
    font-size: 13px;
    color: var(--accent-fg);
  }
  .preview {
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: var(--log-bg);
    border-radius: 8px;
    padding: 10px;
    max-height: 180px;
    overflow-y: auto;
  }
  .preview code {
    font-family: ui-monospace, monospace;
    font-size: 12px;
    color: var(--ok);
    white-space: pre-wrap;
    word-break: break-all;
  }
  .running .status { font-weight: 600; margin-bottom: 8px; }
  .log {
    background: var(--log-bg);
    border-radius: 8px;
    padding: 8px 10px;
    max-height: 220px;
    overflow-y: auto;
    font-family: ui-monospace, monospace;
    font-size: 12px;
  }
  .log-line { color: var(--text-soft); white-space: pre-wrap; word-break: break-all; }
  .history { display: flex; flex-direction: column; gap: 6px; }
  .hist-row {
    display: flex;
    align-items: baseline;
    gap: 8px;
    text-align: left;
    background: var(--surface);
    border: 1px solid var(--surface-border);
    border-radius: 8px;
    padding: 8px 10px;
    cursor: pointer;
    color: inherit;
    font-size: 13px;
  }
  .hist-row:hover { background: var(--surface-hover); border-color: var(--accent); }
  .hist-kind { flex-shrink: 0; }
  .hist-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hist-meta { flex-shrink: 0; color: var(--text-muted); font-size: 11px; }
  .settings { display: flex; flex-direction: column; gap: 12px; }
  .row2 { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .field { display: flex; flex-direction: column; gap: 4px; font-size: 13px; }
  .field span { color: var(--text-soft); }
  .field input, .field select {
    background: var(--input-bg);
    border: 1px solid var(--input-border);
    border-radius: 8px;
    padding: 8px 10px;
    color: var(--text-strong);
    font-size: 14px;
    outline: none;
  }
  .field input:focus, .field select:focus { border-color: var(--accent); }
  .hotkey-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--input-bg);
    border: 1px solid var(--input-border);
    border-radius: 8px;
    padding: 8px 10px;
    color: var(--text-strong);
    font-size: 14px;
    cursor: pointer;
    text-align: left;
  }
  .hotkey-btn:hover { border-color: var(--accent); }
  .hotkey-btn.capturing {
    border-color: var(--accent);
    color: var(--accent-fg);
    background: var(--accent-soft);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    cursor: pointer;
  }
  .check input { accent-color: var(--accent); }
  .check em { font-style: normal; font-size: 11px; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 12px; }
  .btn { padding: 8px 16px; border-radius: 8px; border: none; cursor: pointer; font-size: 14px; font-weight: 500; }
  .btn.primary { background: var(--accent); color: #fff; }
  .btn.primary:hover { background: var(--accent-hover); }
  .btn.ghost { background: var(--ghost-bg); color: var(--ghost-fg); }
  .btn.ghost:hover { background: var(--ghost-bg-hover); }
  .footer {
    display: flex;
    justify-content: space-between;
    padding: 8px 14px;
    border-top: 1px solid var(--divider);
    font-size: 11px;
  }
  .brand { color: var(--text-muted); font-weight: 600; letter-spacing: 0.5px; }
</style>

