import assert from 'node:assert/strict';
import { beforeEach, test } from 'vitest';
import * as React from 'react';
import type { ReactElement } from 'react';

import { Composition, Sequence, Video } from '@celesta.studio/react';
import type { Layer, LayerContent } from '@celesta.studio/react';
import { mount } from '@celesta.studio/react/internal';
import type { MountedComposition } from '@celesta.studio/react/internal';
import { MediaFrame } from '../src/index.js';
import type { MediaFrameProps, MediaFrameVideoProps } from '../src/index.js';
import { installStubMeasurer } from './text-metrics-stub.mjs';

const h = React.createElement;

beforeEach(() => {
  installStubMeasurer();
});

function composition(element: ReactElement, durationInFrames = 300) {
  return mount(() => h(Composition, { width: 1920, height: 1080, fps: 30, durationInFrames }, element));
}

function render(element: ReactElement) {
  const mounted = composition(element);
  return (frame = 0): Layer[] => mounted.renderAt({ value: frame, timescale: 30 }, null).scene.layers;
}

/** A layer whose content is of type `T`. */
type LayerOf<T extends LayerContent['type']> = Layer & { content: Extract<LayerContent, { type: T }> };

/** `layer`, checked to have content of `type`. */
function layerOf<T extends LayerContent['type']>(layer: Layer | undefined, type: T): LayerOf<T> {
  assert.ok(layer, `a ${type} layer`);
  assert.equal(layer.content.type, type);
  return layer as LayerOf<T>;
}

const is = <T extends LayerContent['type']>(type: T) => (layer: Layer): layer is LayerOf<T> => layer.content.type === type;

/** The background, media clip group, media layer, caption, and border of a rendered frame. */
function parts(frame: Layer | undefined) {
  const layers = layerOf(frame, 'group').content.layers;
  const rects = layers.filter(is('rect'));
  const content = layerOf(layers.find(is('group')), 'group');
  const [clip, caption] = content.content.layers;
  const clipGroup = layerOf(clip, 'group');
  return {
    background: rects.find((rect) => rect.content.fill), border: rects.find((rect) => rect.content.stroke),
    content, clip: clipGroup, media: clipGroup.content.layers[0], caption: caption && layerOf(caption, 'text'),
  };
}

const video = { kind: 'video', src: './clip.mp4', sourceWidth: 1920, sourceHeight: 1080 } as const satisfies Partial<MediaFrameVideoProps>;

test('an image gets the area inside the padding and the fit, and is clipped there with rounded corners', () => {
  const [frame] = render(h(MediaFrame, {
    kind: 'image', src: './shot.png', x: 100, y: 50, width: 960, height: 540, padding: 10, fit: 'cover', cornerRadius: 24, fill: '#000000',
  }))();
  const { background, content, clip, media } = parts(frame);
  assert.ok(background);
  assert.deepEqual(frame.transform.position, { x: 100, y: 50 });
  assert.deepEqual([background.content.width, background.content.height, background.content.cornerRadius], [960, 540, 24]);
  assert.deepEqual(content.transform.position, { x: 10, y: 10 });
  assert.deepEqual(clip.content.clip, { x: 0, y: 0, width: 940, height: 520, cornerRadius: 14 });
  assert.equal(media.content.type, 'image');
  const image = layerOf(media, 'image').content;
  assert.deepEqual([image.width, image.height, image.fit], [940, 520, 'cover']);
});

test('fit defaults to contain, and an animated image keeps its playback props', () => {
  const [frame] = render(h(MediaFrame, { kind: 'image', src: './a.gif', width: 400, height: 300, startFrom: 0.5, playbackRate: 2, loop: false }))(0);
  const image = layerOf(parts(frame).media, 'image').content;
  assert.equal(image.fit, 'contain');
  assert.deepEqual(image.animation, { sourceTimeSeconds: 0.5, loop: false });
});

