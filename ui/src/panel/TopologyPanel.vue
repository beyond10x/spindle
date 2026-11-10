<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, shallowRef } from 'vue';
import { NODE, draw, latestCalls, short, sourceMix, statusMix } from './topology';
import type { Topology } from '../api/types';

const props = withDefaults(defineProps<{ fetch?: typeof fetch; every?: number }>(), {
  fetch: undefined,
  every: 2000,
});

const fetcher: typeof fetch = props.fetch ?? ((input, init) => globalThis.fetch(input, init));
const topology = shallowRef<Topology | null>(null);
const error = ref<string | null>(null);
const drawing = computed(() => (topology.value ? draw(topology.value) : null));
let polling: ReturnType<typeof setInterval> | undefined;
let asked = 0;

/** Only the answer of the newest call is kept; an older one, however late, is dropped. */
async function load() {
  const mine = ++asked;
  try {
    const response = await fetcher('./topology', { headers: { accept: 'application/json' }, cache: 'no-store' });
    if (!response.ok) throw new Error(`the topology answered ${response.status}`);
    const answer = (await response.json()) as Topology;
    if (mine !== asked) return;
    topology.value = answer;
    error.value = null;
  } catch (cause) {
    if (mine !== asked) return;
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

onMounted(() => {
  void load();
  if (props.every > 0) polling = setInterval(() => void load(), props.every);
});
onBeforeUnmount(() => clearInterval(polling));
</script>

<template>
  <section class="topology">
    <p v-if="error" class="error" data-test="error">{{ error }}</p>
    <svg
      v-if="drawing"
      data-test="topology"
      :width="drawing.width"
      :height="drawing.height"
      :viewBox="`0 0 ${drawing.width} ${drawing.height}`"
    >
      <g
        v-for="(drawn, index) in drawing.edges"
        :key="index"
        :class="['edge', drawn.edge.kind]"
        data-test="edge"
        :data-kind="drawn.edge.kind"
        :data-from="drawn.edge.from"
        :data-to="drawn.edge.to"
      >
        <path :d="drawn.d" />
        <text v-if="drawn.edge.traffic" :x="drawn.label.x" :y="drawn.label.y">
          <tspan class="count" data-test="count">{{ drawn.edge.traffic.count }}</tspan>
          <tspan v-if="drawn.edge.kind === 'routes'" dx="6" data-test="statuses">{{
            statusMix(drawn.edge.traffic)
          }}</tspan>
          <tspan v-if="drawn.edge.kind === 'asks'" dx="6" data-test="sources">{{ sourceMix(drawn.edge.traffic) }}</tspan>
          <tspan v-if="drawn.edge.kind === 'routes'" dx="6" class="latest" data-test="latest">{{
            latestCalls(drawn.edge.traffic)
          }}</tspan>
        </text>
      </g>
      <g
        v-for="placed in drawing.nodes"
        :key="placed.node.id"
        :class="['node', placed.node.kind]"
        data-test="node"
        :data-id="placed.node.id"
        :data-y="placed.y"
      >
        <title>{{ placed.node.id }}</title>
        <rect :x="placed.x" :y="placed.y" :width="NODE.width" :height="NODE.height" rx="4" />
        <text :x="placed.x + 8" :y="placed.y + NODE.height / 2 + 4">{{ placed.node.kind }} {{ short(placed.node) }}</text>
      </g>
    </svg>
    <p v-else-if="!error" class="banner">no topology yet</p>
  </section>
</template>
