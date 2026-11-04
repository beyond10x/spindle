<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue';
import { Client, tokenFrom } from './api/client';
import { route } from './lib/route';
import { short } from './lib/topology';
import TopologyScreen from './screens/TopologyScreen.vue';
import InstanceScreen from './screens/InstanceScreen.vue';
import ViewScreen from './screens/ViewScreen.vue';
import type { Catalogue, Deployment, Sample, StateChange, ViewAnswer } from './api/types';

const KEEP = 900;
const client = new Client({ base: '', token: tokenFrom(location.href) });

const deployment = shallowRef<Deployment | null>(null);
const catalogues = shallowRef<Record<string, Catalogue>>({});
const samples = shallowRef<Record<string, Sample[]>>({});
const states = shallowRef<Record<string, string>>({});
const live = ref(false);
const error = ref<string | null>(null);
const hash = ref(location.hash);
const answer = shallowRef<ViewAnswer>(null);
const viewError = ref<string | null>(null);

const current = computed(() => route(hash.value));
const shown = computed<Deployment | null>(() =>
  deployment.value && {
    ...deployment.value,
    instances: deployment.value.instances.map((i) => ({ ...i, state: states.value[i.instance_id] ?? i.state })),
  },
);
const instance = computed(() => {
  const r = current.value;
  return r.screen === 'instance' ? shown.value?.instances.find((i) => i.instance_id === r.instance) : undefined;
});

let streaming: AbortController | null = null;
let polling: ReturnType<typeof setInterval> | undefined;
let reconnect: ReturnType<typeof setTimeout> | undefined;

function onSample(sample: Sample) {
  const kept = samples.value[sample.instance_id] ?? [];
  const last = kept[kept.length - 1];
  if (last && sample.sequence <= last.sequence) return;
  samples.value = { ...samples.value, [sample.instance_id]: [...kept, sample].slice(-KEEP) };
  live.value = true;
}

function onState(change: StateChange) {
  states.value = { ...states.value, [change.instance_id]: change.to };
}

function connect() {
  streaming = new AbortController();
  client
    .stream({ sample: onSample, state: onState }, streaming.signal)
    .catch((e: unknown) => {
      if (!streaming?.signal.aborted) error.value = String(e);
    })
    .finally(() => {
      live.value = false;
      if (!streaming?.signal.aborted) reconnect = setTimeout(connect, 2000);
    });
}

async function loadView() {
  clearInterval(polling);
  const r = current.value;
  if (r.screen !== 'view') return;
  const read = async () => {
    try {
      answer.value = await client.view(r.component, r.view);
      viewError.value = null;
    } catch (e) {
      viewError.value = String(e);
    }
  };
  await read();
  polling = setInterval(read, 1000);
}

const onHash = () => (hash.value = location.hash);

onMounted(async () => {
  window.addEventListener('hashchange', onHash);
  try {
    deployment.value = await client.deployment();
    const loaded = await Promise.all(deployment.value.components.map((c) => client.catalogue(c.name).catch(() => null)));
    catalogues.value = Object.fromEntries(loaded.flatMap((c) => (c ? [[c.component, c]] : [])));
    connect();
  } catch (e) {
    error.value = String(e);
  }
  await loadView();
});

watch(current, loadView);

onBeforeUnmount(() => {
  window.removeEventListener('hashchange', onHash);
  streaming?.abort();
  clearInterval(polling);
  clearTimeout(reconnect);
});
</script>

<template>
  <header class="bar">
    <a href="#/" class="brand">spindle</a>
    <span v-if="deployment">· {{ deployment.system }}</span>
    <nav v-if="deployment">
      <template v-for="c in deployment.components" :key="c.name">
        <a v-for="v in c.views" :key="v" :href="`#/view/${encodeURIComponent(c.name)}/${encodeURIComponent(v)}`">{{ short(v) }}</a>
      </template>
    </nav>
    <span :class="['live', { on: live }]">● {{ live ? 'live' : 'not connected' }}</span>
  </header>
  <main>
    <p v-if="error" class="error">{{ error }}</p>
    <template v-if="shown">
      <TopologyScreen v-if="current.screen === 'topology'" :deployment="shown" :samples="samples" />
      <InstanceScreen
        v-else-if="current.screen === 'instance' && instance"
        :instance="instance"
        :catalogue="catalogues[instance.component] ?? null"
        :samples="samples[instance.instance_id] ?? []"
      />
      <ViewScreen
        v-else-if="current.screen === 'view'"
        :component="current.component"
        :view="current.view"
        :answer="answer"
        :error="viewError"
      />
      <p v-else class="error">no such instance</p>
    </template>
  </main>
</template>

<style scoped>
.bar {
  display: flex;
  gap: 12px;
  align-items: center;
  padding: 8px 16px;
  border-bottom: 1px solid var(--border);
}
.brand {
  font-weight: bold;
  color: inherit;
  text-decoration: none;
}
nav {
  display: flex;
  gap: 10px;
}
.live {
  margin-left: auto;
  color: var(--muted);
}
.live.on {
  color: var(--good);
}
main {
  padding: 16px;
}
.error {
  color: var(--bad);
}
</style>
