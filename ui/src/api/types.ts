// The answers of the observe API (docs/design/observe.md § 5), domain `spindle.observe`.
//
// Written by hand until the contract's generated types land in `./generated/`. Then the four
// top-level declarations below become a re-export of the generated ones; the derived aliases at
// the bottom stay, so no other module changes. Every import of an API type goes through this file.

export interface Deployment {
  system: string;
  components: {
    name: string;
    domains: string[];
    commands: string[];
    views: string[];
  }[];
  bindings: {
    name: string;
    kind: 'periodic' | 'event';
    from?: string;
    to: string;
    event?: string;
    command: string;
  }[];
  instances: {
    instance_id: string;
    component: string;
    target_kind: string;
    target: string;
    state: string;
    requested: {
      vcpus: number;
      memory_mib: number;
      disk_mib: number;
    };
    started_at?: string;
  }[];
}

export interface Catalogue {
  component: string;
  counters: {
    name: string;
    kind: 'counter' | 'histogram';
    source: 'kernel' | 'store' | 'channel' | 'timer' | 'ingress' | 'view' | 'resource';
  }[];
}

export interface Sample {
  instance_id: string;
  sequence: number;
  at: string;
  period_ms: number;
  counters: {
    name: string;
    value: number;
  }[];
  histograms: {
    name: string;
    bounds: number[];
    counts: number[];
  }[];
  recent: {
    at: string;
    command: string;
    outcome: string;
    events: string[];
    payload?: string;
  }[];
  resources: {
    cpu_seconds: number;
    memory_bytes: number;
    cpu_share?: number;
    memory_share?: number;
  };
  dropped_frames: number;
}

export interface StateChange {
  instance_id: string;
  from: string;
  to: string;
  at: string;
}

export type Component = Deployment['components'][number];
export type Binding = Deployment['bindings'][number];
export type Instance = Deployment['instances'][number];
export type Requested = Instance['requested'];
export type CounterSpec = Catalogue['counters'][number];
export type CounterValue = Sample['counters'][number];
export type Histogram = Sample['histograms'][number];
export type RecentAnswer = Sample['recent'][number];
export type Resources = Sample['resources'];

// Not part of the contract: `GET /api/views/<component>/<view>` answers the view as the instance's
// ingress does, one row (an object), a list of rows, or null when nothing matches.
export type ViewRow = Record<string, unknown>;
export type ViewAnswer = ViewRow | ViewRow[] | null;
