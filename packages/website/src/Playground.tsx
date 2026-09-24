import { createElement, useEffect, useRef, useState, type ReactNode } from 'react';
import sceneSource from './demo/title-scene.tsx?raw';
import Root, { Scene } from './demo/title-scene';
import { compositionConfig, drawScene } from './demo/celesta-browser';

const palettes = [
  { name: 'Lilac', background: '#e4daf0', ink: '#57456c', accent: '#a68bbf' },
  { name: 'Peach', background: '#f4dfd0', ink: '#764e43', accent: '#d09880' },
  { name: 'Sage', background: '#dce8df', ink: '#425e50', accent: '#8bac98' },
];

const config = compositionConfig(Root);
const lastFrame = config.durationInFrames - 1;

function quote(value: string) {
  return `'${value.replace(/\\/g, '\\\\').replace(/'/g, "\\'")}'`;
}

/** The downloadable scene, with the visitor's title and palette filled in. */
function sourceFor(title: string, { background, ink, accent }: typeof palettes[number]) {
  return sceneSource
    .replace(/^const TITLE = .*$/m, `const TITLE = ${quote(title)};`)
    .replace(/^  background: .*$/m, `  background: '${background}', ink: '${ink}', accent: '${accent}',`);
}

const tokenPattern = /(\/\/.*$)|('(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`|"(?:\\.|[^"\\])*")|\b(import|from|export|default|function|return|const|let|typeof)\b|(<\/?[A-Z][A-Za-z]*|<\/?>|\/>|(?<![=\s])>|^\s*>)|\b(\d+(?:\.\d+)?)\b/g;
const tokenClasses = ['code-comment', 'code-string', 'code-keyword', 'code-tag', 'code-number'];

function highlight(line: string): ReactNode[] {
  const parts: ReactNode[] = [];
  let last = 0;
  for (const match of line.matchAll(tokenPattern)) {
    const group = match.slice(1).findIndex(Boolean);
    if (match.index > last) parts.push(line.slice(last, match.index));
    parts.push(<span key={match.index} className={tokenClasses[group]}>{match[0]}</span>);
    last = match.index + match[0].length;
  }
  if (last < line.length) parts.push(line.slice(last));
  return parts;
}

function timecode(frame: number) {
  const seconds = Math.floor(frame / config.fps);
  return `00:${String(seconds).padStart(2, '0')}:${String(frame % config.fps).padStart(2, '0')}`;
}

export function Playground() {
  const [title, setTitle] = useState('Make a little magic.');
  const [paletteIndex, setPaletteIndex] = useState(0);
  const [frame, setFrame] = useState(60);
  const [playing, setPlaying] = useState(false);
  const [copied, setCopied] = useState('');
  const canvas = useRef<HTMLCanvasElement>(null);
  const palette = palettes[paletteIndex];
  const code = sourceFor(title, palette);

  useEffect(() => {
    if (!canvas.current) return;
    const { background, ink, accent } = palette;
    drawScene(canvas.current, createElement(Scene, { title, colors: { background, ink, accent } }), config, frame);
  }, [title, palette, frame]);

  useEffect(() => {
    if (!playing) return;
    const startedAt = performance.now();
    let request: number;
    // One playback pass; the last frame remains visible instead of flashing in a loop.
    const tick = (now: number) => {
      const nextFrame = Math.min(Math.floor((now - startedAt) * config.fps / 1000), lastFrame);
      setFrame(nextFrame);
      if (nextFrame < lastFrame) request = requestAnimationFrame(tick);
      else setPlaying(false);
    };
    request = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(request);
  }, [playing]);

  useEffect(() => {
    if (!copied) return;
    const timeout = setTimeout(() => setCopied(''), 3000);
    return () => clearTimeout(timeout);
  }, [copied]);

  async function copyCode() {
    try { await navigator.clipboard.writeText(code); setCopied('Copied!'); }
    catch { setCopied('Copy unavailable. Use Download .tsx below.'); }
  }

  function download() {
    const url = URL.createObjectURL(new Blob([code], { type: 'text/plain;charset=utf-8' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = 'my-first-scene.tsx';
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  const lines = code.trimEnd().split('\n');

  return <section id="playground" aria-labelledby="playground-heading" className="playground-section shell">
    <div className="section-top flex items-end justify-between gap-6">
      <div><p className="eyebrow">A SMALL IDEA, BROUGHT TO LIFE</p><h2 id="playground-heading">Meet your new creative space.</h2></div>
      <span className="hand-note">Go on, press play. <span aria-hidden="true">↴</span></span>
    </div>
    <div className="studio">
      <div className="studio-bar flex items-center justify-between">
        <div className="flex items-center gap-3"><div className="window-dots flex gap-1.5" aria-hidden="true"><i /><i /><i /></div><span>my-first-scene.tsx</span></div>
        <span className="flex items-center gap-2"><i className="status-dot" /> Interactive demo</span>
      </div>
      <div className="studio-main grid">
        <div className="source-panel">
          <div className="flex items-center justify-between source-heading"><span><b>TSX</b> Your composition</span><button onClick={copyCode} className="copy-button" aria-label="Copy composition source">Copy <span aria-hidden="true">⧉</span></button></div>
          <div className="source-scroll">
            <pre tabIndex={0} aria-label="React composition source"><code>{lines.map((line, i) => <span className={`code-line ${/^const TITLE =|^  background: /.test(line) ? 'is-live' : ''}`} key={i}><span className="line-number" aria-hidden="true">{i + 1}</span>{highlight(line)}</span>)}</code></pre>
          </div>
          <div className="source-footer flex items-center justify-between gap-2"><span>{lines.length} lines · the whole scene, nothing hidden.</span><button onClick={download}>Download .tsx <span aria-hidden="true">↓</span></button></div>
          <span className="sr-only" role="status">{copied}</span>
          {copied && <span className="copy-status" aria-hidden="true">{copied}</span>}
        </div>
        <div className="preview-panel">
          <div className="preview-heading flex justify-between"><span>COMPOSITION PREVIEW</span><span>{config.width} × {config.height} · {config.fps} FPS</span></div>
          <canvas ref={canvas} className="scene" width={config.width} height={config.height} role="img" aria-label={`Scene preview, frame ${frame}: ${title}`} />
          <div className="transport flex items-center gap-4">
            <button className="play-button" aria-label={playing ? 'Pause preview' : 'Play preview from beginning'} onClick={() => { if (!playing) setFrame(0); setPlaying(!playing); }}>{playing ? <span aria-hidden="true">Ⅱ</span> : <svg aria-hidden="true" viewBox="0 0 20 20"><path d="m7 4 10 6-10 6z" fill="currentColor" /></svg>}</button>
            <input type="range" min="0" max={lastFrame} value={frame} aria-label="Preview frame" aria-valuetext={`Frame ${frame} of ${lastFrame}`} onChange={event => { setPlaying(false); setFrame(Number(event.target.value)); }} />
            <output className="timecode">{timecode(frame)}</output>
          </div>
          <div className="scene-controls flex items-end justify-between gap-4">
            <label className="title-control">MAKE IT YOURS<input value={title} maxLength={40} onChange={event => setTitle(event.target.value)} aria-label="Scene title" /></label>
            <fieldset><legend>PALETTE</legend><div className="flex gap-2">{palettes.map((p, i) => <button key={p.name} className="swatch" style={{ background: p.background }} aria-label={`${p.name} palette`} aria-pressed={i === paletteIndex} onClick={() => setPaletteIndex(i)}>{i === paletteIndex ? <span aria-hidden="true">✓</span> : null}</button>)}</div></fieldset>
          </div>
        </div>
      </div>
    </div>
    <div className="studio-caption flex flex-wrap justify-between gap-2"><p>The preview draws this exact file in your browser. Download it and open it in Celesta to render the real thing.</p><span>WRITE. PREVIEW. MAKE IT YOURS.</span></div>
  </section>;
}
