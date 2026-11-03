import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';

import { CelestaExportError } from './errors.js';
import { resolveExporter } from './resolve.js';
import type { ExportReport, ProgressEvent, RunOptions } from './types.js';

/** How long a cancelled exporter has to stop on its own before it is killed. */
export const CANCEL_TIMEOUT_MS = 10_000;

/** The most stderr kept for an exporter that ends without a report. */
const STDERR_LIMIT = 64 * 1024;

/** Flags this package passes that older exporters reject. */
const NEWER_FLAGS = /'--(info|json-progress|stdin-cancel)'/;

/**
 * Runs the exporter with `args` (which include `--json`) and resolves with
 * its report when it succeeded. Progress lines go to `onProgress`.
 */
export function runExporter(args: string[], options: RunOptions): Promise<ExportReport> {
  const exporter = resolveExporter({ exporter: options.exporter, env: options.env });
  return new Promise((resolve, reject) => {
    if (options.signal?.aborted) {
      reject(cancelled());
      return;
    }
    const child = spawn(exporter, args, {
      cwd: options.cwd,
      env: options.env ?? process.env,
      stdio: ['pipe', 'pipe', 'pipe'],
      windowsHide: true,
    });
    let report: ExportReport | undefined;
    let callbackError: unknown;
    let stderr = '';
    let killTimer: NodeJS.Timeout | undefined;
    let settled = false;

    const cancel = () => {
      if (child.exitCode !== null || child.signalCode !== null) return;
      // `--stdin-cancel`: the exporter stops at the next frame and removes
      // what it was writing. Stills it already wrote stay.
      child.stdin.end('cancel\n');
      killTimer ??= setTimeout(() => child.kill(), CANCEL_TIMEOUT_MS);
    };
    const onAbort = () => cancel();
    options.signal?.addEventListener('abort', onAbort, { once: true });

    const finish = (outcome: () => void) => {
      if (settled) return;
      settled = true;
      clearTimeout(killTimer);
      options.signal?.removeEventListener('abort', onAbort);
      outcome();
    };

    // The exporter may exit before reading what is written to it.
    child.stdin.on('error', () => {});
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', (chunk: string) => {
      stderr = (stderr + chunk).slice(-STDERR_LIMIT);
    });
    const lines = createInterface({ input: child.stdout, crlfDelay: Infinity });
    lines.on('line', (line) => {
      if (!line.trim()) return;
      let message: (ProgressEvent | ExportReport) & { event?: string };
      try {
        message = JSON.parse(line);
      } catch {
        stderr = `${stderr}${line}\n`.slice(-STDERR_LIMIT);
        return;
      }
      if (message.event === undefined) {
        report = message as ExportReport;
        return;
      }
      if (callbackError !== undefined) return;
      try {
        options.onProgress?.(message as ProgressEvent);
      } catch (error) {
        // A throwing callback ends the export and is what the caller sees.
        callbackError = error;
        cancel();
      }
    });

    child.on('error', (error: NodeJS.ErrnoException) => {
      finish(() =>
        reject(
          new CelestaExportError({
            code: error.code === 'ENOENT' ? 'exporter_not_found' : 'exporter_failed',
            message: `could not run ${exporter}: ${error.message}`,
            hint:
              error.code === 'ENOENT'
                ? 'install Celesta, set CELESTA_EXPORTER, or pass exporter with the path to celesta-exporter'
                : undefined,
          }),
        ),
      );
    });

    // `close` waits for stdout to end, so the report line has been read.
    child.on('close', (code, signal) => {
      finish(() => {
        if (callbackError !== undefined) {
          reject(callbackError);
        } else if (report?.status === 'ok') {
          resolve(report);
        } else if (report?.error) {
          reject(reportError(report));
        } else if (options.signal?.aborted) {
          reject(cancelled());
        } else {
          reject(
            new CelestaExportError(
              {
                code: 'exporter_failed',
                message: `${exporter} ended (${signal ?? `exit status ${code}`}) without a result`,
              },
              { stderr },
            ),
          );
        }
      });
    });
  });
}

function reportError(report: ExportReport): CelestaExportError {
  const error = report.error!;
  if (error.code === 'usage' && error.hint === undefined && NEWER_FLAGS.test(error.message)) {
    return new CelestaExportError(
      {
        ...error,
        hint: 'this celesta-exporter is older than @celesta.studio/exporter; set CELESTA_EXPORTER or pass exporter with a newer one',
      },
      { report },
    );
  }
  return new CelestaExportError(error, { report });
}

function cancelled(): CelestaExportError {
  return new CelestaExportError({ code: 'cancelled', message: 'export was cancelled' });
}
