// Source chains: a host node's fiber, walked up to the elements that produced
// it.

import assert from 'node:assert/strict';
import { Fragment, forwardRef, memo } from 'react';
import type { ReactNode } from 'react';
import { jsx } from 'react/jsx-runtime';
import { test } from 'vitest';

import { markPrimitive, sourceChain } from '../src/internal.js';
import { markTransparent } from '../src/source-chain.js';
import { jsxDEV } from '../src/jsx-dev-runtime.js';
import type { SourcePosition } from '../src/jsx-dev-runtime.js';
import { createRoot } from '../src/reconciler.js';
import type { HostNode } from '../src/reconciler.js';

const at = (fileName: string, lineNumber: number, columnNumber: number): SourcePosition => ({ fileName, lineNumber, columnNumber });
const resolve = (file: string) => `/abs/${file}`;

function mount(element: ReactNode) {
  const root = createRoot();
  root.render(element);
  return { container: root.container, render: (next: ReactNode) => root.render(next) };
}

function Title() {
  return jsxDEV('rect', { width: 1 }, undefined, false, at('title.tsx', 5, 3));
}

test('lists the nearest element first, then its ancestors', () => {
  const { container } = mount(jsxDEV(Title, {}, undefined, false, at('entry.tsx', 9, 5)));
  assert.deepEqual(sourceChain(container.children[0], resolve), [
    { name: 'rect', file: '/abs/title.tsx', line: 5, column: 3, kind: 'component' },
    { name: 'Title', file: '/abs/entry.tsx', line: 9, column: 5, kind: 'component' },
  ]);
});

test('skips elements without a source', () => {
  function Middle() {
    return jsx(Title, {});
  }
  const { container } = mount(jsxDEV(Middle, {}, undefined, false, at('entry.tsx', 9, 5)));
  assert.deepEqual(sourceChain(container.children[0], resolve), [
    { name: 'rect', file: '/abs/title.tsx', line: 5, column: 3, kind: 'component' },
    { name: 'Middle', file: '/abs/entry.tsx', line: 9, column: 5, kind: 'component' },
  ]);
});

test('merges adjacent duplicates', () => {
  // Both fibers were written at the same place, as a component returning
  // the element it was given does.
  const same = at('wrap.tsx', 2, 2);
  function Wrap({ children }: { children: ReactNode }) {
    return children;
  }
  const { container } = mount(
    jsxDEV(Wrap, { children: jsxDEV('rect', {}, undefined, false, same) }, undefined, false, same),
  );
  const chain = sourceChain(container.children[0], resolve);
  assert.deepEqual(chain, [{ name: 'rect', file: '/abs/wrap.tsx', line: 2, column: 2, kind: 'component' }]);
});

test('is empty without positions', () => {
  function Plain() {
    return jsx('rect', {});
  }
  const { container } = mount(jsx(Plain, {}));
  assert.deepEqual(sourceChain(container.children[0], resolve), []);
});

test('survives updates that skip commitUpdate', () => {
  const entry = () => jsxDEV(Title, {}, undefined, false, at('entry.tsx', 9, 5));
  const { container, render } = mount(entry());
  const node = container.children[0];
  const before = sourceChain(node, resolve);
  render(entry());
  render(entry());
  assert.equal(container.children[0], node);
  assert.deepEqual(sourceChain(node, resolve), before);
  assert.equal(before.length, 2);
});

test('names memo and forwardRef components', () => {
  const Memo = memo(Title);
  const Forwarded = forwardRef(function Inner(_props, _ref) {
    return jsxDEV('rect', {}, undefined, false, at('inner.tsx', 1, 1));
  });
  Forwarded.displayName = 'Shown';
  const { container } = mount(
    jsxDEV('group', {
      children: [
        jsxDEV(Memo, {}, 'a', false, at('entry.tsx', 1, 1)),
        jsxDEV(Forwarded, {}, 'b', false, at('entry.tsx', 2, 1)),
      ],
    }, undefined, true, at('entry.tsx', 0, 0)),
  );
  const [memoRect, forwardedRect] = container.children[0].children;
  assert.equal(sourceChain(memoRect, resolve)[1].name, 'Title');
  assert.equal(sourceChain(forwardedRect, resolve)[1].name, 'Shown');
});

