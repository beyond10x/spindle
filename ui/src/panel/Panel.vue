<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue';
import Banner from './Banner.vue';
import { requestsUrl } from './requests';
import type { RequestLog } from '../api/types';

const props = withDefaults(defineProps<{ fetch?: typeof fetch; every?: number }>(), {
  fetch: undefined,
  every: 2000,
});

const fetcher: typeof fetch = props.fetch ?? ((input, init) => globalThis.fetch(input, init));
const log = shallowRef<RequestLog | null>(null);
const notServedOnly = ref(false);
const error = ref<string | null>(null);
let polling: ReturnType<typeof setInterval> | undefined;

let asked = 0;

/** Only the answer of the newest call is kept; an older one, however late, is dropped. */
async function load() {
  const mine = ++asked;
  try {
    const response = await fetcher(requestsUrl(notServedOnly.value), {
      headers: { accept: 'application/json' },
      cache: 'no-store',
    });
    if (!response.ok) throw new Error(`the request log answered ${response.status}`);
    const answer = (await response.json()) as RequestLog;
    if (mine !== asked) return;
    log.value = answer;
    error.value = null;
  } catch (cause) {
    if (mine !== asked) return;
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

const time = (at: string): string => at.slice(11, 23);

watch(notServedOnly, () => void load());
onMounted(() => {
  void load();
  if (props.every > 0) polling = setInterval(() => void load(), props.every);
});
onBeforeUnmount(() => clearInterval(polling));
</script>

<template>
  <main class="panel">
    <header>
      <Banner v-if="log" :totals="log.totals" />
      <p v-else class="banner">no request log yet</p>
      <label><input v-model="notServedOnly" type="checkbox" data-test="not-served-only" /> not served only</label>
      <button type="button" data-test="refresh" @click="load">refresh</button>
    </header>
    <p v-if="error" class="error" data-test="error">{{ error }}</p>
    <table v-if="log">
      <thead>
        <tr>
          <th>#</th>
          <th>at</th>
          <th>method</th>
          <th>path</th>
          <th>status</th>
          <th>class</th>
          <th>outcome or reason</th>
          <th>µs</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="request in log.requests" :key="request.sequence" :class="request.class" data-test="request">
          <td>{{ request.sequence }}</td>
          <td>{{ time(request.at) }}</td>
          <td>{{ request.method }}</td>
          <td>{{ request.path }}</td>
          <td>{{ request.status }}</td>
          <td>{{ request.class }}</td>
          <td>{{ request.outcome ?? request.reason ?? '' }}</td>
          <td class="number">{{ request.duration_us }}</td>
        </tr>
      </tbody>
    </table>
  </main>
</template>
