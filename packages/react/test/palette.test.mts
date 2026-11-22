// Palettes from definePalettes(): roles, derived colors, contrast checks,
// <Theme palette>, and picking a palette through a project property.

import { afterEach, expect, test, vi } from 'vitest';
import React from 'react';
import type { ReactNode } from 'react';

import {
  Composition,
  Rect,
  Text,
  Theme,
  alpha,
  contrastRatio,
  defineProjectProperties,
  definePalettes,
  getProjectProperty,
  mix,
  useTheme,
} from '../src/index.js';
import * as browser from '../src/browser.js';
import type { PaletteRoles, ResolvedTheme } from '../src/index.js';
import { setProjectPropertyValues } from '../src/properties.js';
import { mount } from '../src/render.js';

import { colorOf, contentOf } from './helpers.mjs';

const h = React.createElement;

function scene(children: ReactNode) {
  const Root = () => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 30 }, children);
  return mount(Root).renderAt({ value: 0, timescale: 30 }, null).scene;
}

const harbor: PaletteRoles = { ground: '#0F2A3D', main: '#5FD3B5', second: '#EAF4F2', accent: '#F2B544' };
const paper: PaletteRoles = { ground: '#F4F1EA', main: '#1F4E79', second: '#111111', accent: '#FF4D1F' };
const derived = { panel: mix('ground', 'main', 0.25), veil: alpha('second', 0.2), deep: mix('panel', '#000000', 0.5) };

afterEach(() => {
  vi.restoreAllMocks();
  defineProjectProperties({});
  setProjectPropertyValues({}, {});
});

test('a palette names its roles and derived colors', () => {
  const { colors, roles, name } = definePalettes({ harbor }, { derived }).get();
  expect(name).toBe('harbor');
  expect(roles).toEqual(harbor);
  // 75% ground and 25% main, channel by channel: 0x0F*.75 + 0x5F*.25 = 0x23.
  expect(colors).toEqual({
    ...harbor,
    panel: '#23545B',
    veil: '#EAF4F233',
    // Half of panel, which was defined before it, toward black.
    deep: '#122A2E',
  });
});

test('mix() blends see-through colors like CSS color-mix, premultiplied', () => {
  const { colors } = definePalettes({ harbor }, {
    derived: { half: mix('#FF000000', '#0000FF', 0.5), clear: mix('#FF000000', '#00FF0000', 0.5), none: alpha('accent', 0) },
  }).get();
  // The transparent red adds no color: the blend is blue at half opacity.
  expect(colors.half).toBe('#0000FF80');
  expect(colors.clear).toBe('#00000000');
  expect(colors.none).toBe('#F2B54400');
});

