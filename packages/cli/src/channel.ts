// The CLI's side of the request/response channel with the bridge: the lines
// it reads from stdin, and the queue its prepare-time requests take turns on.

import * as fs from 'node:fs';

/** How many bytes one read asks for. */
const CHUNK_BYTES = 64 * 1024;

/**
 * Newline-separated lines read from `fd` (stdin unless given), by `next()` or
 * `nextSync()`. One shared buffer keeps async reads from swallowing
 * synchronous RPC replies.
 *
 * Reading a line takes time linear in its length: every byte is searched for
 * the newline once, and a line's chunks are joined once, when it ends. A frame
 * request that carries a companion project can be megabytes long.
 */
export class ProtocolLines implements AsyncIterator<string> {
  /** The current line's bytes so far, read before `rest`; none holds a newline. */
  private partial: Buffer[] = [];
  /** Bytes read and not yet searched for a newline. */
  private rest: Buffer = Buffer.alloc(0);
  private ended = false;

  constructor(private readonly fd = 0) {}

  private takeLine(): IteratorResult<string> | undefined {
    const newline = this.rest.indexOf(10);
    if (newline >= 0) {
      this.partial.push(this.rest.subarray(0, newline));
      this.rest = this.rest.subarray(newline + 1);
      return this.flush();
    }
    if (this.rest.length > 0) {
      this.partial.push(this.rest);
      this.rest = Buffer.alloc(0);
    }
    if (!this.ended) return undefined;
    // The last line may end without a newline.
    return this.partial.length > 0 ? this.flush() : { done: true, value: undefined };
  }

  private flush(): IteratorResult<string> {
    const value = Buffer.concat(this.partial).toString('utf8');
    this.partial = [];
    return { done: false, value };
  }

  private received(chunk: Buffer, count: number): void {
    this.ended = count === 0;
    this.rest = chunk.subarray(0, count);
  }

  async next(): Promise<IteratorResult<string>> {
    while (true) {
      const line = this.takeLine();
      if (line) return line;
      // A fresh buffer each read: `partial` keeps views of the earlier ones.
      const chunk = Buffer.allocUnsafe(CHUNK_BYTES);
      const count = await new Promise<number>((resolve, reject) => {
        fs.read(this.fd, chunk, 0, chunk.length, null, (error, bytesRead) => {
          if (error) reject(error);
          else resolve(bytesRead);
        });
      });
      this.received(chunk, count);
    }
  }

  nextSync(): IteratorResult<string> {
    while (true) {
      const line = this.takeLine();
      if (line) return line;
      const chunk = Buffer.allocUnsafe(CHUNK_BYTES);
      this.received(chunk, fs.readSync(this.fd, chunk, 0, chunk.length, null));
    }
  }
}

/**
 * A queue requests take turns on, one at a time, in the order they were made.
 * `serialized(request)` wraps `request` so that each call waits for the calls
 * before it, on any function the same queue wraps, to settle. A call's
 * failure rejects to its caller and does not stop the calls after it.
 */
export function requestQueue(): <A extends unknown[], R>(request: (...args: A) => Promise<R>) => (...args: A) => Promise<R> {
  let tail: Promise<void> = Promise.resolve();
  return (request) => (...args) => {
    const result = tail.then(() => request(...args));
    tail = result.then(
      () => undefined,
      () => undefined,
    );
    return result;
  };
}
