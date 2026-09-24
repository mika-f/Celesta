// Loads a `.celesta.json` project file for read access from TypeScript. The
// canonical schema lives in Rust (`celesta-project`); the types under
// `./generated` are generated from it by ts-rs, not hand-maintained here, so
// this stays a thin JSON.parse wrapper rather than a second validator. A
// project loaded this way is expected to already be valid: run it through
// `celesta-project`'s `Project::load` (the editor and exporter always do) if
// that has not already happened.

import * as fs from 'node:fs';

import type { Project } from './generated/Project';

export function loadProjectFromString(json: string): Project {
  return JSON.parse(json) as Project;
}

export function loadProject(path: string): Project {
  return loadProjectFromString(fs.readFileSync(path, 'utf8'));
}
