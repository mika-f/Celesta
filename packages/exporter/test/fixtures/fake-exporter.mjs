#!/usr/bin/env node
// Stays plain JavaScript: run.test.mts spawns it as the exporter binary
// through its shebang, with no TypeScript loader in between.
// Stands in for celesta-exporter: FAKE_MODE picks what it prints.
const line = (value) => process.stdout.write(`${JSON.stringify(value)}\n`);
const composition = { width: 640, height: 360, fps: 30, frames: 30, duration: 1 };
const ok = (extra = {}) => ({
  status: 'ok',
  source: '/work/film.tsx',
  composition,
  outputs: [],
  warnings: [],
  stats: { elapsedSeconds: 0.1 },
  ...extra,
});
const error = (code, message, hint) => ({ status: 'error', source: '/work/film.tsx', error: { code, message, ...(hint ? { hint } : {}) } });

const video = {
  type: 'video',
  path: '/work/film.mp4',
  firstFrame: 0,
  frames: 30,
  start: 0,
  duration: 1,
  muxer: 'ffmpeg/mp4',
  video: { component: 'ffmpeg/libx264', codec: 'avc1.64001f' },
  audio: null,
  alpha: false,
  files: ['/work/film.mp4'],
};

switch (process.env.FAKE_MODE) {
  case 'hang':
    setInterval(() => {}, 1000);
    break;
  case 'component-error':
    line(error('component_unavailable', 'no such component: ffmpeg/nope'));
    process.exitCode = 1;
    break;
  case 'video':
    line({
      event: 'output',
      muxer: 'ffmpeg/mp4',
      muxerName: 'MP4',
      video: { component: 'ffmpeg/libx264', name: 'H.264 (x264)', settings: ['CRF 20'] },
      audio: null,
    });
    line(ok({ outputs: [video] }));
    break;
  case 'components':
    // The listings print their result without a status.
    line(
      process.argv.includes('--describe')
        ? { id: 'ffmpeg/libx264', kind: 'videoEncoder', name: 'H.264', plugin: 'ffmpeg', origin: 'bundled', available: true, settings: [] }
        : { components: [{ id: 'ffmpeg/mp4', kind: 'muxer', name: 'MP4', plugin: 'ffmpeg', origin: 'bundled', args: process.argv.slice(2) }] },
    );
    break;
  case 'progress':
    line({ event: 'composition', composition });
    line({ event: 'rendering', frame: 1, total: 30 });
    line({ event: 'wrote', output: { type: 'frame', path: '/work/f.png', frame: 0, time: 0 } });
    line(ok({ outputs: [{ type: 'frame', path: '/work/f.png', frame: 0, time: 0 }], warnings: process.argv.slice(2) }));
    break;
  case 'error':
    line(error('output_exists', 'output already exists: /work/film.mp4', 'pass --overwrite to replace it, or write to another path'));
    process.exitCode = 1;
    break;
  case 'old':
    line(error('usage', "unexpected argument '--stdin-cancel' found"));
    process.exitCode = 2;
    break;
  case 'crash':
    process.stderr.write('thread main panicked: boom\n');
    process.exitCode = 101;
    break;
  case 'wait-cancel':
    line({ event: 'rendering', frame: 1, total: 30 });
    process.stdin.setEncoding('utf8');
    process.stdin.on('data', (chunk) => {
      if (chunk.includes('cancel')) {
        line(error('cancelled', 'export was cancelled'));
        process.exit(1);
      }
    });
    break;
  default:
    process.exitCode = 3;
}
