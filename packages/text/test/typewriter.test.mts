import assert from 'node:assert/strict';
import { test } from 'vitest';
import React from 'react';
import type { ReactNode } from 'react';

import { Composition, Span } from '@celesta.studio/react';
import { mount } from '@celesta.studio/react/internal';
import { typewriterFrames, useTypewriter } from '../src/index.js';
import type { Typewriter, TypewriterOptions } from '../src/index.js';
import { typedCount } from '../src/text-motion.js';

const h = React.createElement;

/** `useTypewriter(content, options)` as a component sees it on `frame`. */
function typewriterAt(content: ReactNode, frame: number, options?: TypewriterOptions): Typewriter {
  let typed: Typewriter | undefined;
  const Probe = () => {
    typed = useTypewriter(content, options);
    return null;
  };
  const Root = () => h(Composition, { width: 1920, height: 1080, fps: 30, durationInFrames: 1000 }, h(Probe));
  mount(Root).renderAt({ value: frame, timescale: 30 }, null);
  return typed!;
}

test('typewriterFrames without pauses is from + ceil(i * framesPerChar) on whole frames', () => {
  assert.deepEqual(typewriterFrames('abcd', { from: 10, framesPerChar: 2 }), [10, 12, 14, 16]);
  assert.deepEqual(typewriterFrames('abcd', { framesPerChar: 0.5 }), [0, 1, 1, 2]);
  assert.deepEqual(typewriterFrames('abc'), [0, 1, 2]);
  assert.deepEqual(typewriterFrames(''), []);
});

test('a pause delays every character after it', () => {
  assert.deepEqual(
    typewriterFrames('はい、できました。', { from: 10, framesPerChar: 2, pauses: { '、': 6, '。': 12 } }),
    [10, 12, 14, 22, 24, 26, 28, 30, 32],
  );
  // Fractional pauses and speeds add up before rounding to a frame.
  assert.deepEqual(typewriterFrames('a,b,c', { framesPerChar: 0.5, pauses: { ',': 1.25 } }), [0, 1, 3, 3, 5]);
});

test('frames[i] is the first whole frame on which the character is shown, for awkward speeds', () => {
  const characters = Array.from('x'.repeat(200));
  for (const framesPerChar of [0.3, 0.7, 1, 1.1, 1.5, 2.2, 3, 7.3]) {
    for (const pauses of [undefined, { x: 0.4 }]) {
      const options = { from: 3, framesPerChar, pauses };
      const frames = typewriterFrames(characters.join(''), options);
      for (const i of [0, 1, 29, 30, 169, 170, 199]) {
        assert.ok(typedCount(characters, frames[i], options) > i, `${framesPerChar} shows ${i} on ${frames[i]}`);
        assert.ok(typedCount(characters, frames[i] - 1, options) <= i, `${framesPerChar} hides ${i} before ${frames[i]}`);
      }
    }
  }
});

test('without pauses, the typed count is unchanged on whole and fractional frames', () => {
  const characters = Array.from('x'.repeat(200));
  for (const frame of [-2, 0, 1, 1.5, 2.25, 33, 34, 187, 188, 5000]) {
    for (const framesPerChar of [0.4, 1, 1.1, 1.5]) {
      assert.equal(
        typedCount(characters, frame, { framesPerChar }),
        Math.min(200, Math.max(0, Math.floor(frame / framesPerChar) + 1)),
      );
    }
  }
});

test('the hook reports the same frames, counts code points, and keeps the caret steady in a pause', () => {
  const content = ['a😀', h(Span, { key: 's', color: '#ff0000' }, 'b。'), 'c'];
  const options = { framesPerChar: 2, pauses: { '。': 10 }, blinkFrames: 4 };
  const typed = typewriterAt(content, 9, options);
  assert.deepEqual(typed.frames, [0, 2, 4, 6, 18]);
  assert.deepEqual(typed.frames, typewriterFrames('a😀b。c', options));
  assert.equal(typed.length, 4); // frame 9: inside the pause after 。
  assert.equal(typed.text, 'a😀b。');
  // Frame 11 is in the "off" half of the idle blink (phase 3 of 4), so only
  // typing keeps the caret on.
  assert.equal(typewriterAt(content, 11, options).caretVisible, true);
  assert.equal(typed.done, false);
  assert.equal(typewriterAt(content, 17, options).length, 4);
  assert.equal(typewriterAt(content, 18, options).length, 5);
  assert.equal(typewriterAt(content, 18, options).done, true);
});

test('a pause after the last character delays nothing', () => {
  const end = typewriterAt('ab。', 4, { framesPerChar: 2, pauses: { '。': 30 } });
  assert.equal(end.done, true);
  assert.equal(end.length, 3);
});

test('without pauses the hook is as before', () => {
  const typed = typewriterAt('あい🙂x', 6, { from: 2, framesPerChar: 2 });
  assert.equal(typed.text, 'あい🙂');
  assert.equal(typed.length, 3);
  assert.deepEqual(typed.frames, [2, 4, 6, 8]);
  assert.equal(typewriterAt('abc', 0, { from: 5 }).length, 0);
});

test('pauses and speeds are validated', () => {
  assert.throws(() => typewriterFrames('a', { pauses: { ab: 3 } }), /one character/);
  assert.throws(() => typewriterFrames('a', { pauses: { '': 3 } }), /one character/);
  assert.throws(() => typewriterFrames('a', { pauses: { a: -1 } }), /pauses/);
  assert.throws(() => typewriterFrames('a', { pauses: { a: NaN } }), /pauses/);
  assert.throws(() => typewriterFrames('a', { framesPerChar: 0 }), /positive framesPerChar/);
  assert.throws(() => typewriterFrames('a', { from: NaN }), /finite from/);
  assert.throws(() => typewriterAt('a', 0, { pauses: { ab: 3 } }), /one character/);
  // An emoji of one code point is one character.
  assert.deepEqual(typewriterFrames('😀a', { pauses: { '😀': 5 } }), [0, 6]);
});
