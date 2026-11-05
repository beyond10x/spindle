// The answers of the observe API (docs/design/observe.md § 5), domain `spindle.observe`. The types
// are generated from the specification into `./generated/` and drift-checked by `task check`;
// every import of an API type goes through this file.

import type { DeployedInstance } from './generated/types';

export { BindingKind, CounterKind, CounterSource, State, TargetKind } from './generated/types';

export type {
  Binding,
  Catalogue,
  Component,
  Counter,
  CounterValue,
  DeployedInstance,
  Deployment,
  Histogram,
  InstanceId,
  Payload,
  RecentAnswer,
  ResourceReading,
  Resources,
  Sample,
  StateChange,
  TargetId,
} from './generated/types';

/** One placed instance, as the deployment answer lists it. */
export type Instance = DeployedInstance;

// Not part of the contract: `GET /api/views/<component>/<view>` answers the view as the instance's
// ingress does: `{"rows": [...]}` for a list view; the dev fixtures hold one row (an object), a list of
// rows, or null when nothing matches.
export type ViewRow = Record<string, unknown>;
export type ViewAnswer = ViewRow | ViewRow[] | null;
