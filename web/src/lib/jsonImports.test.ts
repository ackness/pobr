import { readdirSync, readFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { describe, expect, it } from 'vitest';

// Node (Playwright specs, web/scripts) loads these modules without Vite and rejects a JSON
// import that lacks the attribute, which stops every browser test from being collected.
const root = join(import.meta.dirname, '..');
const sources = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry =>
  entry.isDirectory() ? sources(join(dir, entry.name)) : /\.tsx?$/.test(entry.name) ? [join(dir, entry.name)] : []);

describe('JSON imports', () => {
  it('declare the json import attribute', () => {
    const missing = sources(root).flatMap(file => readFileSync(file, 'utf8').split('\n')
      .filter(line => /^import\s.+\sfrom\s+'[^']+\.json'/.test(line) && !/with\s*\{\s*type:\s*'json'\s*\}/.test(line))
      .map(line => `${relative(root, file)}: ${line}`));
    expect(missing).toEqual([]);
  });
});