test('a landscape video contained in a portrait frame is centred and letterboxed', () => {
  const [frame] = render(h(MediaFrame, { ...video, width: 540, height: 960, fit: 'contain' }))();
  const { media } = parts(frame);
  // 1920×1080 at 540 / 1920: 540 × 303.75, centred down the 960.
  assert.equal(media.transform.scale.x, 0.28125);
  assert.deepEqual(media.transform.position, { x: 0, y: 328.125 });
});

test('a landscape video covering a portrait frame overflows sideways and is clipped to the frame', () => {
  const [frame] = render(h(MediaFrame, { ...video, width: 540, height: 960, fit: 'cover', cornerRadius: 30 }))();
  const { clip, media } = parts(frame);
  // At 960 / 1080 it is 1706.67 wide, centred on the 540.
  assert.equal(media.transform.scale.x, 960 / 1080);
  assert.ok(Math.abs(media.transform.position.x - (540 - 1920 * (960 / 1080)) / 2) < 1e-9);
  assert.equal(media.transform.position.y, 0);
  assert.deepEqual(clip.content.clip, { x: 0, y: 0, width: 540, height: 960, cornerRadius: 30 });
});

test('a portrait video in a landscape frame: contain pillarboxes, cover crops top and bottom', () => {
  const tall = { ...video, sourceWidth: 1080, sourceHeight: 1920, width: 960, height: 540 };
  const contain = parts(render(h(MediaFrame, { ...tall, fit: 'contain' }))()[0]).media;
  assert.equal(contain.transform.scale.x, 540 / 1920);
  assert.deepEqual(contain.transform.position, { x: (960 - 1080 * (540 / 1920)) / 2, y: 0 });
  const cover = parts(render(h(MediaFrame, { ...tall, fit: 'cover' }))()[0]).media;
  assert.equal(cover.transform.scale.x, 960 / 1080);
  assert.deepEqual(cover.transform.position, { x: 0, y: (540 - 1920 * (960 / 1080)) / 2 });
});

test('the media corner radius is the frame\'s less the narrowest padding edge, never below 0', () => {
  const radius = (props: Partial<MediaFrameVideoProps>) =>
    parts(render(h(MediaFrame, { ...video, width: 400, height: 300, ...props }))()[0]).clip.content.clip?.cornerRadius;
  assert.equal(radius({ cornerRadius: 24, padding: { top: 8, right: 12, bottom: 8, left: 12 } }), 16);
  assert.equal(radius({ cornerRadius: 6, padding: 10 }), 0);
  assert.equal(radius({}), 0);
});

test('the border draws over the media', () => {
  const [frame] = render(h(MediaFrame, { ...video, width: 400, height: 300, fill: '#000000', stroke: '#FFFFFF', strokeWidth: 4 }))();
  const layers = layerOf(frame, 'group').content.layers;
  assert.deepEqual(layers.map((layer) => layer.content.type), ['rect', 'group', 'rect']);
  assert.ok(parts(frame).border?.content.stroke);
});

test('a video plays and sounds exactly as a bare Video does, inside a Sequence', () => {
  const playback = { src: './clip.mp4', startFrom: 1.5, playbackRate: 2, volume: 0.6, muted: false, loop: true };
  const framed = composition(h(Sequence, { from: 10, durationInFrames: 60 },
    h(MediaFrame, { kind: 'video', ...playback, sourceWidth: 1280, sourceHeight: 720, width: 640, height: 360, fit: 'cover' })), 90);
  const bare = composition(h(Sequence, { from: 10, durationInFrames: 60 }, h(Video, playback)), 90);
  const videoOf = (mounted: MountedComposition, frame: number): LayerOf<'video'> | undefined => {
    const find = (layers: Layer[]): LayerOf<'video'>[] => layers.flatMap((layer) =>
      is('video')(layer) ? [layer] : find(layer.content.type === 'group' ? layer.content.layers : []));
    return find(mounted.renderAt({ value: frame, timescale: 30 }, null).scene.layers)[0];
  };
  for (const frame of [5, 10, 25, 69, 75]) {
    assert.deepEqual(videoOf(framed, frame)?.content, videoOf(bare, frame)?.content, `frame ${frame}`);
  }
  // Both draw nothing outside the Sequence and the same video inside it.
  assert.equal(videoOf(framed, 5), undefined);
  const inside = videoOf(framed, 25)!.content;
  assert.ok(Math.abs(inside.timing.sourceTimeSeconds - (1.5 + (15 / 30) * 2)) < 1e-9);
  assert.deepEqual(framed.collectAudio(), bare.collectAudio());
  assert.ok(framed.collectAudio().length > 0);
});

