/** What to export: a React entry or a JSON project, with its inputs. */
export interface SourceOptions {
  /** A React entry (`.tsx`, `.ts`, `.jsx`, `.js`) or a JSON project (`.celesta.json`). */
  source: string;
  /** Whether `source` is a React entry. Defaults to true for anything but a `.json` file. */
  react?: boolean;
  /** The React root composition's ID; defaults to the first declared one. */
  composition?: string;
  /** A companion `.celesta.json` for `<ProjectTimeline />` / `<ProjectTrack />`. */
  project?: string;
  /** Project property values; win over `propsFile`. */
  props?: Record<string, unknown>;
  /** A JSON file of project property values. */
  propsFile?: string;
}

/** How the exporter process is found and run. */
export interface RunOptions {
  /** The `celesta-exporter` executable; see `resolveExporter()` for the default. */
  exporter?: string;
  /** Cancels the export. Files being written are removed, as on any failure. */
  signal?: AbortSignal;
  /** Called with each progress event as it happens. */
  onProgress?: (event: ProgressEvent) => void;
  /** The exporter's working directory; relative paths resolve from it. */
  cwd?: string;
  /** The exporter's environment; defaults to `process.env`. */
  env?: Record<string, string | undefined>;
}

export type RenderQuality = 'draft' | 'final';
export type GpuDriver = 'auto' | 'vulkan' | 'dx12' | 'metal' | 'gl';
/** A component setting's value. */
export type SettingValue = string | number | boolean;

/** An encoder component and the settings to give it. */
export interface EncoderOptions {
  /** `<plugin>/<id>`; the muxer's default or the preset's when left out. */
  id?: string;
  /**
   * Setting values by key; each is checked against the component's settings.
   * They reach the exporter as `--video-option KEY=VALUE` text, read by the
   * setting's type as on the command line, so `'18'` sets a number setting to 18.
   */
  options?: Record<string, SettingValue>;
}

/** What gives where the loudness gain would take a true peak over `truePeak`. */
export type PeakHandling = 'limit' | 'lower';

/**
 * A composition time: seconds (rounded to milliseconds), or a timecode
 * string, `HH:MM:SS(.mmm)`, `MM:SS(.mmm)` or `SS(.mmm)`.
 */
export type Timecode = number | string;

interface RenderOptions {
  /** Replace existing output files. */
  overwrite?: boolean;
  /** `draft` skips re-rasterizing scaled text, for quick timing checks. */
  renderQuality?: RenderQuality;
  /** The graphics API; `auto` takes the first one with a GPU. */
  driver?: GpuDriver;
}

export interface VideoOptions extends SourceOptions, RunOptions, RenderOptions {
  /** The `.mp4`, `.mov` or `.webm` file. Leave it out to write only `subtitles`. */
  output?: string;
  /** Export from this time. */
  from?: Timecode;
  /** Export up to (excluding) this time. */
  to?: Timecode;
  /** `.srt` or `.vtt` files to write beside the video; `{lang}` writes one per language. */
  subtitles?: string | string[];
  /**
   * An export preset's name (`--preset`), looked up in `presetFile`, then the
   * project's `export-presets.json`, then the user's. Without it, the
   * project's default preset applies when no component is chosen.
   */
  preset?: string;
  /** An `export-presets.json` to look `preset` up in first (`--preset-file`). */
  presetFile?: string;
  /** The muxer component, `<plugin>/<id>`; the output's extension picks one when left out. */
  muxer?: string;
  /** The video encoder component and its settings. */
  videoEncoder?: EncoderOptions;
  /** The audio encoder component and its settings, or `'none'` for a silent video. */
  audioEncoder?: EncoderOptions | 'none';
  /**
   * Bring the whole mix to this integrated loudness: LUFS from -70 to -5,
   * `streaming` (-14) or `broadcast` (-23). Every video reports its mix's
   * `loudness` either way.
   */
  loudness?: number | 'streaming' | 'broadcast';
  /** The highest true peak `loudness` may leave, -9 to 0 dBTP; default -1. */
  truePeak?: number;
  /** `limit` (default) holds peaks at `truePeak`; `lower` stops the gain short of the target. */
  peakHandling?: PeakHandling;
}