test('tells a marked primitive from a component of the same name', () => {
  const Image = forwardRef(function Image(_props, _ref) {
    return jsxDEV('image', {}, undefined, false, at('image.tsx', 1, 1));
  });
  markPrimitive(Image);
  function Rect() {
    return jsxDEV('rect', {}, undefined, false, at('rect.tsx', 1, 1));
  }
  const { container } = mount(
    jsxDEV('group', {
      children: [
        jsxDEV(Image, {}, 'a', false, at('entry.tsx', 1, 1)),
        jsxDEV(Rect, {}, 'b', false, at('entry.tsx', 2, 1)),
      ],
    }, undefined, true, at('entry.tsx', 0, 0)),
  );
  const [image, rect] = container.children[0].children;
  assert.deepEqual(sourceChain(image, resolve)[1], { name: 'Image', file: '/abs/entry.tsx', line: 1, column: 1, kind: 'primitive' });
  assert.deepEqual(sourceChain(rect, resolve)[1], { name: 'Rect', file: '/abs/entry.tsx', line: 2, column: 1, kind: 'component' });
});

test('omits layerId when the primitive has no host descendant', () => {
  // A real tree always has one; build the fibers by hand.
  function Outer() {}
  markPrimitive(Outer);
  interface Fiber {
    return: Fiber | null;
    child: Fiber | null;
    type: unknown;
    elementType: unknown;
    memoizedProps: unknown;
    stateNode?: unknown;
  }
  const props = (line: number) => jsxDEV('x', {}, undefined, false, at('f.tsx', line, 1)).props;
  const outer: Fiber = { return: null, child: null, type: Outer, elementType: Outer, memoizedProps: props(1) };
  const innerFiber: Fiber = { return: outer, child: null, type: 'rect', elementType: 'rect', memoizedProps: props(2) };
  // Only the fiber, which is all sourceChain reads of the node.
  const inner = { fiber: innerFiber } as HostNode;
  innerFiber.stateNode = inner;
  const chain = sourceChain(inner, resolve, (node) => (node === inner ? 'inner' : undefined));
  const entry = chain.find((e) => e.name === 'Outer');
  assert.ok(entry);
  assert.equal(entry.kind, 'primitive');
  assert.equal('layerId' in entry, false);
});

// Primitives as the runtime has them: components that render a host node
// without a source position of their own.
function G({ children }: { children?: ReactNode }) {
  return jsxDEV('group', { children }, undefined, false);
}
function R() {
  return jsxDEV('rect', {}, undefined, false);
}
markPrimitive(G);
markPrimitive(R);

/** Names every host node under `container` by `name`, for `layerIdOf`. */
function ids(names: [HostNode, string][]): (node: HostNode) => string | undefined {
  const map = new Map(names);
  return (node) => map.get(node);
}

test('gives a component the id of its single root layer', () => {
  function PriceTag() {
    return jsxDEV(G, { children: jsxDEV(R, {}, undefined, false, at('tag.tsx', 3, 7)) }, undefined, false, at('tag.tsx', 2, 5));
  }
  const { container } = mount(jsxDEV(PriceTag, {}, undefined, false, at('scene.tsx', 9, 5)));
  const group = container.children[0];
  const rect = group.children[0];
  const chain = sourceChain(rect, resolve, ids([[group, 'g'], [rect, 'r']]));
  assert.deepEqual(
    chain.map((entry) => [entry.name, entry.kind, entry.layerId]),
    [
      ['R', 'primitive', 'r'],
      ['G', 'primitive', 'g'],
      ['PriceTag', 'component', 'g'],
    ],
  );
});

