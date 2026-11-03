// SVG path data (`d`) to path commands and back.

import type { PathCommand } from '@celesta.studio/react';

const NUMBER = /^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?/;

/** How many numbers each SVG command takes per repetition. */
const ARITY: Record<string, number> = { m: 2, l: 2, h: 1, v: 1, c: 6, s: 4, q: 4, t: 2, a: 7, z: 0 };

class Reader {
  private index = 0;

  constructor(private readonly d: string) {}

  private skip(): void {
    while (this.index < this.d.length && /[\s,]/.test(this.d[this.index])) {
      this.index += 1;
    }
  }

  done(): boolean {
    this.skip();
    return this.index >= this.d.length;
  }

  /** The next command letter, or `null` when a number follows (an implicit repeat). */
  command(): string | null {
    this.skip();
    const c = this.d[this.index];
    if (c !== undefined && /[a-zA-Z]/.test(c)) {
      if (!(c.toLowerCase() in ARITY)) {
        throw this.error(`unknown command ${JSON.stringify(c)}`);
      }
      this.index += 1;
      return c;
    }
    return null;
  }

  number(): number {
    this.skip();
    const match = NUMBER.exec(this.d.slice(this.index));
    if (match === null) {
      throw this.error('expected a number');
    }
    this.index += match[0].length;
    return Number(match[0]);
  }

  /** An arc flag: a lone `0` or `1`, which may run into the next number. */
  flag(): boolean {
    this.skip();
    const c = this.d[this.index];
    if (c !== '0' && c !== '1') {
      throw this.error('expected an arc flag (0 or 1)');
    }
    this.index += 1;
    return c === '1';
  }

  error(message: string): Error {
    return new Error(`parseSvgPath(): ${message} at index ${this.index} of ${JSON.stringify(this.d)}`);
  }
}

/** `control` mirrored through `x`/`y`, or `x`/`y` itself when there is none to mirror. */
function reflect(control: readonly [number, number] | null, x: number, y: number): [number, number] {
  return control ? [2 * x - control[0], 2 * y - control[1]] : [x, y];
}

/**
 * Converts SVG path data — the `d` of an SVG `<path>` — to path commands for
 * `<Path>`. Every command is supported, relative or absolute: `M L H V C S
 * Q T A Z`. The results are absolute `moveTo`/`lineTo`/`quadTo`/`cubicTo`/
 * `close`; `H`/`V` become `lineTo`, `S`/`T` have their reflected control
 * point written out, and an arc `A` becomes cubic curves (each an eighth
 * of a turn at most, within 0.0004% of the radius).
 */
export function parseSvgPath(d: string): PathCommand[] {
  const reader = new Reader(d);
  const commands: PathCommand[] = [];
  let x = 0;
  let y = 0;
  let startX = 0;
  let startY = 0;
  // The control point an `S` or a `T` reflects, if the previous command left one.
  let cubicControl: [number, number] | null = null;
  let quadControl: [number, number] | null = null;
  let closed = false;
  let previous: string | null = null;

  while (!reader.done()) {
    let letter = reader.command();
    if (letter === null) {
      if (previous === null || previous.toLowerCase() === 'z') {
        throw reader.error('expected a command');
      }
      // Numbers after `M` repeat as `L`; after any other command, as itself.
      letter = previous === 'M' ? 'L' : previous === 'm' ? 'l' : previous;
    }
    previous = letter;
    const relative = letter === letter.toLowerCase();
    const kind = letter.toLowerCase();
    const ox = relative ? x : 0;
    const oy = relative ? y : 0;

    // A drawing command after `close` starts a new subpath where the last one started.
    if (closed && kind !== 'm' && kind !== 'z') {
      commands.push({ type: 'moveTo', x, y });
    }
    closed = false;
    let nextCubic: [number, number] | null = null;
    let nextQuad: [number, number] | null = null;

    switch (kind) {
      case 'm':
        x = ox + reader.number();
        y = oy + reader.number();
        startX = x;
        startY = y;
        commands.push({ type: 'moveTo', x, y });
        break;
      case 'l':
        x = ox + reader.number();
        y = oy + reader.number();
        commands.push({ type: 'lineTo', x, y });
        break;
      case 'h':
        x = ox + reader.number();
        commands.push({ type: 'lineTo', x, y });
        break;
      case 'v':
        y = oy + reader.number();
        commands.push({ type: 'lineTo', x, y });
        break;
      case 'c': {
        const x1 = ox + reader.number();
        const y1 = oy + reader.number();
        const x2 = ox + reader.number();
        const y2 = oy + reader.number();
        x = ox + reader.number();
        y = oy + reader.number();
        commands.push({ type: 'cubicTo', x1, y1, x2, y2, x, y });
        nextCubic = [x2, y2];
        break;
      }
      case 's': {
        const [x1, y1] = reflect(cubicControl, x, y);
        const x2 = ox + reader.number();
        const y2 = oy + reader.number();
        x = ox + reader.number();
        y = oy + reader.number();
        commands.push({ type: 'cubicTo', x1, y1, x2, y2, x, y });
        nextCubic = [x2, y2];
        break;
      }
      case 'q': {
        const x1 = ox + reader.number();
        const y1 = oy + reader.number();
        x = ox + reader.number();
        y = oy + reader.number();
        commands.push({ type: 'quadTo', x1, y1, x, y });
        nextQuad = [x1, y1];
        break;
      }
      case 't': {
        const [x1, y1] = reflect(quadControl, x, y);
        x = ox + reader.number();
        y = oy + reader.number();
        commands.push({ type: 'quadTo', x1, y1, x, y });
        nextQuad = [x1, y1];
        break;
      }
      case 'a': {
        const rx = reader.number();
        const ry = reader.number();
        const rotation = reader.number();
        const large = reader.flag();
        const sweep = reader.flag();
        const ex = ox + reader.number();
        const ey = oy + reader.number();
        commands.push(...arcToCubics(x, y, rx, ry, rotation, large, sweep, ex, ey));
        x = ex;
        y = ey;
        break;
      }
      case 'z':
        commands.push({ type: 'close' });
        x = startX;
        y = startY;
        closed = true;
        break;
    }
    cubicControl = nextCubic;
    quadControl = nextQuad;
  }
  return commands;
}

