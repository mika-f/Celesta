import { t, text } from './i18n';
import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import initialSource from './demo/title-scene.tsx?raw';
import { Engine, SceneCanvas, exportMp4, type CompositionConfig } from '@celesta/web';
const SourceEditor = lazy(() => import('./SourceEditor'));

function download(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

function timecode(frame: number, fps: number) {
  const seconds = Math.floor(frame / fps);
  return `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}:${String(frame % fps).padStart(2, '0')}`;
}

export function Playground({ initialFiles, entry = 'my-first-scene.tsx', initialFrame = 60, baseURL, silent = false, title }: {
  initialFiles?: Record<string, string>; entry?: string; initialFrame?: number; baseURL?: string; silent?: boolean; title?: string;
} = {}) {
  const [files, setFiles] = useState(initialFiles ?? { [entry]: initialSource });
  const [activeEntry, setActiveEntry] = useState(entry);
  const [name, setName] = useState(entry);
  const [editorVersion, setEditorVersion] = useState(0);
  const source = files[name];
  const [engine, setEngine] = useState<Engine | null>(null);
  const [config, setConfig] = useState<CompositionConfig | null>(null);
  const [renderer, setRenderer] = useState(() => new SceneCanvas(new Map(), baseURL));
  const [frame, setFrame] = useState(initialFrame);
  const [playing, setPlaying] = useState(false);
  const [message, setMessage] = useState(t('playground.status.preparing'));
  const [error, setError] = useState('');
  const [exporting, setExporting] = useState(false);
  const [progress, setProgress] = useState(0);
  const [mediaNames, setMediaNames] = useState<string[]>([]);
  const [hasAudio, setHasAudio] = useState(false);
  const canvas = useRef<HTMLCanvasElement>(null);
  const controller = useRef<AbortController | null>(null);
  const frameRequest = useRef(0);
  const drawQueue = useRef(Promise.resolve());
  const compiledFiles = useRef<Record<string, string> | null>(null);

  useEffect(() => {
    let current = true;
    let next: Engine | null = null;
    setEngine(null);
    setConfig(null);
    setHasAudio(false);
    setError('');
    const timeout = setTimeout(() => {
      setMessage(t('playground.status.compiling'));
      next = new Engine();
      void next.compile(files[activeEntry], { files, entry: activeEntry, baseURL, silent }).then(result => {
        if (!current) return;
        compiledFiles.current = files;
        setEngine(next);
        setConfig(result);
        setError('');
        setMessage(t('playground.status.ready'));
        setFrame(previous => Math.min(previous, result.durationInFrames - 1));
      }).catch(cause => {
        if (!current) return;
        setEngine(null);
        setConfig(null);
        setError(cause instanceof Error ? cause.message : String(cause));
        setMessage(t('playground.status.fix-code'));
      });
    }, 450);
    return () => { current = false; clearTimeout(timeout); next?.dispose(); };
  }, [files, activeEntry, baseURL, silent]);

  useEffect(() => {
    if (!engine || !canvas.current || exporting) return;
    const request = ++frameRequest.current;
    void engine.frame(frame).then(({ scene, audio }) => {
      setHasAudio(audio.length > 0);
      drawQueue.current = drawQueue.current.catch(() => {}).then(async () => {
        if (request !== frameRequest.current || !canvas.current) return;
        const scratch = document.createElement('canvas');
        await renderer.draw(scratch, scene);
        if (request !== frameRequest.current || !canvas.current) return;
        canvas.current.width = scene.width;
        canvas.current.height = scene.height;
        canvas.current.getContext('2d')?.drawImage(scratch, 0, 0);
        setError('');
      });
      return drawQueue.current;
    }).catch(cause => {
      if (request === frameRequest.current) { setPlaying(false); setError(cause instanceof Error ? cause.message : String(cause)); }
    });
    return () => { frameRequest.current++; };
  }, [engine, renderer, frame, exporting]);

  useEffect(() => {
    if (!playing || !config || exporting) return;
    const fps = config.frameRate.numerator / config.frameRate.denominator;
    const last = config.durationInFrames - 1;
    const startFrame = frame;
    const start = performance.now();
    let request: number;
    const tick = (now: number) => {
      const next = Math.min(last, startFrame + Math.floor((now - start) * fps / 1000));
      setFrame(next);
      if (next < last) request = requestAnimationFrame(tick);
      else setPlaying(false);
    };
    request = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(request);
    // Playback must retain its starting frame until paused.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [playing, config, exporting]);

  useEffect(() => () => { renderer.dispose(); controller.current?.abort(); }, [renderer]);

  function addMedia(files: FileList | null) {
    if (!files) return;
    const assets = new Map(Array.from(files, file => [file.name, file]));
    setMediaNames(Array.from(assets.keys()));
    setRenderer(new SceneCanvas(assets, baseURL));
  }

  async function openSource(file: File | undefined) {
    if (!file) return;
    const source = await file.text();
    setPlaying(false);
    setFiles(previous => initialFiles ? { ...previous, [file.name]: source } : { [file.name]: source });
    if (!initialFiles) {
      setEditorVersion(version => version + 1);
      setActiveEntry(file.name);
    }
    setName(file.name);
  }

  function resetSample() {
    if (!initialFiles) return;
    setPlaying(false);
    setFiles(initialFiles);
    setEditorVersion(version => version + 1);
    setName(entry);
    setActiveEntry(entry);
    setFrame(initialFrame);
    setMediaNames([]);
    setRenderer(new SceneCanvas(new Map(), baseURL));
  }

  async function exportFile() {
    if (!engine || !config || compiledFiles.current !== files) return;
    setPlaying(false);
    setExporting(true);
    setProgress(0);
    setError('');
    setMessage(t('playground.status.exporting'));
    const abort = new AbortController();
    controller.current = abort;
    try {
      const blob = await exportMp4(engine, config, renderer, abort.signal, (done, total) => setProgress(Math.round(done / total * 100)));
      download(blob, activeEntry.replace(/\.[^.]+$/, '') + '.mp4');
      setMessage(t('playground.status.downloaded'));
    } catch (cause) {
      if (abort.signal.aborted) setMessage(t('playground.status.canceled'));
      else { setError(cause instanceof Error ? cause.message : String(cause)); setMessage(t('playground.status.failed')); }
    } finally {
      controller.current = null;
      setExporting(false);
    }
  }

  const fps = config ? config.frameRate.numerator / config.frameRate.denominator : 30;
  const last = (config?.durationInFrames ?? 1) - 1;
  const ready = !!engine && !!config && !error && compiledFiles.current === files;

  return <section id="playground" aria-labelledby="playground-heading" className="playground-section shell">
    <div className="section-top">
      <p className="eyebrow">{text('playground.label')}</p><h2 id="playground-heading">{title ?? text('playground.the-scene-below-is-code-change-a', [<br />, <em />])}</h2>
      <span className="hand-note">{initialFiles ? t('showcase.edit-hint') : text('playground.try-editing-title')}</span>
    </div>
    <div className="studio">
      <div className="studio-bar flex items-center justify-between">{text('playground.celesta-web-editor', [<span />, name, <span className="flex items-center gap-2" />, <i className="status-dot" />])}</div>
      <div className="studio-main grid">
        <div className="source-panel">
          {initialFiles && <div className="source-files"><label>{t('showcase.source-file')}<select aria-label={t('showcase.source-file')} value={name} disabled={exporting} onChange={event => setName(event.target.value)}>{Object.keys(files).sort().map(path => <option key={path} value={path}>{path}</option>)}</select></label><button disabled={exporting} onClick={resetSample}>{t('showcase.reset')}</button></div>}
          <div className="flex items-center justify-between source-heading">{text('playground.tsx-your-composition-copy-source', [<span />, <b />, <button onClick={() => { void navigator.clipboard.writeText(source).then(() => setMessage(t('playground.status.copied-source'))).catch(() => setMessage(t('playground.status.copy-source-failed'))); }} className="copy-button" />])}</div>
          <div className="source-scroll"><div className="source-editor"><Suspense fallback={<p className="editor-loading" role="status">{t('playground.editor-loading')}</p>}><SourceEditor key={editorVersion} name={name} source={source} readOnly={exporting} onChange={value => { setPlaying(false); setFiles(previous => ({ ...previous, [name]: value })); }} /></Suspense></div></div>
          <div className="source-footer flex items-center justify-between gap-2"><span>{text('playground.lines-not-saved-download-to-keep-it', [source.split('\n').length])}</span><div className="flex gap-3">{text('playground.open-tsx-download-tsx', [<label className="file-action" />, <input type="file" accept=".tsx,.jsx,.ts,.js" onChange={event => { void openSource(event.target.files?.[0]); event.target.value = ''; }} />, <button onClick={() => download(new Blob([source], { type: 'text/plain;charset=utf-8' }), name)} />])}</div></div>
        </div>
        <div className="preview-panel">
          <div className="preview-heading flex justify-between">{text('playground.composition-preview', [<span />, <span />, config ? `${config.width} × ${config.height} · ${fps} FPS` : t('playground.status.waiting')])}</div>
          <canvas ref={canvas} className="scene" width={1920} height={1080} role="img" aria-label={t('playground.controls.preview-at', { frame })} />
          <div className="transport flex items-center gap-4">
            <button className="play-button" disabled={!ready || exporting} aria-label={playing ? t('playground.controls.pause') : t('playground.controls.play')} onClick={() => { if (!playing && frame >= last) setFrame(0); setPlaying(!playing); }}>{playing ? <span aria-hidden="true">Ⅱ</span> : <svg aria-hidden="true" viewBox="0 0 20 20"><path d="m7 4 10 6-10 6z" fill="currentColor" /></svg>}</button>
            <input type="range" min="0" max={last} value={Math.min(frame, last)} disabled={!ready || exporting} aria-label={t('playground.preview-frame')} aria-valuetext={t('playground.controls.frame-of', { frame, last })} onChange={event => { setPlaying(false); setFrame(Number(event.target.value)); }} />
            <output className="timecode">{timecode(frame, fps)}</output>
          </div>
          <div className="studio-tools">{text('playground.add-media', [<label className="file-action" />, <input type="file" multiple accept="image/*,video/*,audio/*" onChange={event => { addMedia(event.target.files); event.target.value = ''; }} />, <span />, mediaNames.length ? mediaNames.join(', ') : t('playground.media-hint')])}</div>
          <div className="studio-export">{text('playground.export-mp4', [<button className="button button-primary" disabled={!ready || exporting} onClick={() => { void exportFile(); }} />, exporting && <button onClick={() => controller.current?.abort()}>{t('playground.controls.cancel')}</button>, exporting && <progress aria-label={t('playground.controls.export-progress')} max="100" value={progress} />])}</div>
          <p className={`studio-status ${error ? 'is-error' : ''}`} role="status">{error || message}</p>
          {hasAudio && <p className="studio-audio-note">{text('playground.video-preview-is-silent-mp4-export-includes')}</p>}
        </div>
      </div>
    </div>
    <div className="studio-caption flex flex-wrap justify-between gap-2"><p>{text('playground.esbuild-compiles-your-tsx-in-a-web')}</p><span>{text('playground.runs-entirely-in-your-browser')}</span></div>
  </section>;
}