test('gives a component that returns another component the same root', () => {
  function Tag() {
    return jsxDEV(G, { children: jsxDEV(R, {}, undefined, false, at('tag.tsx', 3, 7)) }, undefined, false, at('tag.tsx', 2, 5));
  }
  function PriceTag() {
    return jsxDEV(Tag, {}, undefined, false, at('price.tsx', 2, 5));
  }
  const { container } = mount(jsxDEV(PriceTag, {}, undefined, false, at('scene.tsx', 9, 5)));
  const group = container.children[0];
  const rect = group.children[0];
  const chain = sourceChain(rect, resolve, ids([[group, 'g'], [rect, 'r']]));
  assert.equal(chain.find((entry) => entry.name === 'Tag')?.layerId, 'g');
  assert.equal(chain.find((entry) => entry.name === 'PriceTag')?.layerId, 'g');
});

test('looks through a wrapper and empty children for the root', () => {
  function Window({ children }: { children?: ReactNode }) {
    return children;
  }
  markTransparent(Window);
  function Card() {
    return jsx(Fragment, {
      children: [
        null,
        false,
        jsxDEV(
          Window,
          { children: jsxDEV(G, { children: jsxDEV(R, {}, undefined, false, at('card.tsx', 4, 9)) }, undefined, false, at('card.tsx', 3, 7)) },
          'w',
          false,
          at('card.tsx', 2, 5),
        ),
      ],
    });
  }
  const { container } = mount(jsxDEV(Card, {}, undefined, false, at('scene.tsx', 9, 5)));
  const group = container.children[0];
  const rect = group.children[0];
  const chain = sourceChain(rect, resolve, ids([[group, 'g'], [rect, 'r']]));
  assert.equal(chain.find((entry) => entry.name === 'Card')?.layerId, 'g');
});

test('omits layerId for a component that draws several layers', () => {
  function Pair() {
    return jsx(Fragment, {
      children: [
        jsxDEV(R, {}, 'a', false, at('pair.tsx', 2, 5)),
        jsxDEV(R, {}, 'b', false, at('pair.tsx', 3, 5)),
      ],
    });
  }
  const { container } = mount(jsxDEV(Pair, {}, undefined, false, at('scene.tsx', 9, 5)));
  const [first, second] = container.children;
  const chain = sourceChain(first, resolve, ids([[first, 'a'], [second, 'b']]));
  const entry = chain.find((e) => e.name === 'Pair');
  assert.ok(entry);
  assert.equal('layerId' in entry, false);
});

test('omits layerId on wrapper entries', () => {
  function Window({ children }: { children?: ReactNode }) {
    return children;
  }
  markTransparent(Window);
  const { container } = mount(
    jsxDEV(Window, { children: jsxDEV(G, {}, undefined, false, at('scene.tsx', 3, 7)) }, undefined, false, at('scene.tsx', 2, 5)),
  );
  const group = container.children[0];
  const chain = sourceChain(group, resolve, ids([[group, 'g']]));
  const entry = chain.find((e) => e.name === 'Window');
  assert.ok(entry);
  assert.equal(entry.kind, 'wrapper');
  assert.equal('layerId' in entry, false);
});

test('omits layerId for a component whose root is not written in the source', () => {
  // As `<Stack>` does: a component of a package draws the group around
  // its children with no source position, so the group can't be edited.
  function Stack({ children }: { children?: ReactNode }) {
    return jsx(G, { children });
  }
  const { container } = mount(
    jsxDEV(Stack, { children: jsxDEV(R, {}, undefined, false, at('scene.tsx', 3, 7)) }, undefined, false, at('scene.tsx', 2, 5)),
  );
  const group = container.children[0];
  const rect = group.children[0];
  const chain = sourceChain(rect, resolve, ids([[group, 'g'], [rect, 'r']]));
  const entry = chain.find((e) => e.name === 'Stack');
  assert.ok(entry);
  assert.equal(entry.kind, 'component');
  assert.equal('layerId' in entry, false);
});
