<!-- Плеер видео/аудио в стиле палитры: свои кнопки вместо системных controls.
     Для аудио перемотка идёт по волновой форме (png от showwavespic), для видео —
     по обычной полосе. Клавиши: пробел — пауза, ←/→ — ±5 c, ↑/↓ — громкость. -->
<script lang="ts">
  import Icon from "$lib/Icon.svelte";
  import { t } from "$lib/i18n.svelte";
  import icPlay from "@material-symbols/svg-400/outlined/play_arrow.svg?raw";
  import icPause from "@material-symbols/svg-400/outlined/pause.svg?raw";
  import icReplay from "@material-symbols/svg-400/outlined/replay.svg?raw";
  import icVolUp from "@material-symbols/svg-400/outlined/volume_up.svg?raw";
  import icVolOff from "@material-symbols/svg-400/outlined/volume_off.svg?raw";

  let {
    src,
    mode,
    wave = null,
    duration = null,
    onerror,
  }: {
    src: string;
    mode: "video" | "audio";
    wave?: string | null;
    duration?: number | null;
    onerror?: () => void;
  } = $props();

  const SEEK_STEP = 5;

  let media = $state<HTMLMediaElement | null>(null);
  let root = $state<HTMLElement | null>(null);
  let playing = $state(false);
  let ended = $state(false);
  let cur = $state(0);
  let metaDur = $state(0);
  let vol = $state(1);
  let muted = $state(false);

  // Пока не пришли метаданные, берём длительность из карточки (ffprobe).
  let dur = $derived(metaDur > 0 ? metaDur : (duration ?? 0));

  let pct = $derived(dur > 0 ? Math.min(100, (cur / dur) * 100) : 0);

  function fmt(s: number): string {
    if (!isFinite(s) || s < 0) s = 0;
    const total = Math.floor(s);
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    const sec = total % 60;
    const mm = h > 0 ? String(m).padStart(2, "0") : String(m);
    return `${h > 0 ? `${h}:` : ""}${mm}:${String(sec).padStart(2, "0")}`;
  }

  function toggle() {
    if (!media) return;
    if (media.paused) media.play().catch(() => {});
    else media.pause();
  }

  function seekTo(sec: number) {
    if (!media || !isFinite(dur) || dur <= 0) return;
    media.currentTime = Math.max(0, Math.min(dur - 0.05, sec));
    ended = false;
  }

  function nudge(delta: number) {
    seekTo((media?.currentTime ?? 0) + delta);
  }

  function setVolume(v: number) {
    vol = Math.max(0, Math.min(1, v));
    if (media) {
      media.volume = vol;
      media.muted = vol === 0 ? true : false;
    }
    muted = vol === 0;
  }

  function toggleMute() {
    if (!media) return;
    muted = !muted;
    media.muted = muted;
  }

  /// Общая логика «тянуть по дорожке»: доля от ширины элемента под курсором.
  function trackRatio(e: PointerEvent): number {
    const el = e.currentTarget as HTMLElement;
    const r = el.getBoundingClientRect();
    return Math.max(0, Math.min(1, (e.clientX - r.left) / r.width));
  }

  function onSeekDown(e: PointerEvent) {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    seekTo(trackRatio(e) * dur);
  }
  function onSeekMove(e: PointerEvent) {
    if (e.buttons !== 1) return;
    seekTo(trackRatio(e) * dur);
  }

  function onVolDown(e: PointerEvent) {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    setVolume(trackRatio(e));
  }
  function onVolMove(e: PointerEvent) {
    if (e.buttons !== 1) return;
    setVolume(trackRatio(e));
  }

  function onKey(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement | null)?.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA") return;
    if (e.key === "ArrowRight") nudge(SEEK_STEP);
    else if (e.key === "ArrowLeft") nudge(-SEEK_STEP);
    else if (e.key === "ArrowUp") setVolume(vol + 0.05);
    else if (e.key === "ArrowDown") setVolume(vol - 0.05);
    else if (e.key === " " || e.key === "Spacebar") toggle();
    else return;
    e.preventDefault();
  }

  // Забрать фокус у поля ввода палитры, чтобы стрелки работали сразу.
  $effect(() => {
    root?.focus({ preventScroll: true });
  });
</script>

