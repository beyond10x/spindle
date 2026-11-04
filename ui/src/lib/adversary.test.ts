import { describe, expect, test } from 'vitest';
import { SseParser, type SseEvent } from './sse';
import { route } from './route';

const parse = (...chunks: string[]): SseEvent[] => {
  const events: SseEvent[] = [];
  const parser = new SseParser((event) => events.push(event));
  for (const chunk of chunks) parser.push(chunk);
  return events;
};

describe('adversary: the event stream parser', () => {
  // The event stream format allows CR alone as a line end; a blank line ends the event. The event
  // below is complete when its chunk arrives, so it is dispatched then, not one chunk later.
  test('dispatches an event whose lines end in CR alone when its chunk arrives', () => {
    expect(parse('data: one\r\r')).toEqual([{ event: 'message', data: 'one' }]);
  });
});

describe('adversary: the route', () => {
  // The fragment is whatever the address bar holds; a stray `%` must not throw out of the
  // computed route and take every screen down with it.
  test('a fragment with a malformed escape falls back to the topology', () => {
    expect(route('#/view/rally-table/100%')).toEqual({ screen: 'topology' });
  });
});
