// Excerpts of bench/*/ (the opacity expression is shortened to `twinkle`).
import type { ToolId } from './constants';

export type Snippet = { lang: 'tsx' | 'rust'; source: string; note: string; marks: number[] };

export const PARTICLE: Record<ToolId, Snippet> = {
  remotion: {
    lang: 'tsx',
    note: 'DOM and CSS. Centering is your own arithmetic.',
    marks: [3, 5],
    source: `const a = rand(i, 2) * TAU + t * (0.1 + rand(i, 3) * 0.4) * (rand(i, 4) < 0.5 ? -1 : 1);
return <div key={i} style={{
  position: 'absolute', width: size, height: size, borderRadius: '50%',
  left: 960 + Math.cos(a) * r - size / 2, top: 540 + Math.sin(a) * r * 0.45 - size / 2,
  background: STARS[i % 4], opacity: twinkle,
}} />;`,
  },
  fframes: {
    lang: 'rust',
    note: 'Rust and an SVG macro: float literals like 2. and a.cos().',
    marks: [1, 3],
    source: `let dir = if rand(i, 4) < 0.5 { -1. } else { 1. };
let a = rand(i, 2) * TAU + t * (0.1 + rand(i, 3) * 0.4) * dir;
fframes::svgr!(<circle cx={960. + a.cos() * r} cy={540. + a.sin() * r * 0.45} r={size / 2.}
    fill={STARS[i % 4]} opacity={twinkle} />)`,
  },
  celesta: {
    lang: 'tsx',
    note: 'Still React. Anchors and corners are props.',
    marks: [3],
    source: `const a = rand(i, 2) * TAU + t * (0.1 + rand(i, 3) * 0.4) * (rand(i, 4) < 0.5 ? -1 : 1);
return <Rect key={i} x={960 + Math.cos(a) * r} y={540 + Math.sin(a) * r * 0.45}
  anchorX={0.5} anchorY={0.5} width={size} height={size} cornerRadius={size / 2}
  fill={STARS[i % 4]} opacity={twinkle} />;`,
  },
};

export const EFFECTS: Record<ToolId, Snippet> = {
  remotion: {
    lang: 'tsx',
    note: 'CSS filters. drop-shadow needs double the radius.',
    marks: [2, 4],
    source: `<div style={{ ...circle, background: color, opacity: 0.55,
  filter: 'blur(48px)', mixBlendMode: 'screen' }} />
// drop-shadow's radius is twice the Gaussian's standard deviation (24).
<div style={{ filter: 'drop-shadow(0 0 48px #7B5CFF)' }}>{title}</div>`,
  },
  fframes: {
    lang: 'rust',
    note: 'SVG filters declared in <defs>, referenced by url(#id).',
    marks: [2, 4, 5],
    source: `<filter id="blur" x="-50%" y="-50%" width="200%" height="200%">
    <feGaussianBlur stdDeviation="48" />
</filter>
<circle cx={cx} cy={cy} r={r} fill={*color} opacity="0.55" filter="url(#blur)" style="mix-blend-mode:screen" />
<g filter="url(#glow)">{title}</g>`,
  },
  celesta: {
    lang: 'tsx',
    note: 'Blur, blend mode and glow: one prop each.',
    marks: [2, 3],
    source: `<Rect {...circle} fill={color} opacity={0.55}
  blur={48} blendMode="screen" />
<Group glow={{ color: '#7B5CFF', blur: 24 }}>{title}</Group>`,
  },
};

export const FILES: Record<ToolId, { files: [string, string][]; command: string }> = {
  remotion: {
    files: [['package.json', 'dependencies'], ['remotion.config.ts', 'config'], ['src/index.ts', 'registerRoot()'],
      ['src/Root.tsx', '<Composition>'], ['src/Nebula.tsx', 'the scene']],
    command: 'npx remotion render Nebula out.mp4',
  },
  fframes: {
    files: [['Cargo.toml', 'crates, features'], ['src/main.rs', 'CLI, GPU, encoder'], ['src/lib.rs', 'impl Video']],
    command: 'cargo run --release -- render',
  },
  celesta: {
    files: [['nebula.tsx', 'scene + <Composition>']],
    command: 'celesta-export --react nebula.tsx out.mp4',
  },
};