<svelte:window onkeydown={onKey} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="player" bind:this={root} tabindex="-1">
  {#if mode === "video"}
    <!-- svelte-ignore a11y_media_has_caption, a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="stage" onclick={toggle}>
      <video
        bind:this={media}
        {src}
        autoplay
        onplay={() => ((playing = true), (ended = false))}
        onpause={() => (playing = false)}
        onended={() => ((playing = false), (ended = true))}
        ontimeupdate={() => (cur = media?.currentTime ?? 0)}
        onloadedmetadata={() => {
          const d = media?.duration ?? 0;
          if (isFinite(d) && d > 0) metaDur = d;
          if (media) media.volume = vol;
        }}
        onerror={() => onerror?.()}
      ></video>
      {#if !playing}
        <span class="big-play"><Icon svg={ended ? icReplay : icPlay} size={34} /></span>
      {/if}
    </div>
  {:else}
    <audio
      bind:this={media}
      {src}
      autoplay
      onplay={() => ((playing = true), (ended = false))}
      onpause={() => (playing = false)}
      onended={() => ((playing = false), (ended = true))}
      ontimeupdate={() => (cur = media?.currentTime ?? 0)}
      onloadedmetadata={() => {
        const d = media?.duration ?? 0;
        if (isFinite(d) && d > 0) metaDur = d;
        if (media) media.volume = vol;
      }}
      onerror={() => onerror?.()}
    ></audio>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="wave" onpointerdown={onSeekDown} onpointermove={onSeekMove}>
      {#if wave}
        <img class="wave-img" src={wave} alt="" />
        <img class="wave-img played" src={wave} alt="" style={`clip-path: inset(0 ${100 - pct}% 0 0)`} />
      {:else}
        <div class="wave-flat"></div>
        <div class="wave-flat played" style={`width:${pct}%`}></div>
      {/if}
      <span class="wave-cursor" style={`left:${pct}%`}></span>
    </div>
  {/if}

  <div class="bar">
    <button class="pbtn" onclick={toggle} aria-label={playing ? t("pv_pause") : t("pv_play")}>
      <Icon svg={ended ? icReplay : playing ? icPause : icPlay} size={19} />
    </button>

    {#if mode === "video"}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="track" onpointerdown={onSeekDown} onpointermove={onSeekMove}>
        <div class="track-fill" style={`width:${pct}%`}></div>
        <span class="track-knob" style={`left:${pct}%`}></span>
      </div>
    {/if}

    <span class="time">{fmt(cur)} <i>/</i> {fmt(dur)}</span>

    <button class="pbtn small" onclick={toggleMute} aria-label={muted ? t("pv_unmute") : t("pv_mute")}>
      <Icon svg={muted || vol === 0 ? icVolOff : icVolUp} size={17} />
    </button>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="track vol" onpointerdown={onVolDown} onpointermove={onVolMove}>
      <div class="track-fill" style={`width:${muted ? 0 : vol * 100}%`}></div>
    </div>
  </div>
</div>

<style>
  .player {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 10px;
    outline: none;
  }
  .stage {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    background: #000;
    border-radius: 10px;
    overflow: hidden;
    cursor: pointer;
  }
  .stage video {
    max-width: 100%;
    max-height: 300px;
    display: block;
  }
  .big-play {
    position: absolute;
    width: 62px;
    height: 62px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.5);
    color: #fff;
    backdrop-filter: blur(2px);
    pointer-events: none;
  }

  /* ── Волна аудио: она же полоса перемотки ── */
  .wave {
    position: relative;
    width: 100%;
    height: 84px;
    cursor: pointer;
    touch-action: none;
    border-radius: 8px;
    background: var(--surface);
    overflow: hidden;
  }
  .wave-img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0.3;
    pointer-events: none;
  }
  .wave-img.played {
    opacity: 1;
  }
  .wave-flat {
    position: absolute;
    top: 50%;
    left: 0;
    width: 100%;
    height: 4px;
    margin-top: -2px;
    border-radius: 2px;
    background: var(--chip-bg);
  }
  .wave-flat.played {
    background: var(--accent);
  }
  .wave-cursor {
    position: absolute;
    top: 4px;
    bottom: 4px;
    width: 2px;
    margin-left: -1px;
    border-radius: 1px;
    background: var(--text-strong);
    opacity: 0.75;
    pointer-events: none;
  }

  /* ── Нижняя панель управления ── */
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
  }
  .pbtn {
    flex: none;
    width: 34px;
    height: 34px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: 50%;
    background: var(--accent);
    color: #fff;
    cursor: pointer;
    transition: background 0.12s;
  }
  .pbtn:hover {
    background: var(--accent-hover);
  }
  .pbtn.small {
    width: 28px;
    height: 28px;
    background: var(--btn-bg);
    color: var(--btn-fg);
  }
  .pbtn.small:hover {
    background: var(--btn-bg-hover);
  }
  .track {
    position: relative;
    flex: 1;
    height: 14px;
    display: flex;
    align-items: center;
    cursor: pointer;
    touch-action: none;
  }
  .track::before {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    height: 4px;
    border-radius: 2px;
    background: var(--chip-bg);
  }
  .track-fill {
    position: absolute;
    left: 0;
    height: 4px;
    border-radius: 2px;
    background: var(--accent);
  }
  .track-knob {
    position: absolute;
    width: 10px;
    height: 10px;
    margin-left: -5px;
    border-radius: 50%;
    background: var(--text-strong);
    box-shadow: 0 0 0 2px var(--accent);
    pointer-events: none;
  }
  .track.vol {
    flex: none;
    width: 62px;
  }
  .time {
    flex: none;
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
    color: var(--text-soft);
    white-space: nowrap;
  }
  .time i {
    font-style: normal;
    opacity: 0.5;
  }
</style>
