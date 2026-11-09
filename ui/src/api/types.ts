// The answers of the observe API (docs/design/observe.md § 5) and of a served run's reserved routes
// (§ 10), domain `spindle.observe`. The types are generated from the specification into
// `./generated/` and drift-checked by `task check`; every import of an API type goes through this
// file.

import type { DeployedInstance } from './generated/types';

export {
  AnswerSource,
  BindingKind,
  CounterKind,
  CounterSource,
  DeliveryGuarantee,
  EdgeKind,
  MetricKind,
  NodeKind,
  OperationKind,
  RequestClass,
  RequestLogFormat,
  State,
  TargetKind,
  TopologyFormat,
} from './generated/types';

export type {
  Binding,
  BindingDelivery,
  Catalogue,
  Component,
  Counter,
  CounterValue,
  DeployedInstance,
  Deployment,
  EdgeTraffic,
  ExternalAnswer,
  Histogram,
  InstanceId,
  MetricFamilies,
  MetricFamily,
  Payload,
  RecentAnswer,
  RequestLog,
  RequestRecord,
  RequestTotals,
  ResourceReading,
  Resources,
  Sample,
  SourceCount,
  StatusClass,
  StateChange,
  StatusCount,
  TargetId,
  Topology,
  TopologyEdge,
  TopologyNode,
  UnmetInput,
} from './generated/types';

/** One placed instance, as the deployment answer lists it. */
export type Instance = DeployedInstance;

// Not part of the contract: `GET /api/views/<component>/<view>` answers the view as the instance's
// ingress does: `{"rows": [...]}` for a list view; the dev fixtures hold one row (an object), a list of
// rows, or null when nothing matches.
export type ViewRow = Record<string, unknown>;
export type ViewAnswer = ViewRow | ViewRow[] | null;
