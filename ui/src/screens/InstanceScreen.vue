<script setup lang="ts">
import { computed } from 'vue';
import Bars from '../components/Bars.vue';
import QuotaLine from '../components/QuotaLine.vue';
import { short } from '../lib/topology';
import { bytes, perSecond } from '../lib/rates';
import { byBinding, rows } from '../lib/summary';
import type { Catalogue, Instance, Sample } from '../api/types';

const props = defineProps<{ instance: Instance; catalogue?: Catalogue | null; samples: readonly Sample[] }>();

const declared = computed(() => props.catalogue?.counters.filter((c) => c.kind === 'counter').map((c) => c.name));
const latest = computed(() => props.samples[props.samples.length - 1]);
const commands = computed(() =>
  rows(props.samples, 'command.', declared.value).map((row) => {
    const rest = row.name.slice('command.'.length);
    const dot = rest.lastIndexOf('.');
    return { ...row, command: short(rest.slice(0, dot)), outcome: rest.slice(dot + 1) };
  }),
);
const events = computed(() => rows(props.samples, 'event.', declared.value));
const store = computed(() => rows(props.samples, 'store.', declared.value));
const channels = computed(() => byBinding(props.samples, 'binding.', declared.value));
const timers = computed(() => byBinding(props.samples, 'timer.', declared.value));
const count = (value: number | undefined) => (value === undefined ? 'never reported' : String(value));
</script>

<template>
  <section class="instance" data-test="instance-screen">
    <header>
      <h2>{{ instance.instance_id }}</h2>
      <p>
        {{ instance.component }} · <strong>{{ instance.state }}</strong> · {{ instance.target_kind }}
        {{ instance.target }} · {{ instance.requested.vcpus }} vCPU, {{ instance.requested.memory_mib }} MiB,
        {{ instance.requested.disk_mib }} MiB disk
        <span v-if="instance.started_at"> · since {{ instance.started_at }}</span>
      </p>
      <p v-if="latest" class="muted">
        sample {{ latest.sequence }} at {{ latest.at }} · every {{ latest.period_ms }} ms ·
        <span :class="{ warn: latest.dropped_frames > 0 }">{{ latest.dropped_frames }} frames dropped</span>
      </p>
      <p v-else class="muted">no sample yet</p>
    </header>

    <QuotaLine :instance="instance" :samples="samples" />

    <div class="grid">
      <section>
        <h3>Commands</h3>
        <table data-test="commands">
          <thead><tr><th>command</th><th>outcome</th><th>count</th><th>rate</th></tr></thead>
          <tbody>
            <tr v-for="row in commands" :key="row.name" :class="{ muted: row.count === undefined }">
              <td>{{ row.command }}</td><td>{{ row.outcome }}</td><td>{{ count(row.count) }}</td><td>{{ perSecond(row.rate) }}</td>
            </tr>
          </tbody>
        </table>
      </section>

      <section>
        <h3>Latency</h3>
        <div v-for="h in latest?.histograms ?? []" :key="h.name" class="histogram">
          <h4>{{ h.name }}</h4>
          <Bars :bounds="h.bounds" :counts="h.counts" />
        </div>

        <h3>Events appended</h3>
        <table data-test="events">
          <tbody>
            <tr v-for="row in events" :key="row.name" :class="{ muted: row.count === undefined }">
              <td>{{ row.name }}</td><td>{{ count(row.count) }}</td><td>{{ perSecond(row.rate) }}</td>
            </tr>
          </tbody>
        </table>

        <h3>Store</h3>
        <table data-test="store">
          <tbody>
            <tr v-for="row in store" :key="row.name">
              <td>{{ row.name }}</td>
              <td>{{ row.name.endsWith('bytes_written') ? bytes(row.count) : count(row.count) }}</td>
              <td>{{ row.name.endsWith('bytes_written') ? `${bytes(row.rate)}/s` : perSecond(row.rate) }}</td>
            </tr>
          </tbody>
        </table>
      </section>

      <section>
        <h3>Channels</h3>
        <table data-test="channels">
          <thead><tr><th>binding</th><th>sent</th><th>delivered</th><th>redelivered</th><th>dropped</th></tr></thead>
          <tbody>
            <tr v-for="[binding, kinds] in channels" :key="binding">
              <td>{{ binding }}</td>
              <td v-for="kind in ['sent', 'delivered', 'redelivered', 'dropped']" :key="kind">
                {{ kinds.get(kind)?.count ?? '' }}
              </td>
            </tr>
          </tbody>
        </table>

        <h3>Timers</h3>
        <table data-test="timers">
          <thead><tr><th>binding</th><th>on time</th><th>late</th><th>dropped</th></tr></thead>
          <tbody>
            <tr v-for="[binding, kinds] in timers" :key="binding">
              <td>{{ binding }}</td>
              <td>{{ (kinds.get('fired')?.count ?? 0) - (kinds.get('late')?.count ?? 0) }}</td>
              <td>{{ kinds.get('late')?.count ?? 0 }}</td>
              <td>{{ kinds.get('dropped')?.count ?? 0 }}</td>
            </tr>
          </tbody>
        </table>
      </section>
    </div>

    <section>
      <h3>Recent answers</h3>
      <ol class="recent" data-test="recent" reversed>
        <li v-for="(answer, i) in [...(latest?.recent ?? [])].reverse()" :key="`${answer.at}-${i}`">
          <time>{{ answer.at }}</time>
          <span>{{ short(answer.command) }} → {{ answer.outcome }}</span>
          <span class="muted">{{ answer.events.map(short).join(', ') }}</span>
          <code v-if="answer.payload">{{ answer.payload }}</code>
        </li>
      </ol>
    </section>
  </section>
</template>

<style scoped>
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
  gap: 16px;
}
table {
  border-collapse: collapse;
  font-size: 12px;
}
td,
th {
  padding: 2px 8px 2px 0;
  text-align: left;
}
h4 {
  margin: 4px 0;
  font-size: 12px;
  font-weight: normal;
}
.muted {
  color: var(--muted);
}
.warn {
  color: var(--bad);
}
.recent {
  font-size: 12px;
  padding-left: 1.5em;
}
.recent li {
  display: flex;
  gap: 10px;
}
code {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  max-width: 60ch;
}
</style>
