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
  startHeroPlayback,
  stepClampedFrame,
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

test('keyboard frame stepping clamps at timeline endpoints instead of wrapping', () => {
  assert.equal(stepClampedFrame(0, -1), 0);
  assert.equal(stepClampedFrame(4, -10), 0);
  assert.equal(stepClampedFrame(0, 1), 1);
  assert.equal(stepClampedFrame(HERO_TOTAL_FRAMES - 1, 1), HERO_TOTAL_FRAMES - 1);
  assert.equal(stepClampedFrame(HERO_TOTAL_FRAMES - 5, 10), HERO_TOTAL_FRAMES - 1);
  assert.equal(stepClampedFrame(HERO_TOTAL_FRAMES - 1, -1), HERO_TOTAL_FRAMES - 2);
});

test('playback clock rebases on visibilitychange without skipping across hidden tab gaps', () => {
  let frame = 100;
  let hidden = false;
  let nextId = 1;
  const scheduled = new Map<number, (now: number) => void>();
  const visibilityListeners = new Set<() => void>();

  const stop = startHeroPlayback(
    () => frame,
    (next) => { frame = next; },
    {
      isHidden: () => hidden,
      onVisibilityChange: (listener) => {
        visibilityListeners.add(listener);
        return () => visibilityListeners.delete(listener);
      },
      requestFrame: (cb) => {
        const id = nextId++;
        scheduled.set(id, cb);
        return id;
      },
      cancelFrame: (id) => {
        scheduled.delete(id);
      },
    },
  );

  const runNextFrame = (now: number) => {
    const entry = scheduled.entries().next().value;
    assert.ok(entry, 'expected a scheduled animation frame');
    scheduled.delete(entry[0]);
    entry[1](now);
  };

  // Establish initial clock at t=1000ms and advance 3 frames over 100ms.
  runNextFrame(1000);
  assert.equal(frame, 100);
  runNextFrame(1100);
  assert.equal(frame, 103);

  // Hide the tab: pending animation frame is cancelled and no callbacks fire for 2 seconds.
  hidden = true;
  for (const listener of visibilityListeners) listener();
  assert.equal(scheduled.size, 0);

  // Return to the tab 2 seconds later: clock rebases instead of skipping ~60 frames.
  hidden = false;
  for (const listener of visibilityListeners) listener();
  assert.equal(scheduled.size, 1);

  runNextFrame(3100);
  assert.equal(frame, 103);
  runNextFrame(3134);
  assert.equal(frame, 104);

  stop();
  assert.equal(scheduled.size, 0);
  assert.equal(visibilityListeners.size, 0);
});
