#!/usr/bin/env node
'use strict';

const args = process.argv.slice(2);
const flag = args.length === 1 ? args[0] : undefined;

switch (flag) {
  case '-h':
  case '--help':
    console.log(`celesta-react-render — internal React composition evaluator

Celesta launches this helper to evaluate React compositions, not to write images
or video. It exchanges JSON messages over stdin/stdout.

For command-line rendering, use celesta-exporter (Celesta-export in desktop
packages). Run celesta-exporter --help for export options.

Options:
  -h, --help     Print help
  -V, --version  Print version`);
    break;
  case '-V':
  case '--version':
    console.log(`celesta-react-render ${require('../package.json').version}`);
    break;
  default:
    require('../dist/cli.js');
}