/**
 * An SVG elliptical arc from `x0`/`y0` to `x`/`y` as cubic curves, following
 * the SVG spec's endpoint-to-centre conversion (appendix B.2.4), radii
 * scaled up when they are too small to reach.
 */
function arcToCubics(
  x0: number,
  y0: number,
  rx: number,
  ry: number,
  degrees: number,
  large: boolean,
  sweep: boolean,
  x: number,
  y: number,
): PathCommand[] {
  rx = Math.abs(rx);
  ry = Math.abs(ry);
  if (x0 === x && y0 === y) {
    return [];
  }
  if (rx === 0 || ry === 0) {
    return [{ type: 'lineTo', x, y }];
  }
  const phi = (degrees * Math.PI) / 180;
  const cos = Math.cos(phi);
  const sin = Math.sin(phi);
  const dx = (x0 - x) / 2;
  const dy = (y0 - y) / 2;
  const x1p = cos * dx + sin * dy;
  const y1p = -sin * dx + cos * dy;
  const lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
  if (lambda > 1) {
    rx *= Math.sqrt(lambda);
    ry *= Math.sqrt(lambda);
  }
  const numerator = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
  const denominator = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
  const factor = (large === sweep ? -1 : 1) * Math.sqrt(Math.max(0, numerator / denominator));
  const cxp = (factor * rx * y1p) / ry;
  const cyp = (-factor * ry * x1p) / rx;
  const cx = cos * cxp - sin * cyp + (x0 + x) / 2;
  const cy = sin * cxp + cos * cyp + (y0 + y) / 2;

  const angle = (ux: number, uy: number, vx: number, vy: number) =>
    Math.atan2(ux * vy - uy * vx, ux * vx + uy * vy);
  const theta = angle(1, 0, (x1p - cxp) / rx, (y1p - cyp) / ry);
  let delta = angle((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry);
  if (!sweep && delta > 0) {
    delta -= 2 * Math.PI;
  } else if (sweep && delta < 0) {
    delta += 2 * Math.PI;
  }

  // Eighth turns at most, like `<Ellipse>`'s arcs.
  const count = Math.max(1, Math.ceil(Math.abs(delta) / (Math.PI / 4) - 1e-9));
  const step = delta / count;
  const k = (4 / 3) * Math.tan(step / 4);
  // A point on the arc at `t` radians, and the arc's direction there scaled by `k`.
  const at = (t: number): [number, number] => [
    cx + rx * Math.cos(t) * cos - ry * Math.sin(t) * sin,
    cy + rx * Math.cos(t) * sin + ry * Math.sin(t) * cos,
  ];
  const along = (t: number): [number, number] => [
    k * (-rx * Math.sin(t) * cos - ry * Math.cos(t) * sin),
    k * (-rx * Math.sin(t) * sin + ry * Math.cos(t) * cos),
  ];
  const commands: PathCommand[] = [];
  for (let i = 0; i < count; i += 1) {
    const t0 = theta + step * i;
    const t1 = t0 + step;
    const [px, py] = at(t0);
    const [qx, qy] = i === count - 1 ? [x, y] : at(t1);
    const [ax, ay] = along(t0);
    const [bx, by] = along(t1);
    commands.push({ type: 'cubicTo', x1: px + ax, y1: py + ay, x2: qx - bx, y2: qy - by, x: qx, y: qy });
  }
  return commands;
}

/**
 * Writes path commands as SVG path data, for an SVG `<path d>`: absolute
 * `M L Q C Z` with each number written in full, so
 * `parseSvgPath(toSvgPath(commands))` gives the same commands back.
 */
export function toSvgPath(commands: readonly PathCommand[]): string {
  return commands
    .map((command) => {
      switch (command.type) {
        case 'moveTo':
          return `M${command.x} ${command.y}`;
        case 'lineTo':
          return `L${command.x} ${command.y}`;
        case 'quadTo':
          return `Q${command.x1} ${command.y1} ${command.x} ${command.y}`;
        case 'cubicTo':
          return `C${command.x1} ${command.y1} ${command.x2} ${command.y2} ${command.x} ${command.y}`;
        case 'close':
          return 'Z';
      }
    })
    .join('');
}