export interface StillsOptions extends SourceOptions, RunOptions, RenderOptions {
  /**
   * The `.png`, `.jpg`/`.jpeg` or `.webp` file. Several frames are written as
   * `name-000090.png` beside it.
   */
  output: string;
  /** Zero-based frames to export. Give this or `every`. */
  frames?: number | readonly number[];
  /** Export every n-th frame of the composition (or `from`/`to`) and its last one. */
  every?: number;
  /** With `every`, start at this time. */
  from?: Timecode;
  /** With `every`, stop before this time. */
  to?: Timecode;
  /** JPEG and WebP quality, 1-100. */
  quality?: number;
  /** The muxer that writes the images, as `<plugin>/<component>`, instead of the built-in one. */
  muxer?: string;
  /**
   * The image encoder and its settings, instead of the built-in one:
   * `compression`, `depth` and `transparent` for `builtin/png`, `lossless`
   * and `transparent` for `builtin/webp`. Give `quality` here or above, not both.
   */
  videoEncoder?: EncoderOptions;
  /** Write the frames as one labelled grid image. */
  contactSheet?: boolean | { columns?: number; tileWidth?: number };
  /** Draw a React entry's `<DebugOverlay>` and `<DebugBounds>`. */
  debugGuides?: boolean;
}

export type CompositionOptions = SourceOptions & Omit<RunOptions, 'onProgress'>;

/** A rational frame rate. */
export interface FrameRate {
  numerator: number;
  denominator: number;
}

/** The composition an export covers. Times are seconds rounded to milliseconds. */
export interface Composition {
  width: number;
  height: number;
  fps: number;
  /** Total frames; frame numbers are zero-based. */
  frames: number;
  duration: number;
}

/** A root composition a React entry declares. */
export interface ReactComposition {
  id: string;
  width: number;
  height: number;
  frameRate: FrameRate;
  durationInFrames: number;
}

/** An exported mix's loudness, rounded to 0.01. */
export interface Loudness {
  /** Integrated loudness, LUFS; `null` for a silent mix or one shorter than 400 ms. */
  integratedLufs: number | null;
  /** The highest true peak, dBTP; `null` for digital silence. */
  truePeakDbtp: number | null;
  /** The gain a `loudness` target applied to the whole mix, dB. */
  gainDb?: number;
}

/** A written file. Frame numbers are zero-based composition frames. */
export type ExportOutput =
  | {
      type: 'video';
      path: string;
      firstFrame: number;
      frames: number;
      start: number;
      duration: number;
      /** The muxer component that wrote it. */
      muxer: string;
      video: { component: string; codec: string };
      /** The audio track, or `null` for a silent video. */
      audio: { component: string; codec: string } | null;
      /** The mix's loudness, when the video has sound. */
      loudness?: Loudness;
      alpha: boolean;
      /** Every file written for the output, absolute. */
      files: string[];
    }
  | { type: 'frame'; path: string; frame: number; time: number }
  | { type: 'subtitles'; path: string; format: 'srt' | 'vtt'; cues: number; lang?: string }
  | { type: 'contactSheet'; path: string; columns: number; frames: number[] };

/** One validation issue: a project JSON path or a property key, when there is one. */
export interface ExportIssue {
  path?: string;
  message: string;
}

/** A contact sheet frame that could not be evaluated or rendered. */
export interface FailedFrame {
  frame: number;
  /** The code the failure would have on its own, such as `react` or `render`. */
  code: string;
  message: string;
}

