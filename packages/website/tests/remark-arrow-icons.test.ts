import assert from 'node:assert/strict';
import type { PhrasingContent } from 'mdast';
import { test } from 'vitest';
import remarkParse from 'remark-parse';
import { unified } from 'unified';
import { remarkArrowIcons } from '../plugins/remark-arrow-icons.ts';

const processor = unified().use(remarkParse).use(remarkArrowIcons);

/**
 * The `data` fields mdast-util-to-hast reads, which the plugin sets on an icon. Its module augmentation
 * of mdast's `Data` is not a dependency here, so the node's own `data` type does not list them.
 */
interface HastData {
  hName?: string;
  hProperties?: Record<string, unknown>;
}

/** A node of the compact tree `paragraph` returns. */
type Compact = string | { icon: string; hidden: unknown } | { type: string; children: Compact[] } | { type: string; value: string | undefined };

/** The first paragraph's content as a compact tree: text as strings, icons as `{ icon, hidden }` (its classes after `nr-icon`), other nodes as `{ type, children | value }`. */
function paragraph(markdown: string): Compact[] {
  const tree = processor.runSync(processor.parse(markdown));
  const compact = (node: PhrasingContent): Compact => {
    const data = node.data as HastData | undefined;
    if (data?.hName === 'span') {
      const className = data.hProperties?.className;
      assert.ok(Array.isArray(className), 'an icon has a class list');
      return { icon: className.slice(1).join(' '), hidden: data.hProperties?.['aria-hidden'] };
    }
    if (node.type === 'text') return node.value;
    if ('children' in node) return { type: node.type, children: node.children.map(compact) };
    return { type: node.type, value: 'value' in node ? node.value : undefined };
  };
  const [block] = tree.children;
  const first = block.type === 'list' ? block.children[0].children[0] : block;
  assert.ok(first.type === 'paragraph', 'the first block is a paragraph');
  return first.children.map(compact);
}

/** An icon span; `at` is `start` or `end` when the glyph began or ended its text node. */
const icon = (name: string, at?: 'start' | 'end') => ({ icon: at ? `nr-icon-${name} nr-icon-at-${at}` : `nr-icon-${name}`, hidden: 'true' });
/** The text a reader (or Pagefind, or copy-paste) gets: icons contribute nothing. */
const text = (nodes: Compact[]): string => nodes.map((node) => (typeof node === 'string' ? node : 'children' in node ? text(node.children) : 'value' in node ? node.value ?? '' : '')).join('');

test('a trailing ↗ in link text becomes an icon inside the link', () => {
  assert.deepEqual(paragraph('[Report a problem on GitHub ↗](https://github.com/mika-f/Celesta/issues)\n'), [
    { type: 'link', children: ['Report a problem on GitHub', icon('arrow-up-right', 'end')] },
  ]);
});

test('a leading ← becomes an icon before the text', () => {
  assert.deepEqual(paragraph('← All compositions\n'), [icon('arrow-left', 'start'), 'All compositions']);
});

test('a trailing → inside strong becomes an icon inside strong', () => {
  assert.deepEqual(paragraph('- **Start here →**\\\n  Install\n'), [
    { type: 'strong', children: ['Start here', icon('arrow-right', 'end')] },
    { type: 'break', value: undefined },
    'Install',
  ]);
});

test('each arrow glyph maps to its lucide icon', () => {
  assert.deepEqual(paragraph('[Download for macOS ↓](celesta:download/macos) up ↑\n'), [
    { type: 'link', children: ['Download for macOS', icon('arrow-down', 'end')] },
    ' up',
    icon('arrow-up', 'end'),
  ]);
});

test('an arrow between words becomes an icon with one space kept on each side', () => {
  const nodes = paragraph('Choose **File → Open…** now.\n');
  assert.deepEqual(nodes, [
    'Choose ',
    { type: 'strong', children: ['File ', icon('arrow-right'), ' Open…'] },
    ' now.',
  ]);
  assert.equal(text(nodes), 'Choose File  Open… now.');
});

test('an arrow between emphasis keeps the spaces around it', () => {
  const nodes = paragraph('**01** Write → **02** Preview\n');
  assert.deepEqual(nodes, [
    { type: 'strong', children: ['01'] },
    ' Write ',
    icon('arrow-right'),
    ' ',
    { type: 'strong', children: ['02'] },
    ' Preview',
  ]);
});

test('an arrow at either edge of its text leaves no stray space', () => {
  const trailing = paragraph('[Report a problem on GitHub ↗](https://github.com/mika-f/Celesta/issues)\n');
  const leading = paragraph('← All compositions\n');
  assert.equal(text(trailing), 'Report a problem on GitHub');
  assert.equal(text(leading), 'All compositions');
  for (const value of [text(trailing), text(leading)]) assert.doesNotMatch(value, /^\s|\s$|\s{2}/);
});

test('arrows in inline code and code blocks stay', () => {
  assert.deepEqual(paragraph('Press `a → b` or `←`.\n'), [
    'Press ',
    { type: 'inlineCode', value: 'a → b' },
    ' or ',
    { type: 'inlineCode', value: '←' },
    '.',
  ]);
  const [code] = processor.runSync(processor.parse('```ts\nconst x = 0; // 0 → 1\n```\n')).children;
  assert.ok(code.type === 'code', 'the block is a code block');
  assert.equal(code.value, 'const x = 0; // 0 → 1');
});

test('text without an arrow is untouched', () => {
  assert.deepEqual(paragraph('Plain text, with [a link](/en/docs/) and **bold**.\n'), [
    'Plain text, with ',
    { type: 'link', children: ['a link'] },
    ' and ',
    { type: 'strong', children: ['bold'] },
    '.',
  ]);
});
