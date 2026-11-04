import { Download, Pause, Play } from 'lucide-react';
import { cloneElement, lazy, Suspense, useEffect, useRef, useState, type ReactElement, type ReactNode } from 'react';
import initialSource from './demo/title-scene.tsx?raw';
import { Engine, SceneCanvas, exportMp4, type CompositionConfig } from '@celesta.studio/web';
import type { PlaygroundCopy, Slot } from './copy.ts';
const SourceEditor = lazy(() => import('./SourceEditor.tsx'));

/** A copy slot that renders `element` around the translated children. */
const wrap = (element: ReactElement<{ children?: ReactNode }>): Slot => children => cloneElement(element, undefined, children);
/** A copy slot that renders a value in place of the slot. */
const value = (node: ReactNode): Slot => () => node;

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

export function Playground({ copy: c, initialFiles, entry = 'my-first-scene.tsx', initialFrame = 60, baseURL, silent = false, title }: {
  copy: PlaygroundCopy; initialFiles?: Record<string, string>; entry?: string; initialFrame?: number; baseURL?: string; silent?: boolean; title?: string;
}) {
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
  const [message, setMessage] = useState(c.status.preparing);
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
      setMessage(c.status.compiling);
      next = new Engine();
      void next.compile(files[activeEntry], { files, entry: activeEntry, baseURL, silent }).then(result => {
        if (!current) return;
        compiledFiles.current = files;
        setEngine(next);
        setConfig(result);
        setError('');
        setMessage(c.status.ready);
        setFrame(previous => Math.min(previous, result.durationInFrames - 1));
      }).catch(cause => {
        if (!current) return;
        setEngine(null);
        setConfig(null);
        setError(cause instanceof Error ? cause.message : String(cause));
        setMessage(c.status.fixCode);
      });
    }, 450);
    // Unmounting (or a new compile) terminates this compile's worker.
    return () => { current = false; clearTimeout(timeout); next?.dispose(); };
  }, [files, activeEntry, baseURL, silent, c]);

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
    setMessage(c.status.exporting);
    const abort = new AbortController();
    controller.current = abort;
    try {
      const blob = await exportMp4(engine, config, renderer, abort.signal, (done, total) => setProgress(Math.round(done / total * 100)));
      download(blob, activeEntry.replace(/\.[^.]+$/, '') + '.mp4');
      setMessage(c.status.downloaded);
    } catch (cause) {
      if (abort.signal.aborted) setMessage(c.status.canceled);
      else { setError(cause instanceof Error ? cause.message : String(cause)); setMessage(c.status.failed); }
    } finally {
      controller.current = null;
      setExporting(false);
    }
  }

  const fps = config ? config.frameRate.numerator / config.frameRate.denominator : 30;
  const last = (config?.durationInFrames ?? 1) - 1;
  const ready = !!engine && !!config && !error && compiledFiles.current === files;

  return <div className="pg">
    <div className="pg-top">
      <p className="pg-eyebrow">{c.label}</p><h2 id="playground-heading">{title ?? c.theSceneBelowIsCodeChangeA(wrap(<br />), wrap(<em />))}</h2>
      <span className="pg-hint">{initialFiles ? c.showcase.editHint : c.tryEditingTitle}</span>
    </div>
    <div className="pg-studio">
      <div className="pg-bar">{c.celestaWebEditor(wrap(<span />), value(name), wrap(<span className="pg-bar-state" />), wrap(<i className="pg-lamp" />))}</div>
      <div className="pg-main">
        <div className="pg-source">
          {initialFiles && <div className="pg-files"><label>{c.showcase.sourceFile}<select aria-label={c.showcase.sourceFile} value={name} disabled={exporting} onChange={event => setName(event.target.value)}>{Object.keys(files).sort().map(path => <option key={path} value={path}>{path}</option>)}</select></label><button type="button" disabled={exporting} onClick={resetSample}>{c.showcase.reset}</button></div>}
          <div className="pg-source-heading">{c.tsxYourCompositionCopySource(wrap(<span />), wrap(<b />), wrap(<button type="button" onClick={() => { void navigator.clipboard.writeText(source).then(() => setMessage(c.status.copiedSource)).catch(() => setMessage(c.status.copySourceFailed)); }} className="pg-link-button" />))}</div>
          <div className="pg-source-scroll"><div className="pg-editor"><Suspense fallback={<p className="pg-editor-loading" role="status">{c.editorLoading}</p>}><SourceEditor key={editorVersion} name={name} source={source} readOnly={exporting} loading={c.editorLoading} label={c.compositionSourceCode} onChange={value => { setPlaying(false); setFiles(previous => ({ ...previous, [name]: value })); }} /></Suspense></div></div>
          <div className="pg-source-footer"><span>{c.linesNotSavedDownloadToKeepIt(value(source.split('\n').length))}</span><div className="pg-source-actions">{c.openTsxDownloadTsx(wrap(<label className="pg-file-action" />), wrap(<input type="file" accept=".tsx,.jsx,.ts,.js" onChange={event => { void openSource(event.target.files?.[0]); event.target.value = ''; }} />), children => <button type="button" onClick={() => download(new Blob([source], { type: 'text/plain;charset=utf-8' }), name)}><Download size={14} aria-hidden="true" />{children}</button>)}</div></div>
        </div>
        <div className="pg-preview">
          <div className="pg-preview-heading">{c.compositionPreview(wrap(<span />), wrap(<span />), value(config ? `${config.width} × ${config.height} · ${fps} FPS` : c.status.waiting))}</div>
          <canvas ref={canvas} className="pg-scene" width={1920} height={1080} role="img" aria-label={c.controls.previewAt({ frame })} />
          <div className="pg-transport">
            <button type="button" className="pg-play" disabled={!ready || exporting} aria-label={playing ? c.controls.pause : c.controls.play} onClick={() => { if (!playing && frame >= last) setFrame(0); setPlaying(!playing); }}>{playing ? <Pause size={14} fill="currentColor" aria-hidden="true" /> : <Play size={14} fill="currentColor" aria-hidden="true" />}</button>
            <input type="range" min="0" max={last} value={Math.min(frame, last)} disabled={!ready || exporting} aria-label={c.previewFrame} aria-valuetext={c.controls.frameOf({ frame, last })} onChange={event => { setPlaying(false); setFrame(Number(event.target.value)); }} />
            <output className="pg-timecode">{timecode(frame, fps)}</output>
          </div>
          <div className="pg-tools">{c.addMedia(wrap(<label className="pg-file-action" />), wrap(<input type="file" multiple accept="image/*,video/*,audio/*" onChange={event => { addMedia(event.target.files); event.target.value = ''; }} />), wrap(<span />), value(mediaNames.length ? mediaNames.join(', ') : c.mediaHint))}</div>
          <div className="pg-export">{c.exportMp4(children => <button type="button" className="pg-button" disabled={!ready || exporting} onClick={() => { void exportFile(); }}><Download size={16} strokeWidth={1.75} aria-hidden="true" />{children}</button>, value(exporting && <button type="button" onClick={() => controller.current?.abort()}>{c.controls.cancel}</button>), value(exporting && <progress aria-label={c.controls.exportProgress} max="100" value={progress} />))}</div>
          <p className={`pg-status ${error ? 'is-error' : ''}`} role="status">{error || message}</p>
          {hasAudio && <p className="pg-audio-note">{c.videoPreviewIsSilentMp4ExportIncludes}</p>}
        </div>
      </div>
    </div>
    <div className="pg-caption"><p>{c.esbuildCompilesYourTsxInAWeb}</p><span>{c.runsEntirelyInYourBrowser}</span></div>
  </div>;
}