test('a muted video stays muted', () => {
  const mounted = composition(h(MediaFrame, { ...video, muted: true, width: 400, height: 300 }), 3);
  assert.ok(mounted.collectAudio().every((clip) => clip.muted));
});

test('a caption sits under the frame, wrapped to its width, and moves with the anchor and transforms', () => {
  const [frame] = render(h(MediaFrame, {
    ...video, x: 960, y: 540, anchorX: 0.5, anchorY: 0.5, scale: 1.5, rotation: 5,
    width: 400, height: 300, padding: { top: 20, left: 10 }, caption: 'A caption', captionGap: 16, captionStyle: { fontSize: 20, align: 'center' },
  }))();
  const { content, caption } = parts(frame);
  assert.deepEqual(frame.transform.position, { x: 960, y: 540 });
  assert.deepEqual(frame.transform.scale, { x: 1.5, y: 1.5 });
  assert.equal(frame.transform.rotation, 5);
  // The content group is offset by the anchor and the padding; the caption
  // goes back to the frame's left edge, 16 below its bottom, on its baseline.
  assert.deepEqual(content.transform.position, { x: -190, y: -130 });
  assert.equal(caption.content.text, 'A caption');
  assert.deepEqual(caption.transform.position, { x: -10, y: 300 - 20 + 16 + 20 });
  assert.equal(caption.content.maxWidth, 400);
  assert.equal(caption.content.style.align, 'center');
});

test('no caption draws no text', () => {
  const [frame] = render(h(MediaFrame, { ...video, width: 400, height: 300 }))();
  assert.equal(parts(frame).caption, undefined);
});

test('kind, fit, variant, and the video size are checked', () => {
  // Deliberately invalid props, so they are not typed as MediaFrameProps; cast
  // where they are passed.
  const cases: [Record<string, unknown>, RegExp][] = [
    [{ ...video, kind: 'audio' }, /<MediaFrame> kind must be 'image' or 'video', not "audio"/],
    [{ ...video, kind: undefined }, /<MediaFrame> kind must be/],
    [{ ...video, fit: 'fill' }, /<MediaFrame> fit must be 'contain' or 'cover'/],
    [{ ...video, variant: 'browser' }, /<MediaFrame> variant must be 'plain', not "browser"/],
    [{ ...video, sourceWidth: undefined }, /kind="video" needs a positive sourceWidth.*preloadMedia\(\) in prepare\(\)/],
    [{ ...video, sourceHeight: 0 }, /kind="video" needs a positive sourceHeight/],
    [{ ...video, captionGap: -1 }, /<MediaFrame> captionGap must be/],
    [{ ...video, padding: Number.NaN }, /<MediaFrame> padding top must be/],
    [{ ...video, width: -1 }, /<Panel> width must be/],
  ];
  for (const [props, message] of cases) {
    assert.throws(() => render(h(MediaFrame, { width: 400, height: 300, ...props } as MediaFrameProps))(), message, JSON.stringify(props));
  }
});

test('a frame renders the same on a seek as when played up to it, inside a nested Sequence', () => {
  const scene = h(Sequence, { from: 10 }, h(Sequence, { from: 5 },
    h(MediaFrame, { ...video, width: 640, height: 360, fit: 'cover', cornerRadius: 12, caption: 'clip' })));
  const played = render(scene);
  for (let frame = 0; frame <= 40; frame += 1) played(frame);
  assert.deepEqual(played(40), render(scene)(40));
});
