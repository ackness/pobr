import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

describe('app version', () => {
  it('is the workspace version from Cargo.toml', () => {
    const cargo = readFileSync(new URL('../../../Cargo.toml', import.meta.url), 'utf8');
    const version = cargo.match(/\[workspace\.package\]([\s\S]*?)(?=\n\[|$)/)?.[1]?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
    expect(version).toBeTruthy();
    expect(__POBR_APP_VERSION__).toBe(version);
  });
});
