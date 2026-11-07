// The field-by-field shape of each API answer. `Shape<T>` makes `vue-tsc` refuse any entry that
// is missing from, or absent in, the generated type re-exported by `./types.ts`; the allowed
// values of each enum are read from the generated enum objects, never listed here.

import type { Shape } from '../lib/conform';
import {
  AnswerSource,
  BindingKind,
  CounterKind,
  CounterSource,
  EdgeKind,
  MetricKind,
  NodeKind,
  OperationKind,
  RequestClass,
  RequestLogFormat,
  State,
  TargetKind,
  TopologyFormat,
  type Catalogue,
  type Deployment,
  type MetricFamilies,
  type RequestLog,
  type Sample,
  type StateChange,
  type StatusClass,
  type Topology,
} from './types';

const text = { type: 'string' } as const;
const number = { type: 'number' } as const;
const texts = { type: 'array', of: text } as const;
const oneOf = <T extends string>(values: Record<string, T>) =>
  ({ type: 'string', values: Object.values(values) }) as const;
const state = oneOf(State);

// ess 0.88.0 generates no value object for an enum whose variants start with a digit, so the
// status classes are keyed here; `satisfies` makes `vue-tsc` refuse a missing or extra class.
const statusClasses = {
  '2xx': true,
  '3xx': true,
  '4xx': true,
  '5xx': true,
} as const satisfies Record<StatusClass, true>;
const statusClass = {
  type: 'string',
  values: Object.keys(statusClasses) as StatusClass[],
} as const;

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

export const requestLogShape: Shape<RequestLog> = {
  format: oneOf(RequestLogFormat),
  capacity: number,
  recorded: number,
  totals: {
    type: 'object',
    fields: {
      served: number,
      refused: number,
      not_granted: number,
      not_served: number,
      failed: number,
      unrouted: number,
    },
  },
  requests: {
    type: 'array',
    of: {
      type: 'object',
      fields: {
        sequence: number,
        at: text,
        method: text,
        path: text,
        route: { ...text, optional: true },
        operation: { ...text, optional: true },
        kind: { ...oneOf(OperationKind), optional: true },
        status: number,
        class: oneOf(RequestClass),
        outcome: { ...text, optional: true },
        reason: { ...text, optional: true },
        duration_us: number,
        events: texts,
        externals: {
          type: 'array',
          of: {
            type: 'object',
            fields: {
              outcome: text,
              holds: { type: 'boolean', optional: true },
              source: oneOf(AnswerSource),
            },
          },
        },
      },
    },
  },
};

export const metricFamiliesShape: Shape<MetricFamilies> = {
  families: {
    type: 'array',
    of: {
      type: 'object',
      fields: { name: text, kind: oneOf(MetricKind), labels: texts, summary: text },
    },
  },
};

export const topologyShape: Shape<Topology> = {
  format: oneOf(TopologyFormat),
  at: text,
  specifications: texts,
  nodes: {
    type: 'array',
    of: {
      type: 'object',
      fields: {
        id: text,
        kind: oneOf(NodeKind),
        name: text,
        specification: { ...text, optional: true },
      },
    },
  },
  edges: {
    type: 'array',
    of: {
      type: 'object',
      fields: {
        from: text,
        to: text,
        kind: oneOf(EdgeKind),
        traffic: {
          type: 'object',
          optional: true,
          fields: {
            count: number,
            statuses: {
              type: 'array',
              of: { type: 'object', fields: { status_class: statusClass, count: number } },
            },
            sources: {
              type: 'array',
              of: { type: 'object', fields: { source: oneOf(AnswerSource), count: number } },
            },
            latest: { type: 'array', of: number },
          },
        },
      },
    },
  },
};
