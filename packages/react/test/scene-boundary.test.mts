import assert from 'node:assert/strict';
import { afterEach, test, vi } from 'vitest';
import * as React from 'react';

import {
  Audio,
  Composition,
  SceneBoundary,
  SceneRenderError,
  Scenes,
  Sequence,
  Series,
  Text,
  defineScenes,
  useCurrentFrame,
} from '../src/index.js';
import type { Layer } from '../src/index.js';
import { mount } from '../src/render.js';
import type { MountedComposition } from '../src/render.js';

const h = React.createElement;
const time = (frame: number) => ({ value: frame, timescale: 30 });
const preview = (composition: MountedComposition, frame: number) =>
  composition.renderAt(time(frame), null, { preview: true });
const exported = (composition: MountedComposition, frame: number) => composition.renderAt(time(frame), null);
const texts = (layers: Layer[]): string[] => layers.flatMap((layer) =>
  layer.content.type === 'text' ? [layer.content.text] : texts(layer.content.type === 'group' ? layer.content.layers : []),
);

afterEach(() => {
  vi.restoreAllMocks();
});

function Label({ text }: { text: string }) {
  return h(Text, null, `${text}:${useCurrentFrame()}`);
}

/** Throws from its 10th frame on. */
function Breaks() {
  const frame = useCurrentFrame();
  if (frame >= 10) throw new TypeError(`no data for frame ${frame}`);
  return h(Text, null, `stats:${frame}`);
}

const SCENES = defineScenes([
  { id: 'intro', name: 'Intro', durationInFrames: 20, Scene: () => h(Label, { text: 'intro' }) },
  { id: 'stats', name: 'Stats', durationInFrames: 20, Scene: Breaks },
  { id: 'outro', durationInFrames: 20, Scene: () => h(Label, { text: 'outro' }) },
]);

const film = () => mount(() => h(Composition, { width: 1280, height: 720, fps: 30, durationInFrames: 60 }, h(Scenes, { scenes: SCENES })));

test('in the preview a scene that throws draws its error card and the others still play', () => {
  const composition = film();
  assert.deepEqual(texts(preview(composition, 5).scene.layers), ['intro:5']);
  assert.equal(preview(composition, 5).sceneErrors, undefined);
  assert.deepEqual(texts(preview(composition, 25).scene.layers), ['stats:5']);

  const failed = preview(composition, 32);
  const card = texts(failed.scene.layers);
  assert.equal(card[0], 'Scene failed to render');
  assert.ok(card.includes('Stats'), card.join(' | '));
  assert.ok(card.includes('id: stats'), card.join(' | '));
  assert.ok(card.includes('no data for frame 12'), card.join(' | '));
  assert.equal(failed.sceneErrors?.length, 1);
  const [report] = failed.sceneErrors!;
  assert.equal(report.id, 'stats');
  assert.equal(report.name, 'Stats');
  assert.equal(report.message, 'no data for frame 12');
  assert.match(report.stack ?? '', /^TypeError: no data for frame 12\n\s+at Breaks /);
  assert.doesNotMatch(report.stack ?? '', /react-reconciler/);
  assert.match(report.componentStack ?? '', /^\s+at Breaks\b/);
  assert.doesNotMatch(report.componentStack ?? '', /\bBoundary\b|\bScenes\b/);

  assert.deepEqual(texts(preview(composition, 45).scene.layers), ['outro:5']);
  composition.dispose();
});

test('the card fills the composition', () => {
  const composition = film();
  const [sequence] = preview(composition, 30).scene.layers;
  assert.equal(sequence.content.type, 'group');
  const [card] = sequence.content.type === 'group' ? sequence.content.layers : [];
  assert.equal(card.content.type, 'group');
  const [background] = card.content.type === 'group' ? card.content.layers : [];
  assert.equal(background.content.type, 'rect');
  if (background.content.type === 'rect') {
    assert.equal(background.content.width, 1280);
    assert.equal(background.content.height, 720);
  }
  composition.dispose();
});

test('a caught error is never carried into another frame, in any order', () => {
  const composition = film();
  for (const frame of [35, 29, 30, 21, 39, 20, 31, 5, 32]) {
    const local = frame - 20;
    const { scene, sceneErrors } = preview(composition, frame);
    if (frame >= 20 && local < 10) {
      assert.deepEqual(texts(scene.layers), [`stats:${local}`], `frame ${frame}`);
      assert.equal(sceneErrors, undefined, `frame ${frame}`);
    } else if (frame >= 20) {
      assert.equal(texts(scene.layers)[0], 'Scene failed to render', `frame ${frame}`);
      assert.equal(sceneErrors?.[0].message, `no data for frame ${local}`, `frame ${frame}`);
    } else {
      assert.deepEqual(texts(scene.layers), [`intro:${frame}`]);
    }
  }
  composition.dispose();
});

