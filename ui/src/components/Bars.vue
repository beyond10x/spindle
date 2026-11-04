<script setup lang="ts">
import { computed } from 'vue';

const props = defineProps<{ bounds: readonly number[]; counts: readonly number[]; unit?: string }>();

const WIDTH = 32;
const HEIGHT = 60;

const bars = computed(() => {
  const top = Math.max(...props.counts, 1);
  return props.counts.map((count, i) => {
    const h = (count / top) * HEIGHT;
    const bound = props.bounds[i];
    return {
      count,
      label: bound === undefined ? `>${props.bounds[props.bounds.length - 1] ?? 0}` : `≤${bound}`,
      x: i * (WIDTH + 6),
      y: HEIGHT - h,
      h,
    };
  });
});
</script>

<template>
  <svg
    data-test="bars"
    :width="bars.length * (WIDTH + 6)"
    :height="HEIGHT + 30"
    role="img"
  >
    <g v-for="bar in bars" :key="bar.label">
      <rect :x="bar.x" :y="bar.y" :width="WIDTH" :height="bar.h" class="bar" />
      <text :x="bar.x + WIDTH / 2" :y="HEIGHT + 12" text-anchor="middle">{{ bar.label }}</text>
      <text :x="bar.x + WIDTH / 2" :y="HEIGHT + 25" text-anchor="middle" class="count">{{ bar.count }}</text>
    </g>
    <text v-if="unit" :x="0" :y="10" class="unit">{{ unit }}</text>
  </svg>
</template>

<style scoped>
.bar {
  fill: var(--accent);
}
text {
  font-size: 10px;
  fill: var(--muted);
}
.count {
  fill: var(--fg);
}
</style>
