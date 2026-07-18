<!-- Инлайн-иконка Material Symbols (Google Fonts): svg — содержимое файла из
     @material-symbols/svg-400, импортированное как ?raw. Красится в currentColor. -->
<script lang="ts">
  let {
    svg,
    size = 18,
    spin = false,
  }: { svg: string; size?: number; spin?: boolean } = $props();
</script>

<span class="icon" class:spin style={`--icon-size:${size}px`}>{@html svg}</span>

<style>
  .icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    /* События мыши уходят родителю: кнопки кликаются, drag-region окна работает. */
    pointer-events: none;
  }
  .icon :global(svg) {
    width: var(--icon-size);
    height: var(--icon-size);
    fill: currentColor;
  }
  .spin {
    animation: icon-spin 0.9s linear infinite;
  }
  @keyframes icon-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
