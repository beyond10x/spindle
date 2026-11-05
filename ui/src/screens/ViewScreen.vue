<script setup lang="ts">
import { computed } from 'vue';
import RallyBoard from '../components/RallyBoard.vue';
import { isMatchState } from '../lib/board';
import type { ViewAnswer, ViewRow } from '../api/types';

const props = defineProps<{ component: string; view: string; answer: ViewAnswer; error?: string | null }>();

// An instance's ingress answers a list view as `{"rows": [...]}`; a fixture may hold one row or a
// list of rows.
const rowsOf = (answer: ViewAnswer): ViewRow[] => {
  if (answer === null) return [];
  if (Array.isArray(answer)) return answer;
  const keys = Object.keys(answer);
  if (keys.length === 1 && keys[0] === 'rows' && Array.isArray(answer.rows)) return answer.rows as ViewRow[];
  return [answer];
};
const rows = computed(() => rowsOf(props.answer));
const columns = computed(() => [...new Set(rows.value.flatMap((row) => Object.keys(row)))]);
const cell = (value: unknown) =>
  value === null || value === undefined ? '' : typeof value === 'object' ? JSON.stringify(value) : String(value);
</script>

<template>
  <section class="view" data-test="view-screen">
    <h2>{{ view }} <span class="muted">· {{ component }}</span></h2>
    <p v-if="error" class="error">{{ error }}</p>
    <template v-else>
      <RallyBoard v-for="(row, i) in isMatchState(view) ? rows : []" :key="i" :view="row" />
      <p v-if="rows.length === 0" class="muted">the view answers nothing</p>
      <table v-else data-test="view-table">
        <thead>
          <tr><th v-for="c in columns" :key="c">{{ c }}</th></tr>
        </thead>
        <tbody>
          <tr v-for="(row, i) in rows" :key="i">
            <td v-for="c in columns" :key="c">{{ cell(row[c]) }}</td>
          </tr>
        </tbody>
      </table>
    </template>
  </section>
</template>

<style scoped>
table {
  border-collapse: collapse;
  font-size: 12px;
  margin-top: 12px;
}
td,
th {
  border: 1px solid var(--border);
  padding: 2px 6px;
  text-align: left;
}
.muted {
  color: var(--muted);
}
.error {
  color: var(--bad);
}
</style>
