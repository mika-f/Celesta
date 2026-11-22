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
import * as browser from '../src/browser.js';
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

test('the preview audio sweep silences a failing scene on its broken frames only', () => {
  function Loud(): React.ReactElement {
    const frame = useCurrentFrame();
    if (frame >= 2 && frame < 4) throw new Error('loud broke');
    return h(Audio, { src: 'loud.wav', volume: { type: 'keyframes', keyframes: [
      { time: { value: 0, timescale: 30 }, value: 0 },
      { time: { value: 6, timescale: 30 }, value: 1 },
    ] }, fadeIn: 0.01, fadeOut: 0.02 });
  }
  const scenes = defineScenes([
    { id: 'loud', durationInFrames: 6, Scene: Loud },
    { id: 'quiet', durationInFrames: 4, Scene: () => h(Audio, { src: 'quiet.wav' }) },
  ]);
  // A sound beside the scenes plays throughout, broken frames included.
  const composition = mount(() => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 10 },
    h(Scenes, { scenes }), h(Audio, { src: 'bed.wav' })));
  const clips = composition.collectAudio({ preview: true });
  const loud = clips.filter(({ src }) => src === 'loud.wav');
  const close = (actual: number, expected: number) => assert.ok(Math.abs(actual - expected) < 1e-9, `${actual} ≉ ${expected}`);
  // Frames 0–1 and 4–5 of the scene are heard, each once; 2–3 are cut.
  assert.equal(loud.length, 2);
  close(loud[0].start, 0);
  close(loud[0].duration, 2 / 30);
  assert.equal(loud[0].fadeIn, 0.01);
  assert.equal(loud[0].fadeOut, undefined);
  close(loud[1].start, 4 / 30);
  close(loud[1].duration, 2 / 30);
  close(loud[1].sourceStart, 4 / 30);
  assert.equal(loud[1].fadeIn, undefined);
  assert.equal(loud[1].fadeOut, 0.02);
  // The volume curve keeps its place in the composition.
  const volume = loud[1].volume;
  assert.ok(typeof volume !== 'number');
  assert.ok(Math.abs(volume.keyframes[0].time.value / volume.keyframes[0].time.timescale + 4 / 30) < 1e-6);
  // The other scene and the sound beside them are reported as before.
  assert.equal(clips.filter(({ src }) => src === 'quiet.wav').length, 4);
  const bed = clips.filter(({ src }) => src === 'bed.wav');
  assert.equal(bed.length, 10);
  close(bed[0].duration, 10 / 30);
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

test('the card cuts a long message between code points', () => {
  const message = `${'x'.repeat(398)}🎬!!`;
  function Throws(): React.ReactElement {
    throw new Error(message);
  }
  const composition = mount(() => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 1 },
    h(Series, null, h(Series.Sequence, { durationInFrames: 1 }, h(Throws)))));
  const { scene, sceneErrors } = preview(composition, 0);
  const shown = texts(scene.layers).find((text) => text.startsWith('xxx'));
  assert.equal(shown, `${'x'.repeat(398)}🎬…`);
  assert.doesNotMatch(JSON.stringify(scene), /\\ud83c/i);
  assert.equal(sceneErrors?.[0].message, message);
  composition.dispose();
});

test('an empty series id is no id', () => {
  function Throws(): React.ReactElement {
    throw new Error('second');
  }
  const composition = mount(() => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 2 },
    h(Series, null,
      h(Series.Sequence, { durationInFrames: 1, id: '' }, h(Label, { text: 'ok' })),
      h(Series.Sequence, { durationInFrames: 1, id: '', name: '' }, h(Throws)))));
  assert.deepEqual(texts(exported(composition, 0).scene.layers), ['ok:0']);
  assert.deepEqual(preview(composition, 1).sceneErrors?.map(({ id, name }) => [id, name]), [['series-2', 'Series item 2']]);
  composition.dispose();
});

const keyframesAt = (pairs: [number, number, number][]) => ({
  type: 'keyframes' as const,
  keyframes: pairs.map(([value, timescale, at]) => ({ time: { value, timescale }, value: at })),
});

