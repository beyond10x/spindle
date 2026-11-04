<script setup lang="ts">
import { computed } from 'vue';

const props = withDefaults(
  defineProps<{ values: readonly number[]; max?: number; width?: number; height?: number }>(),
  { width: 90, height: 18 },
);

const points = computed(() => {
  const values = props.values.length > 0 ? props.values : [0];
  const top = props.max ?? Math.max(...values, Number.EPSILON);
  const step = values.length > 1 ? props.width / (values.length - 1) : props.width;
  return values
    .map((v, i) => {
      const y = props.height - (Math.min(Math.max(v, 0), top) / top) * (props.height - 2) - 1;
      return `${(i * step).toFixed(1)},${y.toFixed(1)}`;
    })
    .join(' ');
});
</script>

<template>
  <svg
    data-test="sparkline"
    class="sparkline"
    :width="width"
    :height="height"
    :viewBox="`0 0 ${width} ${height}`"
    role="img"
  >
    <polyline :points="points" fill="none" stroke="currentColor" stroke-width="1.5" />
  </svg>
</template>

<style scoped>
.sparkline {
  vertical-align: middle;
  color: var(--accent);
}
</style>
