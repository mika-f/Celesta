// The film's data, taken from this repository's own Git history.

// ── Data ─────────────────────────────────────────────────────────────────
// git log --no-merges --date=short --format=%ad | sort | uniq -c
export const FIRST_DAY = Date.UTC(2026, 7, 25);
export const COMMITS_BY_DATE: Record<string, number> = {
  '2026-08-25': 25, '2026-08-26': 6, '2026-08-28': 11, '2026-08-29': 7,
  '2026-08-30': 10, '2026-08-31': 7, '2026-09-07': 21, '2026-09-08': 7,
  '2026-09-24': 4, '2026-09-25': 17, '2026-09-26': 1, '2026-09-28': 15,
  '2026-09-29': 6,
};
export const DAYS = Array.from({ length: 36 }, (_, i) => {
  const date = new Date(FIRST_DAY + i * 86_400_000).toISOString().slice(0, 10);
  return { date, commits: COMMITS_BY_DATE[date] ?? 0 };
});
export const TOTAL_COMMITS = 163; // git rev-list --count HEAD, merges included
export const NON_MERGE = DAYS.reduce((n, d) => n + d.commits, 0);
// find crates/<name> -name '*.rs' | xargs cat | wc -l
export const SOURCES = [
  { name: 'editor', lines: 5176 },
  { name: 'gpu-renderer', lines: 4610 },
  { name: '@celesta/react', lines: 3418, ts: true },
  { name: 'renderer', lines: 2859 },
  { name: 'exporter', lines: 2601 },
  { name: 'react-bridge', lines: 2248 },
  { name: 'media', lines: 1263 },
  { name: 'composition', lines: 1261 },
  { name: 'project', lines: 1120 },
  { name: 'evaluator', lines: 728 },
  { name: 'editor-core', lines: 707 },
  { name: 'remote', lines: 682 },
  { name: 'editor-theme', lines: 77 },
];
export const RUST_LINES = SOURCES.filter((s) => !s.ts).reduce((n, s) => n + s.lines, 0);
export const TS_LINES = SOURCES.filter((s) => s.ts).reduce((n, s) => n + s.lines, 0);

export const fmt = (n: number) => n.toLocaleString('en-US');
