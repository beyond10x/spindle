// Server-Sent Events, parsed from a fetched body. `EventSource` cannot send an `Authorization`
// header, and every request to the control process carries one.

export interface SseEvent {
  event: string;
  data: string;
}

export class SseParser {
  private buffer = '';
  private event = '';
  private data: string[] = [];

  private readonly dispatch: (event: SseEvent) => void;

  constructor(dispatch: (event: SseEvent) => void) {
    this.dispatch = dispatch;
  }

  push(chunk: string): void {
    this.buffer += chunk;
    for (;;) {
      const end = this.buffer.search(/\r\n|\r|\n/);
      if (end < 0) return;
      const breakLength = this.buffer.startsWith('\r\n', end) ? 2 : 1;
      if (this.buffer[end] === '\r' && breakLength === 1 && end === this.buffer.length - 1) return;
      const line = this.buffer.slice(0, end);
      this.buffer = this.buffer.slice(end + breakLength);
      this.line(line);
    }
  }

  private line(line: string): void {
    if (line === '') {
      if (this.data.length > 0) this.dispatch({ event: this.event || 'message', data: this.data.join('\n') });
      this.event = '';
      this.data = [];
      return;
    }
    if (line.startsWith(':')) return;
    const colon = line.indexOf(':');
    const field = colon < 0 ? line : line.slice(0, colon);
    let value = colon < 0 ? '' : line.slice(colon + 1);
    if (value.startsWith(' ')) value = value.slice(1);
    if (field === 'event') this.event = value;
    else if (field === 'data') this.data.push(value);
  }
}