test('an opaque blend stays #RRGGBB even when rounding lands just under full opacity', () => {
  const { colors } = definePalettes({ harbor }, { derived: { tint: mix('ground', 'main', 0.04), faint: mix('ground', 'accent', 0.37) } }).get();
  expect(colors.tint).toMatch(/^#[0-9A-F]{6}$/);
  expect(colors.faint).toMatch(/^#[0-9A-F]{6}$/);
});

test('whole-number names are refused, since an object reorders them', () => {
  expect(() => definePalettes({ '2026': harbor, '2025': paper })).toThrow(
    'palette name "2025" is a whole number, which an object does not keep in the order written; give it a name that starts with a letter, such as "v2025"',
  );
  expect(() => definePalettes({ harbor }, { derived: { base: alpha('main', 0.5), 1: mix('base', 'main', 0.5) } })).toThrow(
    'derived color name "1" is a whole number',
  );
  // Names that only contain digits among other characters keep their order.
  expect(definePalettes({ y2026: harbor, '2025b': paper, '01': paper }).names).toEqual(['y2026', '2025b', '01']);
  // Only whole numbers below 2^32 - 1 are reordered; larger ones keep their place.
  expect(() => definePalettes({ harbor, '4294967294': paper })).toThrow('palette name "4294967294" is a whole number');
  expect(definePalettes({ harbor, '4294967295': paper, '1763769600000': paper }).names).toEqual(['harbor', '4294967295', '1763769600000']);
});

test('get() picks a palette by name, the default without one, and refuses other names', () => {
  const palettes = definePalettes({ harbor, paper }, { default: 'paper' });
  expect(palettes.names).toEqual(['harbor', 'paper']);
  expect(palettes.get().name).toBe('paper');
  expect(palettes.get('harbor').colors.ground).toBe('#0F2A3D');
  expect(() => palettes.get('night')).toThrow('"night" is not a palette (harbor, paper)');
  expect(() => palettes.get(3)).toThrow('3 is not a palette');
  expect(() => palettes.get('toString')).toThrow('is not a palette');
});

test('property() is a select field over the palette names', () => {
  const palettes = definePalettes({ harbor, paper });
  expect(palettes.property({ label: 'Palette' })).toEqual({
    type: 'select', label: 'Palette', defaultValue: 'harbor', options: ['harbor', 'paper'],
  });
  expect(palettes.property({ defaultValue: 'paper' })).toEqual({ type: 'select', defaultValue: 'paper', options: ['harbor', 'paper'] });
  expect(() => palettes.property({ defaultValue: 'night' })).toThrow('is not a palette');
});

test('roles must be hex colors, all four, and nothing else', () => {
  expect(() => definePalettes({ bad: { ...harbor, main: 'teal' } })).toThrow(
    'palette "bad": main must be a #RRGGBB or #RRGGBBAA color, got "teal"',
  );
  const { accent: _, ...three } = harbor;
  expect(() => definePalettes({ bad: three as PaletteRoles })).toThrow('accent must be a #RRGGBB or #RRGGBBAA color, got undefined');
  expect(() => definePalettes({ bad: { ...harbor, ink: '#000000' } as PaletteRoles })).toThrow('"ink" is not a role');
  expect(() => definePalettes({})).toThrow('at least one palette');
  expect(() => definePalettes({ harbor }, { default: 'paper' })).toThrow('default "paper" is not a palette (harbor)');
});

test('derived colors must use known names, amounts from 0 to 1, and not reuse a role name', () => {
  expect(() => definePalettes({ harbor }, { derived: { a: mix('ground', 'later', 0.5), later: alpha('main', 0.5) } })).toThrow(
    'palette color "a" uses "later", which is neither a hex color nor a role or derived color defined before it (ground, main, second, accent)',
  );
  expect(() => definePalettes({ harbor }, { derived: { a: alpha('main', 1.5) } })).toThrow('amount must be a number from 0 to 1, got 1.5');
  expect(() => definePalettes({ harbor }, { derived: { main: alpha('main', 0.5) } })).toThrow('"main" is a role');
  expect(() => definePalettes({ harbor }, { derived: { a: '#000000' as never } })).toThrow('must be made with mix() or alpha()');
});

test('contrastRatio() follows WCAG, laying a see-through color over the background', () => {
  expect(contrastRatio('#000000', '#FFFFFF')).toBeCloseTo(21, 5);
  expect(contrastRatio('#FFFFFF', '#FFFFFF')).toBeCloseTo(1, 5);
  expect(contrastRatio('#777777', '#FFFFFF')).toBeCloseTo(4.48, 2);
  expect(contrastRatio('#00000000', '#FFFFFF')).toBeCloseTo(1, 5);
  expect(() => contrastRatio('black', '#FFFFFF')).toThrow('takes hex colors');
});

test('a second below 4.5:1 on its ground is warned about and listed by check()', () => {
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
  const palettes = definePalettes({ harbor, dim: { ...paper, second: '#999999' } });
  expect(warn).toHaveBeenCalledTimes(1);
  expect(warn.mock.calls[0]?.[0]).toBe(
    'Celesta: palette "dim": second #999999 on ground #F4F1EA has 2.52:1 contrast, below 4.5:1 for text',
  );
  const issues = palettes.check();
  expect(issues).toHaveLength(1);
  expect(issues[0]).toMatchObject({ palette: 'dim', code: 'low-contrast' });
  expect(issues[0]!.contrast).toBeCloseTo(2.526, 3);
  // A palette that passes warns about nothing.
  warn.mockClear();
  expect(definePalettes({ harbor, paper }).check()).toEqual([]);
  expect(warn).not.toHaveBeenCalled();
});

test('<Theme palette> names the roles and derived colors for shapes and text', () => {
  const palette = definePalettes({ harbor }, { derived }).get();
  const [ground, veil, text] = scene(h(Theme, { palette },
    h(Rect, { key: 'a', width: 10, height: 10, fill: 'ground' }),
    h(Rect, { key: 'b', width: 10, height: 10, fill: 'veil' }),
    h(Text, { key: 'c', color: 'second', children: 'A' }),
  )).layers;
  expect(colorOf(contentOf(ground, 'rect').fill)).toBe('#0F2A3D');
  expect(colorOf(contentOf(veil, 'rect').fill)).toBe('#EAF4F233');
  expect(colorOf(contentOf(text, 'text').style.fill)).toBe('#EAF4F2');
});

test('colors given with a palette win, and an inner palette replaces the outer one', () => {
  const palettes = definePalettes({ harbor, paper }, { derived });
  let seen: ResolvedTheme | undefined;
  const Probe = () => { seen = useTheme(); return null; };
  scene(h(Theme, { palette: palettes.get('harbor'), colors: { accent: '#123456', ink: '#000000' } },
    h(Theme, { palette: palettes.get('paper') }, h(Probe)),
  ));
  expect(seen?.palette?.name).toBe('paper');
  // The inner palette's names win over the outer theme's, its own accent included.
  expect(seen?.colors).toMatchObject({ ground: '#F4F1EA', accent: '#FF4D1F', ink: '#000000' });

  scene(h(Theme, { palette: palettes.get('harbor'), colors: { accent: '#123456' } }, h(Probe)));
  expect(seen?.colors.accent).toBe('#123456');
  expect(seen?.palette?.name).toBe('harbor');
  // A nested theme without a palette keeps the outer one.
  scene(h(Theme, { palette: palettes.get('harbor') }, h(Theme, { colors: { ink: '#000000' } }, h(Probe))));
  expect(seen?.palette?.name).toBe('harbor');
});

test('<Theme palette> refuses what is not a palette', () => {
  expect(() => scene(h(Theme, { palette: { name: 'x' } as never }))).toThrow('<Theme> palette must be a palette from definePalettes()');
  expect(() => scene(h(Theme, { palette: { name: 'x', roles: harbor, colors: { ground: 'navy' } } }))).toThrow(
    '<Theme> palette color "ground" must be a #RRGGBB or #RRGGBBAA color, got "navy"',
  );
});

test('a palette property picks the palette from outside values', () => {
  const palettes = definePalettes({ harbor, paper });
  defineProjectProperties({ palette: palettes.property({ label: 'Palette' }) });
  const Film = () => h(Theme, { palette: palettes.get(getProjectProperty('palette')) },
    h(Rect, { width: 10, height: 10, fill: 'ground' }));
  expect(colorOf(contentOf(scene(h(Film)).layers[0], 'rect').fill)).toBe('#0F2A3D');
  setProjectPropertyValues({ palette: 'paper' }, {});
  expect(colorOf(contentOf(scene(h(Film)).layers[0], 'rect').fill)).toBe('#F4F1EA');
});

test('the browser entry exports the palette API too', () => {
  expect(browser.definePalettes).toBe(definePalettes);
  expect(browser.mix).toBe(mix);
  expect(browser.alpha).toBe(alpha);
  expect(browser.contrastRatio).toBe(contrastRatio);
});
