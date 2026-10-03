import * as fs from 'node:fs';
import * as React from 'react';

import { Dialogue, Sequence } from './components';
import type { AnimatedNumber, CharacterViewProps, CharacterViewReference, DialogueProps } from './components';
import { entryRelativePath, isRemoteUrl } from './entry-dir';
import type { LipSyncTrack } from './lipsync';
import { mediaDurationInFrames, preloadMedia } from './media';

/** One line of a script, as passed to `planDialogue()`. */
export interface DialogueLine {
  /**
   * Name other direction (scene cuts, camera moves) looks the line up by.
   * Defaults to the line's index as a string. Must be unique.
   */
  id?: string;
  /** Subtitle text. */
  text: string;
  /**
   * Voice file, resolved from the entry file like `<Audio src>`. Its length
   * is the line's length unless `durationInFrames` is given.
   */
  audio?: string;
  /** Explicit length in frames; skips measuring `audio`. Required without `audio`. */
  durationInFrames?: number;
  /** Seconds of silence after the line. Defaults to the plan's `gap`. */
  gap?: number;
  /**
   * Seconds of silence before the line. Defaults to the plan's
   * `sceneLeadIn` on the first line of a new `scene` (other than the
   * script's first line), otherwise 0.
   */
  leadIn?: number;
  /** Groups consecutive lines; `plan.scene(id)` returns the group's window. */
  scene?: string;
  /** Key into `<DialogueSeries views>`. Ignored when `character` is set. */
  speaker?: string;
  /** The `<CharacterView>` ref that speaks this line. */
  character?: React.RefObject<CharacterViewReference | null>;
  expression?: string;
  mouth?: CharacterViewProps['mouth'];
  lipSync?: LipSyncTrack;
  volume?: AnimatedNumber;
  muted?: boolean;
}

export interface PlanDialogueOptions {
  /** The composition's frame rate. */
  fps: number;
  /** Default seconds of silence after each line. Defaults to 0.25. */
  gap?: number;
  /** Default seconds of silence before the first line of each new scene. Defaults to 0. */
  sceneLeadIn?: number;
}

export interface PlannedDialogueLine<L extends DialogueLine = DialogueLine> {
  id: string;
  index: number;
  /** The line as passed in. */
  line: L;
  /** Frame the line (and its voice) starts on. */
  from: number;
  /** Length of the line itself: the voice, rounded up to whole frames. */
  durationInFrames: number;
  /** Silence after the line, in frames. */
  gapInFrames: number;
  /** Silence before the line, in frames (already before `from`). */
  leadInFrames: number;
  /** `durationInFrames + gapInFrames`: frames until the next line's lead-in. */
  spanInFrames: number;
}

/** A window of the timeline; spread it onto a `<Sequence>`. */
export interface DialogueRange {
  from: number;
  durationInFrames: number;
}

export interface DialogueScene<L extends DialogueLine = DialogueLine> extends DialogueRange {
  id: string;
  lines: PlannedDialogueLine<L>[];
}

export interface DialoguePlan<L extends DialogueLine = DialogueLine> {
  fps: number;
  /** Every line in script order. */
  lines: PlannedDialogueLine<L>[];
  /** Total length, including the last line's gap. Use it as `<Composition durationInFrames>`. */
  durationInFrames: number;
  /** Consecutive runs of lines sharing a `scene`, each starting at its first line's lead-in. */
  scenes: DialogueScene<L>[];
  /** The planned line with this id; throws for an unknown id. */
  line(id: string): PlannedDialogueLine<L>;
  /** Shorthand for `line(id).from`. */
  startOf(id: string): number;
  /** From `firstId`'s start to the end of `lastId`'s gap (defaults to `firstId`). */
  range(firstId: string, lastId?: string): DialogueRange;
  /** The scene with this id; throws for an unknown id. */
  scene(id: string): DialogueScene<L>;
}

const DEFAULT_GAP_SECONDS = 0.25;

