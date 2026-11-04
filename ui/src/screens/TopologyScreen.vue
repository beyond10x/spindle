<script setup lang="ts">
import { computed } from 'vue';
import QuotaLine from '../components/QuotaLine.vue';
import { CARD, layout, short } from '../lib/topology';
import { bytes, counter, perSecond, rate, sumCounter, sumRate } from '../lib/rates';
import { commandTotals, rows } from '../lib/summary';
import type { Binding, Deployment, Sample } from '../api/types';

const props = defineProps<{ deployment: Deployment; samples: Record<string, Sample[]> }>();

const drawn = computed(() => layout(props.deployment));
const component = (name: string) => props.deployment.components.find((c) => c.name === name)!;
const instances = (name: string) => props.deployment.instances.filter((i) => i.component === name);
const series = (instanceId: string) => props.samples[instanceId] ?? [];
const all = computed(() => Object.values(props.samples));

function edgeLabel(binding: Binding): { name: string; rate: string; extra: string } {
  if (binding.kind === 'periodic' || binding.from === undefined) {
    const late = sumCounter(all.value, `timer.${binding.name}.late`);
    return {
      name: `${binding.name} → ${short(binding.command)}`,
      rate: perSecond(sumRate(all.value, `timer.${binding.name}.fired`)),
      extra: late ? `late ${late}` : '',
    };
  }
  const delivered = sumRate(all.value, `binding.${binding.name}.delivered`);
  const redelivered = sumCounter(all.value, `binding.${binding.name}.redelivered`);
  return {
    name: short(binding.event ?? binding.command),
    rate: perSecond(delivered ?? sumRate(all.value, `binding.${binding.name}.sent`)),
    extra: redelivered ? `redelivered ${redelivered}` : '',
  };
}

const eventsAppended = (samples: readonly Sample[]) =>
  rows(samples, 'event.').filter((r) => r.name.endsWith('.appended'));
</script>

<template>
  <section class="topology" :style="{ width: `${drawn.width}px`, height: `${drawn.height}px` }">
    <svg class="edges" :width="drawn.width" :height="drawn.height">
      <defs>
        <marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
          <path d="M 0 0 L 10 5 L 0 10 z" class="head" />
        </marker>
      </defs>
      <g v-if="drawn.clock" class="clock">
        <circle :cx="drawn.clock.x" :cy="drawn.clock.y" r="8" />
        <text :x="drawn.clock.x - 14" :y="drawn.clock.y + 4" text-anchor="end">clock</text>
      </g>
      <g
        v-for="edge in drawn.edges"
        :key="edge.binding.name"
        data-test="binding-edge"
        :data-binding="edge.binding.name"
        :data-from="edge.from"
        :data-to="edge.to"
        :class="['edge', edge.binding.kind]"
      >
        <path :d="edge.path" marker-end="url(#arrow)" />
        <text :x="edge.label.x" :y="edge.label.y" text-anchor="middle">
          <tspan>{{ edgeLabel(edge.binding).name }}</tspan>
          <tspan dx="6" class="rate">{{ edgeLabel(edge.binding).rate }}</tspan>
          <tspan v-if="edgeLabel(edge.binding).extra" dx="6" class="warn">{{ edgeLabel(edge.binding).extra }}</tspan>
        </text>
      </g>
    </svg>

    <article
      v-for="card in drawn.cards"
      :key="card.name"
      data-test="component-card"
      :data-component="card.name"
      class="card"
      :style="{ left: `${card.x}px`, top: `${card.y}px`, width: `${CARD.width}px`, height: `${CARD.height}px` }"
    >
      <h2>{{ card.name }}</h2>
      <p v-if="instances(card.name).length === 0" class="muted">no instance</p>
      <div v-for="instance in instances(card.name)" :key="instance.instance_id" class="instance">
        <a :href="`#/instance/${encodeURIComponent(instance.instance_id)}`" class="head">
          <span :class="['state', instance.state]">{{ instance.state }}</span>
          <span>{{ instance.target_kind }} {{ instance.target }}</span>
          <span>{{ instance.requested.vcpus }} vCPU</span>
        </a>
        <QuotaLine :instance="instance" :samples="series(instance.instance_id)" />
        <div class="line">
          <span class="key">cmd</span>
          <span v-for="row in commandTotals(series(instance.instance_id), component(card.name).commands)" :key="row.name">
            {{ short(row.name) }} {{ perSecond(row.rate) }}
          </span>
        </div>
        <div class="line">
          <span class="key">evt</span>
          <span v-for="row in eventsAppended(series(instance.instance_id))" :key="row.name">
            {{ short(row.name.slice(0, -'.appended'.length)) }} {{ row.count }}
          </span>
        </div>
        <div class="line">
          <span class="key">store</span>
          <span>{{ bytes(rate(series(instance.instance_id), 'store.bytes_written')) }}/s</span>
          <span>{{ counter(series(instance.instance_id).at(-1), 'store.rows_written') ?? '–' }} rows</span>
        </div>
      </div>
      <nav class="views">
        <a
          v-for="view in component(card.name).views"
          :key="view"
          :href="`#/view/${encodeURIComponent(card.name)}/${encodeURIComponent(view)}`"
        >{{ short(view) }}</a>
      </nav>
    </article>
  </section>
</template>

<style scoped>
.topology {
  position: relative;
}
.edges {
  position: absolute;
  inset: 0;
  overflow: visible;
}
.edge path {
  fill: none;
  stroke: var(--muted);
  stroke-width: 1.5;
}
.edge.periodic path {
  stroke-dasharray: 4 3;
}
.edge text,
.clock text {
  font-size: 11px;
  fill: var(--fg);
}
.edge .rate {
  fill: var(--accent);
}
.edge .warn {
  fill: var(--bad);
}
.head {
  fill: var(--muted);
}
.clock circle {
  fill: none;
  stroke: var(--muted);
}
.card {
  position: absolute;
  box-sizing: border-box;
  overflow: hidden;
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--panel);
  font-size: 12px;
}
.card h2 {
  margin: 0 0 4px;
  font-size: 14px;
}
.instance .head {
  display: flex;
  gap: 8px;
  color: inherit;
  text-decoration: none;
}
.line {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 8px;
}
.key,
.muted {
  color: var(--muted);
}
.key {
  width: 2.5em;
}
.state.Running {
  color: var(--good);
}
.state.Exited,
.state.Refused {
  color: var(--bad);
}
.views {
  margin-top: 4px;
  display: flex;
  gap: 8px;
}
</style>
