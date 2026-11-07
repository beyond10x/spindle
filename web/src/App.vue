<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { register } from './register.ts';

interface Subject {
  command: string;
  outcome: string;
}

interface Model {
  subjects: Subject[];
}

interface Published {
  component: string;
  command: string;
  event: string;
  payload: Record<string, unknown>;
}

const status = ref('registering the Service Worker');
const commands = ref<string[]>([]);
const events = ref<Published[]>([]);

async function read<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, init);
  if (!response.ok) {
    throw new Error(`${path} answered ${response.status}: ${await response.text()}`);
  }
  return (await response.json()) as T;
}

async function refresh(): Promise<void> {
  const model = await read<Model>('api/model');
  commands.value = [...new Set(model.subjects.map((subject) => subject.command))];
  events.value = (await read<{ events: Published[] }>('api/events')).events;
}

async function reset(): Promise<void> {
  await read<{ emptied: string[] }>('api/reset', { method: 'POST' });
  await refresh();
}

onMounted(async () => {
  try {
    await register();
    await refresh();
    status.value = 'answered in the browser';
  } catch (error) {
    status.value = String(error);
  }
});
</script>

<template>
  <main>
    <h1>Spindle</h1>
    <p class="status">{{ status }}</p>
    <section>
      <h2>Commands</h2>
      <ul>
        <li v-for="command in commands" :key="command">{{ command }}</li>
      </ul>
    </section>
    <section>
      <h2>Events</h2>
      <button type="button" @click="refresh">Refresh</button>
      <button type="button" @click="reset">Reset</button>
      <ol>
        <li v-for="(published, index) in events" :key="index">
          {{ published.component }} {{ published.event }} {{ JSON.stringify(published.payload) }}
        </li>
      </ol>
    </section>
  </main>
</template>