test('a cut clip resumes its source where an animated rate has got to, and moves curves exactly', () => {
  function Ramp(): React.ReactElement {
    const frame = useCurrentFrame();
    // Broken from 0.4 s to 2 s into the scene.
    if (frame >= 12 && frame < 60) throw new Error('ramp broke');
    return h(Audio, {
      src: 'ramp.wav',
      // 1x at 0 s rising to 2x at 2 s: 3 s of source by 2 s.
      playbackRate: keyframesAt([[0, 1, 1], [2, 1, 2]]),
      // Full at 1 s, in whole seconds.
      volume: keyframesAt([[0, 1, 0], [1, 1, 1]]),
    });
  }
  const scenes = defineScenes([{ id: 'ramp', durationInFrames: 90, Scene: Ramp }]);
  const composition = mount(() => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 90 }, h(Scenes, { scenes })));
  const [first, second] = composition.collectAudio({ preview: true });
  const close = (actual: number, expected: number, what: string) => assert.ok(Math.abs(actual - expected) < 1e-6, `${what}: ${actual} ≉ ${expected}`);
  close(first.sourceStart, 0, 'first source');
  close(first.duration, 0.4, 'first length');
  close(second.start, 2, 'second start');
  close(second.sourceStart, 3, 'second source');
  const volume = second.volume;
  assert.ok(typeof volume !== 'number');
  // 1 s on the composition is 1 s before the piece.
  assert.deepEqual(volume.keyframes.map(({ time }) => time.value / time.timescale), [-2, -1]);
  composition.dispose();
});

test('a sound declared the same in a scene that played is not cut', () => {
  function Breaks2(): React.ReactElement | null {
    const frame = useCurrentFrame();
    if (frame >= 2 && frame < 4) throw new Error('broke');
    return null;
  }
  const bed = () => h(Audio, { src: 'bed.wav' });
  const composition = mount(() => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 6 },
    h(Sequence, { durationInFrames: 6 },
      h(SceneBoundary, { id: 'broken' }, bed(), h(Breaks2)),
      h(SceneBoundary, { id: 'fine' }, bed()))));
  const clips = composition.collectAudio({ preview: true });
  assert.ok(clips.length > 0);
  for (const clip of clips) {
    assert.equal(clip.start, 0);
    assert.ok(Math.abs(clip.duration - 6 / 30) < 1e-9);
  }
  composition.dispose();
});

test('a cut between frames moves whole-second keyframes by exactly the cut', () => {
  function Fade(): React.ReactElement {
    const frame = useCurrentFrame();
    // Broken from 0.2 s to 0.4 s; heard again from 0.4 s.
    if (frame >= 6 && frame < 12) throw new Error('fade broke');
    return h(Audio, { src: 'fade.wav', volume: keyframesAt([[0, 1, 0], [1, 1, 1]]) });
  }
  const scenes = defineScenes([{ id: 'fade', durationInFrames: 60, Scene: Fade }]);
  const composition = mount(() => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 60 }, h(Scenes, { scenes })));
  const [, resumed] = composition.collectAudio({ preview: true });
  assert.ok(Math.abs(resumed.start - 0.4) < 1e-9);
  const volume = resumed.volume;
  assert.ok(typeof volume !== 'number');
  // Full at 1 s on the clip: 0.6 s into the piece that starts at 0.4 s.
  assert.deepEqual(volume.keyframes.map(({ time }) => time.value / time.timescale), [-0.4, 0.6]);
  composition.dispose();
});

test('a cut clip resumes past a rate that steps at one time', () => {
  function Step(): React.ReactElement {
    const frame = useCurrentFrame();
    if (frame >= 30 && frame < 60) throw new Error('step broke');
    return h(Audio, {
      src: 'step.wav',
      // 1x until 1 s, then 2x at once (two keys at 1 s): 1 + 2 = 3 s of source by 2 s.
      playbackRate: keyframesAt([[0, 1, 1], [1, 1, 1], [1, 1, 2]]),
    });
  }
  const scenes = defineScenes([{ id: 'step', durationInFrames: 90, Scene: Step }]);
  const composition = mount(() => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 90 }, h(Scenes, { scenes })));
  const [, resumed] = composition.collectAudio({ preview: true });
  assert.ok(Math.abs(resumed.start - 2) < 1e-9);
  assert.ok(Math.abs(resumed.sourceStart - 3) < 1e-9, `${resumed.sourceStart}`);
  composition.dispose();
});

test('the browser entry exports <SceneBoundary> and SceneRenderError too', () => {
  assert.equal(browser.SceneBoundary, SceneBoundary);
  assert.equal(browser.SceneRenderError, SceneRenderError);
});
