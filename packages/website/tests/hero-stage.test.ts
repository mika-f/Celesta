import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  clipAtFrame,
  formatHeroTimecode,
  frameToRatio,
  HERO_CLIPS,
  HERO_TOTAL_FRAMES,
  normalizeFrame,
  ratioToFrame,
} from '../pages/_components/hero-stage-draw.ts';

test('hero timecode stays within the 10-second loop at 30 fps', () => {
  assert.equal(formatHeroTimecode(0), '00:00:00:00');
  assert.equal(formatHeroTimecode(29), '00:00:00:29');
  assert.equal(formatHeroTimecode(30), '00:00:01:00');
  assert.equal(formatHeroTimecode(HERO_TOTAL_FRAMES - 1), '00:00:09:29');
  assert.equal(formatHeroTimecode(HERO_TOTAL_FRAMES), '00:00:00:00');
  assert.equal(formatHeroTimecode(-1), '00:00:09:29');
});

test('hero clips cover every frame contiguously from 0 to HERO_TOTAL_FRAMES', () => {
  assert.equal(HERO_CLIPS[0].startFrame, 0);
  for (let i = 1; i < HERO_CLIPS.length; i++) {
    assert.equal(HERO_CLIPS[i].startFrame, HERO_CLIPS[i - 1].endFrame);
  }
  assert.equal(HERO_CLIPS[HERO_CLIPS.length - 1].endFrame, HERO_TOTAL_FRAMES);

  assert.equal(clipAtFrame(0).id, 'story');
  assert.equal(clipAtFrame(104).id, 'story');
  assert.equal(clipAtFrame(105).id, 'atmosphere');
  assert.equal(clipAtFrame(204).id, 'atmosphere');
  assert.equal(clipAtFrame(205).id, 'voice');
  assert.equal(clipAtFrame(299).id, 'voice');
  assert.equal(clipAtFrame(300).id, 'story');
});

test('scrubber ratio and frame conversions round-trip at bounds and midpoints', () => {
  assert.equal(normalizeFrame(-301), HERO_TOTAL_FRAMES - 1);
  assert.equal(frameToRatio(0), 0);
  assert.equal(frameToRatio(HERO_TOTAL_FRAMES - 1), 1);
  assert.equal(ratioToFrame(0), 0);
  assert.equal(ratioToFrame(1), HERO_TOTAL_FRAMES - 1);
  assert.equal(ratioToFrame(frameToRatio(150)), 150);
  assert.equal(ratioToFrame(-0.5), 0);
  assert.equal(ratioToFrame(1.5), HERO_TOTAL_FRAMES - 1);
});
