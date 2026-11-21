// Chapters: named spans of a composition's timeline that the editor preview
// lists and seeks to. They are declared on `<Composition chapters>` and read
// once at mount, like its size and clock; nothing is inferred from the tree.

/** One span to list, as `defineScenes`, a dialogue plan, or code places it. */
export interface ChapterInput {
  /** Unique key of the chapter within the composition. */
  id: string;
  /** Label to show. Defaults to `id`. */
  name?: string;
  /** First frame, in the frames of the timeline the span is placed on. */
  from: number;
  /** Length in frames. 0 marks a point: listed and seekable, never current. */
  durationInFrames: number;
  /** One sentence on what happens, listed beside the name. */
  desc?: string;
  /** `'rough'` marks a span that is not built yet; see `SceneEntry.status`. */
  status?: 'rough' | 'final';
}

/**
 * What `<Composition chapters>` and `listChapters` take: spans, or anything
 * holding them in `scenes`, such as a `defineScenes` list or the plan
 * `planDialogue` returns.
 */
export type ChapterSource = readonly ChapterInput[] | { readonly scenes: readonly ChapterInput[] };

/** A checked chapter, as the composition reports it to the editor. */
export interface Chapter {
  id: string;
  name: string;
  from: number;
  durationInFrames: number;
  /** Present when the source gave one. */
  desc?: string;
  /** Present when the source gave one; absent means final. */
  status?: 'rough' | 'final';
}

export interface ListChaptersOptions {
  /**
   * Frames to move every span by: where the timeline the source is placed on
   * starts in the composition, such as a `<Sequence from>` the source plays
   * in. Defaults to 0.
   */
  from?: number;
}

/**
 * The spans of `source` as chapters of the composition, moved by
 * `options.from`, in the order given. Concatenate the results to list
 * chapters from several sources, or the sections of a list played inside one
 * scene of another:
 *
 * ```tsx
 * const chapters = [
 *   ...listChapters(scenes),
 *   ...listChapters(plan, { from: scenes.scenes[1].from }),
 * ];
 * <Composition chapters={chapters} ...>
 * ```
 *
 * A chapter keeps its source's `desc` and `status`, so the editor marks rough
 * scenes and an export warns about them.
 *
 * Throws for a missing or empty `id`, a negative or fractional frame, an
 * `id` used twice, a `desc` that is not a string, and a `status` other than
 * `'rough'` or `'final'`.
 */
export function listChapters(source: ChapterSource, options?: ListChaptersOptions): Chapter[] {
  const shift = options?.from ?? 0;
  if (!Number.isInteger(shift)) {
    throw new Error('listChapters() requires an integer `from` option');
  }
  const entries = isChapterArray(source) ? source : source?.scenes;
  if (!Array.isArray(entries)) {
    throw new Error('chapters must be an array of { id, from, durationInFrames } or an object with such `scenes`');
  }
  const ids = new Set<string>();
  return entries.map((entry: ChapterInput, index): Chapter => {
    const { id, name, from, durationInFrames, desc, status } = entry ?? ({} as ChapterInput);
    if (typeof id !== 'string' || id === '') {
      throw new Error(`chapter ${index} requires a non-empty string id`);
    }
    if (ids.has(id)) {
      throw new Error(`chapter id "${id}" is used more than once`);
    }
    ids.add(id);
    if (name !== undefined && typeof name !== 'string') {
      throw new Error(`chapter "${id}" requires a string name`);
    }
    if (!Number.isInteger(from) || from + shift < 0) {
      throw new Error(`chapter "${id}" requires a non-negative integer from`);
    }
    if (!Number.isInteger(durationInFrames) || durationInFrames < 0) {
      throw new Error(`chapter "${id}" requires a non-negative integer durationInFrames`);
    }
    if (desc !== undefined && typeof desc !== 'string') {
      throw new Error(`chapter "${id}" requires a string desc`);
    }
    if (status !== undefined && status !== 'rough' && status !== 'final') {
      throw new Error(`chapter "${id}" requires status to be 'rough' or 'final'`);
    }
    return {
      id,
      name: name ?? id,
      from: from + shift,
      durationInFrames,
      ...(desc !== undefined ? { desc } : {}),
      ...(status !== undefined ? { status } : {}),
    };
  });
}

function isChapterArray(source: ChapterSource): source is readonly ChapterInput[] {
  return Array.isArray(source);
}

/**
 * The chapters `<Composition chapters>` reports for a composition of
 * `durationInFrames` frames: checked as `listChapters` checks them, ordered by
 * their first frame (declaration order among equal ones), and cut at the
 * composition's end. A chapter starting at or after the end throws.
 */
export function compositionChapters(source: ChapterSource, durationInFrames: number): Chapter[] {
  return listChapters(source)
    .map((chapter) => {
      if (chapter.from >= durationInFrames) {
        throw new Error(
          `chapter "${chapter.id}" starts at frame ${chapter.from}, at or after the composition's end (${durationInFrames})`,
        );
      }
      return { ...chapter, durationInFrames: Math.min(chapter.durationInFrames, durationInFrames - chapter.from) };
    })
    .sort((a, b) => a.from - b.from);
}