test('outside the preview the frame fails, naming the scene, and later frames still render', () => {
  const composition = film();
  assert.throws(() => exported(composition, 30), (error: unknown) => {
    assert.ok(error instanceof SceneRenderError);
    assert.equal(error.message, 'scene "stats" (Stats) threw while rendering: no data for frame 10');
    assert.equal(error.sceneId, 'stats');
    assert.ok(error.cause instanceof TypeError);
    assert.match(error.stack ?? '', /at Breaks /);
    return true;
  });
  assert.deepEqual(texts(exported(composition, 25).scene.layers), ['stats:5']);
  assert.deepEqual(texts(exported(composition, 45).scene.layers), ['outro:5']);
  // A scene whose name is its id is named once.
  assert.equal(new SceneRenderError('outro', 'outro', new Error('x')).message, 'scene "outro" threw while rendering: x');
  composition.dispose();
});

test('the preview logs nothing for a caught scene error', () => {
  const logged = vi.spyOn(console, 'error').mockImplementation(() => {});
  const composition = film();
  preview(composition, 30);
  preview(composition, 31);
  assert.equal(logged.mock.calls.length, 0);
  composition.dispose();
});

test('an error outside every scene still fails a preview frame', () => {
  function Hud(): React.ReactElement {
    if (useCurrentFrame() === 3) throw new Error('hud broke');
    return h(Text, null, 'hud');
  }
  const composition = mount(() => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 60 },
    h(Scenes, { scenes: SCENES }), h(Hud)));
  assert.throws(() => preview(composition, 3), /^Error: hud broke$/);
  assert.deepEqual(texts(preview(composition, 4).scene.layers), ['intro:4', 'hud']);
  composition.dispose();
});

test('Series items and a SceneBoundary of your own are isolated too', () => {
  function Throws({ message }: { message: string }): React.ReactElement {
    throw new Error(message);
  }
  const composition = mount(() => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 30 },
    h(Series, null,
      h(Series.Sequence, { durationInFrames: 10 }, h(Throws, { message: 'first' })),
      h(Series.Sequence, { durationInFrames: 10, id: 'second', name: 'Second' }, h(Throws, { message: 'second' })),
      h(Series.Sequence, { durationInFrames: 10 }, h(Label, { text: 'third' }))),
    h(Sequence, { from: 0, durationInFrames: 30 },
      h(SceneBoundary, { id: 'badge' }, h(Throws, { message: 'badge' })))));
  const first = preview(composition, 0);
  assert.deepEqual(first.sceneErrors?.map(({ id, name, message }) => ({ id, name, message })), [
    { id: 'series-1', name: 'Series item 1', message: 'first' },
    { id: 'badge', name: 'badge', message: 'badge' },
  ]);
  assert.deepEqual(preview(composition, 10).sceneErrors?.map(({ id, name }) => [id, name]), [['second', 'Second'], ['badge', 'badge']]);
  assert.deepEqual(preview(composition, 20).sceneErrors?.map(({ id }) => id), ['badge']);
  assert.ok(texts(preview(composition, 20).scene.layers).includes('third:0'));
  assert.throws(() => exported(composition, 20), /^SceneRenderError: scene "badge" threw while rendering: badge$/);
  composition.dispose();
});

test('a scene list inside a scene reports the innermost scene that threw', () => {
  const inner = defineScenes([{ id: 'inner', durationInFrames: 20, Scene: Breaks }]);
  const outer = defineScenes([{ id: 'outer', durationInFrames: 30, Scene: () => h(Scenes, { scenes: inner }) }]);
  const composition = mount(() => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 30 },
    h(Scenes, { scenes: outer })));
  assert.deepEqual(texts(preview(composition, 9).scene.layers), ['stats:9']);
  assert.deepEqual(preview(composition, 12).sceneErrors?.map(({ id }) => id), ['inner']);
  assert.throws(() => exported(composition, 12), /^SceneRenderError: scene "inner" threw while rendering: no data for frame 12$/);
  composition.dispose();
});

test('the preview audio sweep skips a failing scene instead of failing', () => {
  function Loud(): React.ReactElement {
    const frame = useCurrentFrame();
    if (frame >= 2) throw new Error('loud broke');
    return h(Audio, { src: 'loud.wav' });
  }
  const scenes = defineScenes([
    { id: 'loud', durationInFrames: 4, Scene: Loud },
    { id: 'quiet', durationInFrames: 4, Scene: () => h(Audio, { src: 'quiet.wav' }) },
  ]);
  const composition = mount(() => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 8 }, h(Scenes, { scenes })));
  const sources = composition.collectAudio({ preview: true }).map(({ src }) => src);
  assert.deepEqual(sources, ['loud.wav', 'loud.wav', 'quiet.wav', 'quiet.wav', 'quiet.wav', 'quiet.wav']);
  assert.throws(() => composition.collectAudio(), /scene "loud" threw while rendering: loud broke/);
  composition.dispose();
});

test('SceneBoundary requires an id', () => {
  const composition = () => mount(() => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 1 },
    h(SceneBoundary, { id: '' }, null)));
  assert.throws(composition, /requires a non-empty string `id`/);
});

test('two scenes with the same default id are both reported', () => {
  function Throws(): React.ReactElement {
    throw new Error('broken');
  }
  const composition = mount(() => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 10 },
    h(Series, null, h(Series.Sequence, { durationInFrames: 10 }, h(Throws))),
    h(Series, null, h(Series.Sequence, { durationInFrames: 10 }, h(Throws)))));
  assert.deepEqual(preview(composition, 0).sceneErrors?.map(({ id }) => id), ['series-1', 'series-1']);
  composition.dispose();
});
