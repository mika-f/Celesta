import type { CompositionConfig, Frame } from './types';

export interface CompileOptions {
  files?: Record<string, string>;
  entry?: string;
  /** Absolute URL of the entry's media directory. */
  baseURL?: string;
  /** For sample editions whose generated soundtrack is not distributed. */
  silent?: boolean;
}

export class Engine {
  private worker = new Worker(new URL('./engine.worker.ts', import.meta.url), { type: 'module' });
  private nextId = 0;
  private pending = new Map<number, { resolve: (value: unknown) => void; reject: (reason: Error) => void; timeout: ReturnType<typeof setTimeout> }>();

  constructor() {
    this.worker.onmessage = (event: MessageEvent<{ id: number; value?: unknown; error?: string }>) => {
      const pending = this.pending.get(event.data.id);
      if (!pending) return;
      clearTimeout(pending.timeout);
      this.pending.delete(event.data.id);
      if (event.data.error) pending.reject(new Error(event.data.error));
      else pending.resolve(event.data.value);
    };
    this.worker.onerror = event => {
      this.failAll(new Error(event.message || 'The composition worker failed.'));
    };
  }

  private failAll(error: Error) {
    for (const pending of this.pending.values()) { clearTimeout(pending.timeout); pending.reject(error); }
    this.pending.clear();
    this.worker.terminate();
  }

  dispose() { this.failAll(new Error('Composition changed.')); }

  private request<T>(type: 'compile' | 'frame', data: { source?: string; frame?: number; options?: CompileOptions }, timeoutMs: number): Promise<T> {
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.failAll(new Error('Composition took too long to evaluate. Check for an infinite loop.'));
      }, timeoutMs);
      this.pending.set(id, { resolve: resolve as (value: unknown) => void, reject, timeout });
      this.worker.postMessage({ id, type, ...data });
    });
  }

  compile(source: string, options: CompileOptions = {}): Promise<CompositionConfig> { return this.request('compile', { source, options }, 30_000); }
  frame(frame: number): Promise<Frame> { return this.request('frame', { frame }, 10_000); }
}