/**
 * Lays a script out back to back from its voice lengths. Call it in
 * `prepare()`: each line's `audio` is measured with `preloadMedia()`, then
 * every line starts after the previous one's gap (and its own lead-in).
 * Render the result with `<DialogueSeries plan={plan} />` and size the
 * composition with `plan.durationInFrames`.
 */
export async function planDialogue<L extends DialogueLine>(
  lines: readonly L[],
  options: PlanDialogueOptions,
): Promise<DialoguePlan<L>> {
  const { fps } = options;
  if (!Number.isFinite(fps) || fps <= 0) {
    throw new Error('planDialogue() requires a positive finite fps');
  }
  const defaultGap = seconds(options.gap ?? DEFAULT_GAP_SECONDS, 'the `gap` option');
  const sceneLeadIn = seconds(options.sceneLeadIn ?? 0, 'the `sceneLeadIn` option');

  const ids = new Set<string>();
  const labels = lines.map((line, index) => {
    const id = line.id ?? String(index);
    if (ids.has(id)) {
      throw new Error(`planDialogue(): line id "${id}" is used more than once`);
    }
    ids.add(id);
    return id;
  });
  const durations = await Promise.all(
    lines.map((line, index) => lineDuration(line, labels[index], fps)),
  );

  let at = 0;
  const planned = lines.map((line, index): PlannedDialogueLine<L> => {
    const label = `line "${labels[index]}"`;
    const startsScene = index > 0 && line.scene !== lines[index - 1].scene;
    const leadIn = line.leadIn === undefined ? (startsScene ? sceneLeadIn : 0) : seconds(line.leadIn, `${label}'s leadIn`);
    const gap = line.gap === undefined ? defaultGap : seconds(line.gap, `${label}'s gap`);
    const leadInFrames = Math.round(leadIn * fps);
    const gapInFrames = Math.round(gap * fps);
    const durationInFrames = durations[index];
    at += leadInFrames;
    const from = at;
    at += durationInFrames + gapInFrames;
    return {
      id: labels[index],
      index,
      line,
      from,
      durationInFrames,
      gapInFrames,
      leadInFrames,
      spanInFrames: durationInFrames + gapInFrames,
    };
  });
  const total = at;

  const scenes: DialogueScene<L>[] = [];
  for (const entry of planned) {
    const sceneId = entry.line.scene;
    const last = scenes[scenes.length - 1];
    if (sceneId !== undefined && last?.id === sceneId && last.lines[last.lines.length - 1].index === entry.index - 1) {
      last.lines.push(entry);
      continue;
    }
    if (sceneId === undefined) {
      continue;
    }
    if (scenes.some((scene) => scene.id === sceneId)) {
      throw new Error(`planDialogue(): scene "${sceneId}" is split by other lines; keep its lines together`);
    }
    scenes.push({ id: sceneId, from: entry.from - entry.leadInFrames, durationInFrames: 0, lines: [entry] });
  }
  scenes.forEach((scene, index) => {
    // A scene lasts until the next one begins, so cuts leave no hole; the
    // last scene runs to the end of the plan.
    const next = scenes[index + 1];
    scene.durationInFrames = (next ? next.from : total) - scene.from;
  });

  const byId = new Map(planned.map((entry) => [entry.id, entry]));
  const line = (id: string): PlannedDialogueLine<L> => {
    const found = byId.get(id);
    if (!found) {
      throw new Error(`dialogue plan has no line "${id}"`);
    }
    return found;
  };
  return {
    fps,
    lines: planned,
    durationInFrames: total,
    scenes,
    line,
    startOf: (id) => line(id).from,
    range(firstId, lastId = firstId) {
      const first = line(firstId);
      const last = line(lastId);
      if (last.index < first.index) {
        throw new Error(`dialogue plan range: line "${lastId}" comes before "${firstId}"`);
      }
      return { from: first.from, durationInFrames: last.from + last.spanInFrames - first.from };
    },
    scene(id) {
      const found = scenes.find((scene) => scene.id === id);
      if (!found) {
        throw new Error(`dialogue plan has no scene "${id}"`);
      }
      return found;
    },
  };
}

