import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const SRC = fileURLToPath(new URL('../..', import.meta.url));

function cssFiles(dir: string): string[] {
  return readdirSync(dir).flatMap(name => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return cssFiles(path);
    return name.endsWith('.css') ? [path] : [];
  });
}

/** The body of every `@media (prefers-reduced-motion: reduce)` block, braces balanced. */
function reducedMotionBlocks(css: string): string[] {
  const blocks: string[] = [];
  const pattern = /@media\s*\(\s*prefers-reduced-motion:\s*reduce\s*\)\s*\{/g;
  for (let match = pattern.exec(css); match; match = pattern.exec(css)) {
    let depth = 1;
    let end = pattern.lastIndex;
    while (depth > 0 && end < css.length) {
      if (css[end] === '{') depth += 1;
      else if (css[end] === '}') depth -= 1;
      end += 1;
    }
    blocks.push(css.slice(pattern.lastIndex, end - 1));
  }
  return blocks;
}

/** `selector { declarations }` pairs at any depth of a block. */
function rules(block: string): { selector: string; body: string }[] {
  return [...block.replace(/\/\*[\s\S]*?\*\//g, '').matchAll(/([^{};]+)\{([^{}]*)\}/g)]
    .map(([, selector, body]) => ({ selector: selector.trim(), body }));
}

describe('reduced motion CSS', () => {
  const files = cssFiles(SRC).map(path => ({ path, css: readFileSync(path, 'utf8') }));

  it('turns transitions off for every element instead of shortening them', () => {
    // A very short transition is still a transition: with the default `transition-property: all` every
    // changed property on every element starts one. On the passive tree that made each pan frame a
    // multi-second compositor update.
    const universal = files.flatMap(({ path, css }) => reducedMotionBlocks(css)
      .flatMap(rules).filter(rule => rule.selector.split(',').some(part => part.trim().startsWith('*')))
      .map(rule => ({ path, ...rule })));
    expect(universal.length).toBeGreaterThan(0);
    for (const rule of universal) {
      expect(rule.body, rule.path).toMatch(/transition:\s*none\s*!important/);
      expect(rule.body, rule.path).not.toMatch(/transition-duration/);
    }
  });

  it('gives no reduced-motion rule a transition with a duration', () => {
    for (const { path, css } of files) {
      for (const rule of reducedMotionBlocks(css).flatMap(rules)) {
        const transitions = [...rule.body.matchAll(/transition(?:-duration)?:\s*([^;!]+)/g)].map(([, value]) => value.trim());
        for (const value of transitions) {
          // `none`, or only zero durations (a delay alone is allowed: it moves no pixels).
          const durations = value === 'none' ? [] : value.split(',').map(part => part.match(/(-?[\d.]+)m?s/)?.[0] ?? '0s');
          for (const duration of durations) expect(parseFloat(duration), `${path}: ${rule.selector}`).toBe(0);
        }
      }
    }
  });
});
