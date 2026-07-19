<!-- Просмотр изображения: масштаб колёсиком (к точке под курсором), перетаскивание
     мышью, двойной клик — сброс. Клавиши +/− тоже меняют масштаб. -->
<script lang="ts">
  import Icon from "$lib/Icon.svelte";
  import { t } from "$lib/i18n.svelte";
  import icZoomIn from "@material-symbols/svg-400/outlined/zoom_in.svg?raw";
  import icZoomOut from "@material-symbols/svg-400/outlined/zoom_out.svg?raw";
  import icReset from "@material-symbols/svg-400/outlined/restart_alt.svg?raw";

  let { src, alt = "", onerror }: { src: string; alt?: string; onerror?: () => void } = $props();

  const MIN = 1;
  const MAX = 8;

  let root = $state<HTMLElement | null>(null);
  let box = $state<HTMLElement | null>(null);
  let scale = $state(1);
  let tx = $state(0);
  let ty = $state(0);
  let dragging = $state(false);

  function reset() {
    scale = 1;
    tx = 0;
    ty = 0;
  }

  /// Масштаб вокруг точки (px от центра контейнера), чтобы она осталась под курсором.
  function zoomAt(next: number, px: number, py: number) {
    const s = Math.max(MIN, Math.min(MAX, next));
    if (s === scale) return;
    const k = s / scale;
    tx = px - (px - tx) * k;
    ty = py - (py - ty) * k;
    scale = s;
    if (scale === MIN) {
      tx = 0;
      ty = 0;
    }
  }

  function zoomCentered(next: number) {
    zoomAt(next, 0, 0);
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    const r = box!.getBoundingClientRect();
    const px = e.clientX - (r.left + r.width / 2);
    const py = e.clientY - (r.top + r.height / 2);
    zoomAt(scale * (e.deltaY < 0 ? 1.15 : 1 / 1.15), px, py);
  }

  // wheel навешиваем вручную: нужен passive: false, иначе preventDefault не сработает.
  $effect(() => {
    const el = box;
    if (!el) return;
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  });

  function onDown(e: PointerEvent) {
    if (scale <= MIN) return;
    dragging = true;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function onMove(e: PointerEvent) {
    if (!dragging) return;
    tx += e.movementX;
    ty += e.movementY;
  }
  function onUp() {
    dragging = false;
  }

  // Забрать фокус у поля ввода палитры, чтобы +/− работали сразу.
  $effect(() => {
    root?.focus({ preventScroll: true });
  });

  function onKey(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement | null)?.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA") return;
    if (e.key === "+" || e.key === "=") zoomCentered(scale * 1.25);
    else if (e.key === "-") zoomCentered(scale / 1.25);
    else if (e.key === "0") reset();
    else return;
    e.preventDefault();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="viewer" bind:this={root} tabindex="-1">
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div
    class="box"
    class:grab={scale > MIN}
    class:grabbing={dragging}
    bind:this={box}
    onpointerdown={onDown}
    onpointermove={onMove}
    onpointerup={onUp}
    onpointercancel={onUp}
    ondblclick={reset}
  >
    <img
      {src}
      {alt}
      draggable="false"
      style={`transform: translate(${tx}px, ${ty}px) scale(${scale})`}
      onerror={() => onerror?.()}
    />
  </div>
  <div class="zbar">
    <button class="zbtn" onclick={() => zoomCentered(scale / 1.25)} aria-label="−">
      <Icon svg={icZoomOut} size={17} />
    </button>
    <span class="zval">{Math.round(scale * 100)}%</span>
    <button class="zbtn" onclick={() => zoomCentered(scale * 1.25)} aria-label="+">
      <Icon svg={icZoomIn} size={17} />
    </button>
    <button class="zbtn" onclick={reset} aria-label="100%"><Icon svg={icReset} size={17} /></button>
    <span class="zhint">{t("pv_zoom_hint")}</span>
  </div>
</div>

<style>
  .viewer {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 8px;
    outline: none;
  }
  .box {
    position: relative;
    width: 100%;
    height: 300px;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    border-radius: 10px;
    background: var(--surface);
    touch-action: none;
  }
  .box.grab {
    cursor: grab;
  }
  .box.grabbing {
    cursor: grabbing;
  }
  .box img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    transform-origin: center center;
    will-change: transform;
    user-select: none;
    -webkit-user-drag: none;
  }
  .zbar {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .zbtn {
    width: 26px;
    height: 26px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: 6px;
    background: var(--btn-bg);
    color: var(--btn-fg);
    cursor: pointer;
  }
  .zbtn:hover {
    background: var(--btn-bg-hover);
  }
  .zval {
    min-width: 44px;
    text-align: center;
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
    color: var(--text-soft);
  }
  .zhint {
    flex: 1;
    text-align: right;
    font-size: 11px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
