// The field-by-field shape of each API answer. `Shape<T>` makes `vue-tsc` refuse any entry that
// is missing from, or absent in, the generated type re-exported by `./types.ts`; the allowed
// values of each enum are read from the generated enum objects, never listed here.

import type { Shape } from '../lib/conform';
import {
  BindingKind,
  CounterKind,
  CounterSource,
  State,
  TargetKind,
  type Catalogue,
  type Deployment,
  type Sample,
  type StateChange,
} from './types';

const text = { type: 'string' } as const;
const number = { type: 'number' } as const;
const texts = { type: 'array', of: text } as const;
const oneOf = <T extends string>(values: Record<string, T>) =>
  ({ type: 'string', values: Object.values(values) }) as const;
const state = oneOf(State);

export const deploymentShape: Shape<Deployment> = {
  system: text,
  components: {
    type: 'array',
    of: {
      type: 'object',
      fields: { name: text, domains: texts, commands: texts, views: texts },
    },
  },
  bindings: {
    type: 'array',
    of: {
      type: 'object',
      fields: {
        name: text,
        kind: oneOf(BindingKind),
        from: { ...text, optional: true },
        to: text,
        event: { ...text, optional: true },
        command: text,
      },
    },
  },
  instances: {
    type: 'array',
    of: {
      type: 'object',
      fields: {
        instance_id: text,
        component: text,
        target_kind: oneOf(TargetKind),
        target: text,
        state,
        requested: {
          type: 'object',
          fields: { vcpus: number, memory_mib: number, disk_mib: number },
        },
        started_at: { ...text, optional: true },
      },
    },
  },
};

export const catalogueShape: Shape<Catalogue> = {
  component: text,
  counters: {
    type: 'array',
    of: {
      type: 'object',
      fields: { name: text, kind: oneOf(CounterKind), source: oneOf(CounterSource) },
    },
  },
};

export const sampleShape: Shape<Sample> = {
  instance_id: text,
  sequence: number,
  at: text,
  period_ms: number,
  counters: {
    type: 'array',
    of: { type: 'object', fields: { name: text, value: number } },
  },
  histograms: {
    type: 'array',
    of: {
      type: 'object',
      fields: { name: text, bounds: { type: 'array', of: number }, counts: { type: 'array', of: number } },
    },
  },
  recent: {
    type: 'array',
    of: {
      type: 'object',
      fields: {
        at: text,
        command: text,
        outcome: text,
        events: texts,
        payload: { ...text, optional: true },
      },
    },
  },
  resources: {
    type: 'object',
    fields: {
      cpu_seconds: number,
      memory_bytes: number,
      cpu_share: { ...number, optional: true },
      memory_share: { ...number, optional: true },
    },
  },
  dropped_frames: number,
};

export const stateChangeShape: Shape<StateChange> = {
  instance_id: text,
  from: state,
  to: state,
  at: text,
};
