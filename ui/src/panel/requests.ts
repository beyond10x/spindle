// What the panel reads, and how. Every URL is relative to the page itself (`./requests`), so the
// panel works wherever a served run's `/__spindle/` is mounted.

import { RequestClass } from '../api/types';
import type { RequestTotals } from '../api/types';

/** The request log, every class or only the requests spindle did not serve. */
export const requestsUrl = (notServedOnly: boolean): string =>
  notServedOnly ? `./requests?class=${RequestClass.NotServed}` : './requests';

/** One count of the banner. */
export interface Count {
  name: RequestClass;
  label: string;
  value: number;
}

/** The five counts the banner shows, in its order, read from the totals. */
export const counts = (totals: RequestTotals): Count[] => [
  { name: RequestClass.Served, label: 'served', value: totals.served },
  { name: RequestClass.NotServed, label: 'not served', value: totals.not_served },
  { name: RequestClass.Refused, label: 'refused', value: totals.refused },
  { name: RequestClass.NotGranted, label: 'not granted', value: totals.not_granted },
  { name: RequestClass.Unrouted, label: 'unrouted', value: totals.unrouted },
];
