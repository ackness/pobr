import { describe, expect, it } from 'vitest';
import { nativeDigest } from './dataIntegrity';

const encode = (text: string) => new TextEncoder().encode(text);
const digest = nativeDigest()!;

describe('nativeDigest', () => {
  it('has a native SHA-256 here, and it is the real one', async () => {
    expect(await digest(encode('abc'))).toBe(
      'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
    );
  });
});
