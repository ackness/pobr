import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const readCss = (name: string) => readFileSync(fileURLToPath(new URL(`./${name}`, import.meta.url)), 'utf8');

describe('responsive layout CSS invariants', () => {
  it('declares shell-page inline-size containment on .shell-main so page container queries stay live', () => {
    const styles = readCss('styles.css');
    expect(styles).toMatch(/\.shell-main\s*\{[^}]*container:\s*shell-page\s*\/\s*inline-size/);
  });

  it('places short-viewport chrome compression after desktop container padding and excludes full-bleed tree', () => {
    const styles = readCss('styles.css');
    const shortViewport = styles.indexOf('@media (max-height: 820px)');
    const ultrawidePadding = styles.indexOf('@container shell-container (min-width: 130rem)');
    expect(shortViewport).toBeGreaterThan(ultrawidePadding);
    expect(styles.slice(shortViewport)).toMatch(/\.shell-main:not\(\.shell-page-tree\)\s*\{[^}]*padding-top:/);
  });

  it('inherits --tree-top-bar-bottom on both .asc-hint and .tree-edit-error so wrapped bars never overlap alerts', () => {
    const tree = readCss('pages-tree.css');
    const match = /([^{}]*--tree-top-bar-bottom:\s*inherit;[^}]*)/.exec(tree);
    expect(match).not.toBeNull();
    const ruleStart = tree.lastIndexOf('\n', match!.index - 1);
    const rule = tree.slice(ruleStart, match!.index + match![0].length);
    expect(rule).toContain('.asc-hint');
    expect(rule).toContain('.tree-edit-error');
  });

  it('lays out flasks and charms as 2+3 without mid-word character breaks on paper-doll labels', () => {
    const pages = readCss('pages.css');
    expect(pages).toMatch(/\.utility-row\s*\{[^}]*grid-template-columns:\s*repeat\(6,\s*minmax\(0,\s*1fr\)\)/);
    expect(pages).toMatch(/\.utility-row\.doll-utility\s*>\s*:nth-child\(-n\s*\+\s*2\)\s*\{\s*grid-column:\s*span\s+3;/);
    expect(pages).toMatch(/\.utility-row\.doll-utility\s*>\s*:nth-child\(n\s*\+\s*3\)\s*\{\s*grid-column:\s*span\s+2;/);
    expect(pages).toMatch(/\.utility-row\.doll-utility\s*>\s*\*\s*\{\s*grid-column:\s*auto;/);
    expect(pages).not.toMatch(/\.doll-item-(?:name|base)\s*\{[^}]*overflow-wrap:\s*anywhere/);
  });

  it('keeps the super-ultrawide three-column skills split above the 2560px workspace contract', () => {
    const skills = readCss('pages-skills.css');
    const blocks: { minRem: number; body: string }[] = [];
    const pattern = /@container\s+skills-page\s*\(\s*min-width:\s*(\d+)rem\s*\)\s*\{/g;
    for (let match = pattern.exec(skills); match; match = pattern.exec(skills)) {
      let depth = 1;
      let end = pattern.lastIndex;
      while (depth > 0 && end < skills.length) {
        if (skills[end] === '{') depth += 1;
        else if (skills[end] === '}') depth -= 1;
        end += 1;
      }
      blocks.push({ minRem: Number(match[1]), body: skills.slice(pattern.lastIndex, end - 1) });
    }
    const split = blocks.find(block => block.body.includes('.skill-support-section'));
    expect(split).toBeDefined();
    // At 2560px the skills content column caps at 132rem; the 3-column split must start strictly wider.
    expect(split!.minRem).toBeGreaterThan(132);
  });
});