function seconds(value: number, what: string): number {
  if (!Number.isFinite(value) || value < 0) {
    throw new Error(`planDialogue(): ${what} must be a finite number of seconds >= 0`);
  }
  return value;
}

async function lineDuration(line: DialogueLine, id: string, fps: number): Promise<number> {
  const label = `line "${id}"`;
  if (line.durationInFrames !== undefined) {
    if (!Number.isInteger(line.durationInFrames) || line.durationInFrames <= 0) {
      throw new Error(`planDialogue(): ${label} requires a positive integer durationInFrames`);
    }
    return line.durationInFrames;
  }
  if (line.audio === undefined) {
    throw new Error(`planDialogue(): ${label} needs \`audio\` or \`durationInFrames\``);
  }
  const src = line.audio;
  if (!isRemoteUrl(src)) {
    const path = entryRelativePath(src);
    if (!fs.existsSync(path)) {
      throw new Error(`planDialogue(): voice file for ${label} not found: ${src} (looked at ${path})`);
    }
  }
  let frames: number | undefined;
  try {
    frames = mediaDurationInFrames(await preloadMedia(src), fps);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`planDialogue(): could not measure the voice of ${label} (${src}): ${message}`);
  }
  if (frames === undefined || frames <= 0) {
    throw new Error(
      `planDialogue(): the voice of ${label} (${src}) has no known length; set durationInFrames on the line`,
    );
  }
  return frames;
}

export interface DialogueSeriesProps<L extends DialogueLine = DialogueLine> {
  /** The result of `planDialogue()`. */
  plan: DialoguePlan<L>;
  /** `<CharacterView>` refs by `speaker`, for lines without their own `character`. */
  views?: Record<string, React.RefObject<CharacterViewReference | null>>;
  /**
   * Keeps each subtitle (and its expression) on screen through the gap after
   * it instead of clearing it when the voice ends. Defaults to false.
   */
  holdThroughGap?: boolean;
  /** Extra `<Dialogue>` props per line, e.g. to move one subtitle; merged over the line's own. */
  dialogueProps?: (line: PlannedDialogueLine<L>) => Partial<Omit<DialogueProps, 'children'>> | undefined;
}

/**
 * Renders a planned script: one `<Sequence>` + `<Dialogue>` per line, each
 * starting at its planned frame with the line's voice, subtitle, and
 * expression applied to its speaker's view.
 *
 * ```tsx
 * <DialogueSeries plan={plan} views={{ mira: miraView, hana: hanaView }} />
 * ```
 */
export function DialogueSeries<L extends DialogueLine>({
  plan,
  views,
  holdThroughGap = false,
  dialogueProps,
}: DialogueSeriesProps<L>): ReturnType<typeof React.createElement> {
  return React.createElement(
    React.Fragment,
    null,
    plan.lines.map((entry) => {
      const { line } = entry;
      const character = line.character ?? (line.speaker === undefined ? undefined : views?.[line.speaker]);
      if (!character) {
        throw new Error(
          line.speaker === undefined
            ? `<DialogueSeries> line "${entry.id}" needs a \`character\` view ref or a \`speaker\``
            : `<DialogueSeries> line "${entry.id}": speaker "${line.speaker}" is not in \`views\``,
        );
      }
      const props: DialogueProps = {
        character,
        audio: line.audio,
        expression: line.expression,
        mouth: line.mouth,
        lipSync: line.lipSync,
        volume: line.volume,
        muted: line.muted,
        ...dialogueProps?.(entry),
        children: line.text,
      };
      return React.createElement(
        Sequence,
        {
          key: entry.id,
          from: entry.from,
          durationInFrames: holdThroughGap ? entry.spanInFrames : entry.durationInFrames,
        },
        React.createElement(Dialogue, props),
      );
    }),
  );
}
