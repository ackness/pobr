import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { toCdnUrl } from 'pathofexile-dat/bundles.js';

test('official PoE2 0.x patches do not fall through to the PoE1 CDN', () => {
  assert.equal(toCdnUrl('0.5.5.4.2', '_.index.bin'),
    'https://patch-poe2.poecdn.com/0.5.5.4.2/Bundles2/_.index.bin');
  assert.equal(toCdnUrl('4.5.5.3', '_.index.bin'),
    'https://patch-poe2.poecdn.com/4.5.5.3/Bundles2/_.index.bin');
  assert.equal(toCdnUrl('3.26.0.1', '_.index.bin'),
    'https://patch.poecdn.com/3.26.0.1/Bundles2/_.index.bin');
});
