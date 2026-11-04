import { describe, expect, test } from 'vitest';
import { SseParser, type SseEvent } from './sse';

const parse = (...chunks: string[]): SseEvent[] => {
  const events: SseEvent[] = [];
  const parser = new SseParser((event) => events.push(event));
  for (const chunk of chunks) parser.push(chunk);
  return events;
};

describe('the event stream parser', () => {
  test('dispatches one event per blank line, with its name and data', () => {
    expect(parse('event: sample\ndata: {"a":1}\n\nevent: state\ndata: {"b":2}\n\n')).toEqual([
      { event: 'sample', data: '{"a":1}' },
      { event: 'state', data: '{"b":2}' },
    ]);
  });

  test('joins an event split across chunks and lines split across CRLF', () => {
    expect(parse('event: sam', 'ple\r\ndata: {"a"', ':1}\r\n', '\r\n')).toEqual([
      { event: 'sample', data: '{"a":1}' },
    ]);
  });

  test('joins several data lines, ignores comments, and names an unnamed event message', () => {
    expect(parse(': keep-alive\n\ndata: one\ndata: two\n\n')).toEqual([
      { event: 'message', data: 'one\ntwo' },
    ]);
  });

  test('keeps an unfinished event until its blank line arrives', () => {
    expect(parse('event: sample\ndata: x\n')).toEqual([]);
  });
});
