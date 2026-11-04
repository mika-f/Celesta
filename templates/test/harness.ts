// Evaluates a template entry through the real CLI, the way the editor does,
// answering the host requests (media probing, text measurement) with fakes.
// A build-integration harness: it spawns the built packages/cli, so it needs
// `pnpm run build:runtime` first.
import { spawn } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import type { CompositionConfig, TextMetrics } from '@celesta.studio/react';
import type { MeasureTextRequest, ProbedMediaInfo } from '@celesta.studio/react/internal';

/** A property-input layer, as the editor sends it (`PropertyInputLayer` in packages/cli/src/property-inputs.ts). */
export interface PropertyInputLayer {
  source: 'project' | 'propsFile' | 'entryProps' | 'props';
  file?: string;
  baseDir: string;
  values: Record<string, unknown>;
}

/** A value the CLI rejected (`PropertyIssue` in packages/cli/src/property-inputs.ts). */
export interface PropertyIssue {
  key: string;
  source: string;
  message: string;
}

/** The stdout messages the harness answers or reports (part of `Message` in packages/cli/src/cli.ts). */
type Message =
  | { probeMedia: { path: string } }
  | { measureText: MeasureTextRequest }
  | { config: CompositionConfig }
  | { error: string }
  | { invalidProperties: PropertyIssue[] };

/** What `evaluate` resolves with: the config (with a plain `fps`), or the error or rejected properties. */
export interface Evaluation {
  config?: CompositionConfig & { fps: number };
  error?: string;
  invalidProperties?: PropertyIssue[];
}

const cli = fileURLToPath(new URL('../../packages/cli/bin/celesta-react-render.js', import.meta.url));

function fakeMetrics(text: string): TextMetrics {
  return { width: text.length * 10, height: 24, ascent: 18, descent: 6, lineHeight: 24, capHeight: 0, xHeight: 0, lines: 1, glyphs: [] };
}

function fakeProbe(path: string): { media: ProbedMediaInfo } | { error: string } {
  return existsSync(path)
    ? { media: { durationSeconds: 2, audio: [{ durationSeconds: 2 }] } }
    : { error: `cannot probe ${path}` };
}

/**
 * Runs `entry` through the CLI with the given property-input layers and
 * resolves with the first `config`, `error` or `invalidProperties` it reports.
 */
export async function evaluate(entry: string, layers: PropertyInputLayer[]): Promise<Evaluation> {
  const child = spawn(process.execPath, [cli, entry, '--properties-stdin'], { stdio: ['pipe', 'pipe', 'pipe'] });
  let stderr = '';
  child.stderr.on('data', (chunk: Buffer) => { stderr += chunk; });
  const send = (value: unknown) => child.stdin.write(`${JSON.stringify(value)}\n`);
  send({ propertyInputs: layers });
  try {
    for await (const line of createInterface({ input: child.stdout })) {
      const message: Message = JSON.parse(line);
      if ('probeMedia' in message) {
        send(fakeProbe(message.probeMedia.path));
      } else if ('measureText' in message) {
        send({ metrics: fakeMetrics(message.measureText.text) });
      } else if ('config' in message) {
        // The CLI reports a rational frame rate; tests compare plain fps.
        const { numerator, denominator } = message.config.frameRate;
        return { ...message, config: { ...message.config, fps: numerator / denominator } };
      } else if ('error' in message || 'invalidProperties' in message) {
        return message;
      }
    }
    throw new Error(`The CLI exited without reporting a config. ${stderr}`);
  } finally {
    child.stdin.end();
    child.kill();
  }
}

/** The property-input layer the editor builds from a project's props.json. */
export function entryPropsLayer(dir: string): PropertyInputLayer {
  const file = join(dir, 'props.json');
  return { source: 'entryProps', file, baseDir: dir, values: JSON.parse(readFileSync(file, 'utf8')) };
}
