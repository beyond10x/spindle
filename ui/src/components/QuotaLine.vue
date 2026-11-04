<script setup lang="ts">
import { computed } from 'vue';
import Sparkline from './Sparkline.vue';
import { cpuShares, memoryShares } from '../lib/rates';
import type { Instance, Sample } from '../api/types';

const props = defineProps<{ instance: Instance; samples: readonly Sample[] }>();

const MIB = 1048576;
const latest = computed(() => props.samples[props.samples.length - 1]);
const cpu = computed(() => cpuShares(props.samples, props.instance.requested.vcpus));
const cpuSeconds = computed(() =>
  props.samples.slice(1).map((s, i) => s.resources.cpu_seconds - props.samples[i]!.resources.cpu_seconds),
);
const memory = computed(() => memoryShares(props.samples));
const memoryBytes = computed(() => props.samples.map((s) => s.resources.memory_bytes));
const used = computed(() => ((latest.value?.resources.memory_bytes ?? 0) / MIB).toFixed(0));
const cpuPercent = computed(() => {
  const shares = cpu.value;
  const now = shares?.[shares.length - 1];
  return now === undefined ? '–' : `${(now * 100).toFixed(0)} %`;
});
</script>

<template>
  <div class="quota">
    <div>
      <span class="key">cpu</span>
      <Sparkline :values="cpu ?? cpuSeconds" :max="cpu ? 1 : undefined" />
      <span v-if="cpu">{{ cpuPercent }} of {{ instance.requested.vcpus }}</span>
      <span v-else class="muted">no limit</span>
    </div>
    <div>
      <span class="key">mem</span>
      <Sparkline :values="memory ?? memoryBytes" :max="memory ? 1 : undefined" />
      <span v-if="memory">{{ used }} / {{ instance.requested.memory_mib }} MiB</span>
      <span v-else>{{ used }} MiB <span class="muted">no limit</span></span>
    </div>
  </div>
</template>

<style scoped>
.quota div {
  display: flex;
  gap: 6px;
  align-items: center;
}
.key {
  width: 2.5em;
  color: var(--muted);
}
.muted {
  color: var(--muted);
}
</style>