/** The exporter's `--json` error. */
export interface ExportErrorReport {
  /** A stable identifier to branch on. */
  code: string;
  message: string;
  hint?: string;
  issues?: ExportIssue[];
  /** The zero-based composition frame that failed, when one did. */
  frame?: number;
  /** With `frames_failed`: the contact sheet's failed tiles, in sheet order. */
  failedFrames?: FailedFrame[];
}

/** The exporter's `--json` result. */
export interface ExportReport {
  status: 'ok' | 'error';
  error?: ExportErrorReport;
  /** Absolute paths. */
  source?: string;
  propsFile?: string;
  propsFileAuto?: boolean;
  composition?: Composition;
  compositionId?: string;
  compositions?: ReactComposition[];
  outputs?: ExportOutput[];
  warnings?: string[];
  stats?: { elapsedSeconds: number; renderFps?: number };
}

/** A finished export. */
export interface ExportResult {
  source: string;
  propsFile?: string;
  propsFileAuto?: boolean;
  composition: Composition;
  /** The selected React root and every root the entry declares. */
  compositionId?: string;
  compositions?: ReactComposition[];
  outputs: ExportOutput[];
  warnings: string[];
  stats: { elapsedSeconds: number; renderFps?: number };
}

/** What `getComposition()` reports. */
export interface CompositionInfo {
  source: string;
  propsFile?: string;
  propsFileAuto?: boolean;
  composition: Composition;
  compositionId?: string;
  compositions?: ReactComposition[];
}

/** One `--json-progress` event. */
export type ProgressEvent =
  | { event: 'composition'; composition: Composition }
  | { event: 'compositions'; compositionId: string; compositions: ReactComposition[] }
  /** Frame `frame` of `total` has started, counting from 1. */
  | { event: 'rendering'; frame: number; total: number }
  | { event: 'mixingAudio' }
  | { event: 'muxing' }
  | { event: 'warning'; message: string }
  /**
   * The components the export resolved, sent before rendering starts. A
   * still export sends it too, with its image muxer and encoder and
   * `audio: null`.
   */
  | {
      event: 'output';
      muxer: string;
      muxerName: string;
      video: { component: string; name: string; settings: string[] };
      audio: { component: string; name: string; settings: string[] } | null;
    }
  | { event: 'wrote'; output: ExportOutput };

/** One installed media component, as `--list-components --json` reports it. */
export interface ComponentInfo {
  /** `<plugin>/<id>`. */
  id: string;
  kind: 'decoder' | 'videoEncoder' | 'audioEncoder' | 'muxer';
  name: string;
  plugin: string;
  /** Where the plugin comes from; `built-in` for the image components inside the exporter. */
  origin: 'user' | 'bundled' | 'project' | 'built-in';
  /** With `check`: whether the plugin could start and offers the component. */
  available?: boolean;
  /** With `check`: why it is not available. */
  reason?: string;
  /** The rest of the plugin manifest's entry for the component. */
  [manifest: string]: unknown;
}

/** One setting a component declares. */
export interface SettingField {
  key: string;
  type: 'select' | 'number' | 'boolean' | 'string';
  label: string;
  default: SettingValue;
  description?: string;
  group?: string;
  advanced?: boolean;
  /** `select`: the choices. */
  options?: { value: string; label: string }[];
  /** `number`: limits, step and unit. */
  min?: number;
  max?: number;
  step?: number;
  integer?: boolean;
  unit?: string;
  /** Shown only while every listed key holds one of its values. */
  visibleWhen?: Record<string, SettingValue[]>;
}

/** A component with what its plugin says about it (`--describe --json`). */
export interface ComponentDescription extends ComponentInfo {
  available: boolean;
  settings: SettingField[];
  /** Encoders: the pixel formats a video encoder takes. */
  inputFormats?: string[];
  transports?: string[];
  /** Audio encoders: `'any'` or the rates they take. */
  sampleRates?: 'any' | number[];
  channels?: number[];
}
