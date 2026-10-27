import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';

import { Composition, Rect, defineScenes, listChapters } from '../src/index.js';
import type { ChapterSource } from '../src/index.js';
import { mount } from '../src/render.js';

const h = React.createElement;

const mountWith = (chapters: ChapterSource | undefined, durationInFrames = 100) =>
  mount(() => h(Composition, { width: 16, height: 9, fps: 30, durationInFrames, chapters }, h(Rect, { width: 1, height: 1 })));

test('listChapters reads defineScenes lists, plans, and plain spans', () => {
  const scenes = defineScenes([
    { id: 'intro', name: 'Cold open', durationInFrames: 30 },
    { id: 'body', durationInFrames: 60, offset: -10 },
  ]);
  assert.deepEqual(listChapters(scenes), [
    { id: 'intro', name: 'Cold open', from: 0, durationInFrames: 30 },
    { id: 'body', name: 'body', from: 20, durationInFrames: 60 },
  ]);
  // A dialogue plan's `scenes` carry lines and more; only the span is kept.
  const plan = { scenes: [{ id: 'talk', from: 5, durationInFrames: 40, lines: [] }] };
  assert.deepEqual(listChapters(plan, { from: 100 }), [{ id: 'talk', name: 'talk', from: 105, durationInFrames: 40 }]);
  assert.deepEqual(listChapters([{ id: 'mark', from: 7, durationInFrames: 0 }]), [
    { id: 'mark', name: 'mark', from: 7, durationInFrames: 0 },
  ]);
});

// The editor finds a chapter's `defineScenes` entry in the source by its id.
test('chapter ids line up with defineScenes entries', () => {
  const entries = [
    { id: 'intro', durationInFrames: 30 },
    { id: 'main', name: 'Main', durationInFrames: 40, offset: -10 },
    { id: 'outro', durationInFrames: 20 },
  ];
  const scenes = defineScenes(entries);
  const mounted = mountWith(scenes, scenes.durationInFrames);
  try {
    const chapters = mounted.config.chapters ?? [];
    assert.deepEqual(
      chapters.map(({ id }) => id),
      entries.map(({ id }) => id),
    );
    for (const [i, chapter] of chapters.entries()) {
      assert.equal(chapter.from, scenes.scenes[i].from, chapter.id);
      assert.equal(chapter.durationInFrames, entries[i].durationInFrames, chapter.id);
    }
  } finally {
    mounted.dispose();
  }
});

test('listChapters rejects bad ids and frames', () => {
  assert.throws(() => listChapters([{ id: '', from: 0, durationInFrames: 1 }]), /non-empty string id/);
  assert.throws(
    () => listChapters([{ id: 'a', from: 0, durationInFrames: 1 }, { id: 'a', from: 1, durationInFrames: 1 }]),
    /used more than once/,
  );
  assert.throws(() => listChapters([{ id: 'a', from: 1.5, durationInFrames: 1 }]), /non-negative integer from/);
  assert.throws(() => listChapters([{ id: 'a', from: 5, durationInFrames: 1 }], { from: -6 }), /non-negative integer from/);
  assert.throws(() => listChapters([{ id: 'a', from: 0, durationInFrames: -1 }]), /non-negative integer durationInFrames/);
  assert.throws(() => listChapters([{ id: 'a', from: 0, durationInFrames: 1 }], { from: 0.5 }), /integer `from`/);
  assert.throws(() => listChapters({} as ChapterSource), /must be an array/);
});

test('<Composition chapters> reports them in the config, ordered and cut at the end', () => {
  const mounted = mountWith([
    { id: 'outro', name: 'Outro', from: 80, durationInFrames: 40 },
    { id: 'intro', from: 0, durationInFrames: 50 },
    { id: 'mark', from: 0, durationInFrames: 0 },
  ]);
  try {
    assert.deepEqual(mounted.config.chapters, [
      { id: 'intro', name: 'intro', from: 0, durationInFrames: 50 },
      { id: 'mark', name: 'mark', from: 0, durationInFrames: 0 },
      { id: 'outro', name: 'Outro', from: 80, durationInFrames: 20 },
    ]);
    assert.deepEqual(mounted.compositions[0].chapters, mounted.config.chapters);
  } finally {
    mounted.dispose();
  }
});

test('a composition without chapters reports none', () => {
  const mounted = mountWith(undefined);
  try {
    assert.equal('chapters' in mounted.config, false);
  } finally {
    mounted.dispose();
  }
});

test('a chapter starting at or after the end fails the mount', () => {
  assert.throws(() => mountWith([{ id: 'late', from: 100, durationInFrames: 0 }]), /at or after the composition's end/);
  assert.throws(() => mountWith([{ id: 'x', from: 0, durationInFrames: 1 }, { id: 'x', from: 2, durationInFrames: 1 }]), /used more than once/);
});
