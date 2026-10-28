// Fixes up Makit's output in dist/ after `makit build` (run by `pnpm build`). The Markdown
// endpoints (`<page>.md`) and llms-full.txt are copied from the sources without the remark
// plugins in makit.config.ts, so example fences are empty and download links keep their
// `celesta:download/…` placeholders. The same plugins fill them here.
//
// Markdown is edited in place at the positions remark reports instead of being re-serialized
// whole: a round trip through remark-stringify rewrites untouched prose (list markers, escapes
// such as `\[!NOTE]`, tables without GFM), and llms-full.txt concatenates many pages.

import { readdir, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import remarkParse from 'remark-parse';
import remarkStringify from 'remark-stringify';
import { unified } from 'unified';
import { visit } from 'unist-util-visit';
import { remarkDownloadLinks } from '../plugins/remark-download-links.ts';
import { remarkInclude } from '../plugins/remark-include.ts';

const packageRoot = fileURLToPath(new URL('..', import.meta.url));

async function* files(dir, extension) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) yield* files(path, extension);
    else if (entry.name.endsWith(extension)) yield path;
  }
}

const parser = unified().use(remarkParse);
const stringifier = unified().use(remarkStringify);

/**
 * Applies remarkInclude and remarkDownloadLinks to Markdown text, changing only the code fences
 * and link destinations the plugins changed.
 */
export async function fillMarkdown(markdown, root = packageRoot) {
  const tree = parser.parse(markdown);
  const before = new Map();
  visit(tree, ['code', 'link', 'definition'], node => { before.set(node, snapshot(node)); });
  await unified().use(remarkInclude, { root }).use(remarkDownloadLinks).run(tree);

  const edits = [];
  for (const [node, was] of before) {
    if (snapshot(node) === was) continue;
    const { start, end } = node.position;
    const source = markdown.slice(start.offset, end.offset);
    if (node.type === 'code') {
      edits.push({ start: start.offset, end: end.offset, text: fence(node, markdown, start.offset) });
    } else {
      // The plugin only changed the destination; keep the link text and title as written.
      const at = source.lastIndexOf(was);
      if (at < 0) throw new Error(`postbuild: cannot find "${was}" in ${source}`);
      const url = /[\s()<>]/.test(node.url) ? `<${node.url}>` : node.url;
      edits.push({ start: start.offset + at, end: start.offset + at + was.length, text: url });
    }
  }
  let output = markdown;
  for (const edit of edits.sort((a, b) => b.start - a.start)) output = output.slice(0, edit.start) + edit.text + output.slice(edit.end);
  return output;
}

const snapshot = node => node.type === 'code' ? `${node.lang} ${node.meta}\n${node.value}` : node.url;

/** Serializes a filled fence, continuing its container's prefix (list indent, `>`) on every line. */
function fence(node, markdown, offset) {
  const lineStart = markdown.lastIndexOf('\n', offset - 1) + 1;
  const prefix = markdown.slice(lineStart, offset).replace(/[^>\s]/g, ' ');
  const text = stringifier.stringify({ type: 'root', children: [{ type: 'code', lang: node.lang, meta: node.meta, value: node.value }] }).replace(/\n$/, '');
  // A blank line keeps a blockquote's `>` so the fence does not end the quote.
  return text.split('\n').map((line, index) => index === 0 ? line : line ? prefix + line : prefix.trimEnd()).join('\n');
}

/** Fills every `*.md` endpoint and llms-full.txt under `dist`. Returns the number of files changed. */
export async function fillMarkdownOutputs(dist, root = packageRoot) {
  const paths = [];
  for await (const path of files(dist, '.md')) paths.push(path);
  paths.push(join(dist, 'llms-full.txt'));
  let changed = 0;
  for (const path of paths) {
    let markdown;
    try {
      markdown = await readFile(path, 'utf8');
    } catch (error) {
      if (error.code === 'ENOENT' && path.endsWith('llms-full.txt')) continue;
      throw error;
    }
    const next = await fillMarkdown(markdown, root);
    if (next !== markdown) {
      await writeFile(path, next);
      changed++;
    }
  }
  return changed;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const dist = join(packageRoot, 'dist');
  console.log(`postbuild: filled ${await fillMarkdownOutputs(dist)} Markdown file(s)`);
}
