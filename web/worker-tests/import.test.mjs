import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { Miniflare } from 'miniflare';

const url = 'https://www.wegame.com.cn/helper/poe2/#/share/SyntheticShareKey_123456';
const payloads = {
  GetRoleInfo: { role: { level: 59, class_name: 'Deadeye', openid: 'private' } },
  GetEquipments: { equipments: [{ baseType: 'Linen Belt', id: 'private' }] },
  GetTalentTree: { talent_tree: { hashes: [], quest_stats: [] } },
  GetJewels: { jewel_data: '[]' },
  GetSkills: { skills: [] },
};

// Execute the deployed module in workerd; only outbound HTTP is mocked.
// No login, live share or network service is needed by this gate.
for (const redirects of [false, true]) {
  test(`Worker import ${redirects ? 'rejects redirects' : 'returns a calculation bundle'}`, async (t) => {
    const seen = [];
    const mf = new Miniflare({
      modules: true,
      scriptPath: fileURLToPath(new URL('../public/_worker.js', import.meta.url)),
      compatibilityDate: '2026-07-30',
      cf: false,
      outboundService: async (request) => {
        const target = new URL(request.url);
        assert.equal(target.origin, 'https://www.wegame.com.cn');
        const method = target.pathname.replace('/api/v1/wegame.pallas.poe2.Profile/', '');
        assert.ok(Object.hasOwn(payloads, method), target.pathname);
        assert.equal(request.method, 'POST');
        assert.deepEqual(await request.json(), {
          share_code: 'SyntheticShareKey_123456', area: 0, from_src: 'poe2_helper',
        });
        seen.push(method);
        return redirects
          ? new Response(null, { status: 302, headers: { location: 'https://example.com/unexpected' } })
          : Response.json({ result: { error_code: 0 }, ...payloads[method] });
      },
    });
    t.after(() => mf.dispose());
    const response = await mf.dispatchFetch('https://pobr.test/api/import/wegame', {
      method: 'POST',
      headers: { 'content-type': 'application/json', origin: 'https://pobr.test' },
      body: JSON.stringify({ url }),
    });
    const body = await response.json();
    assert.equal(response.status, redirects ? 502 : 200, JSON.stringify(body));
    assert.equal(response.headers.get('cache-control'), 'no-store');
    if (redirects) {
      assert.match(body.error, /HTTP 302/);
    } else {
      assert.equal(body.format, 'wegame');
      assert.equal(body.version, 1);
      assert.deepEqual(body.role, { level: 59, class_name: 'Deadeye' });
      assert.deepEqual(body.equipments, [{ baseType: 'Linen Belt' }]);
      assert.deepEqual(body.skills, []);
      assert.equal(JSON.stringify(body).includes('private'), false);
      assert.deepEqual(seen.sort(), Object.keys(payloads).sort());
    }
  });
}

test('Worker loads official leagues and rejects arbitrary upstream realms', async (t) => {
  let calls = 0;
  const mf = new Miniflare({
    modules: true, cf: false, compatibilityDate: '2026-07-30',
    scriptPath: fileURLToPath(new URL('../public/_worker.js', import.meta.url)),
    outboundService: async (request) => {
      calls += 1;
      assert.equal(request.url, 'https://www.pathofexile.com/api/trade2/data/leagues');
      assert.equal(request.headers.get('user-agent'), 'PoBR (+https://github.com/ackness/pobr)');
      return Response.json({ result: [
        { id: 'Future League', realm: 'poe2' }, { id: 'Standard', realm: 'poe2' },
        { id: 'Wrong Game', realm: 'pc' },
      ] });
    },
  });
  t.after(() => mf.dispose());
  const response = await mf.dispatchFetch('https://pobr.test/api/trade/leagues?realm=intl');
  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { leagues: ['Future League', 'Standard'] });
  assert.equal(response.headers.get('cache-control'), 'public, max-age=600');
  const invalid = await mf.dispatchFetch('https://pobr.test/api/trade/leagues?realm=https://example.com');
  assert.equal(invalid.status, 400);
  assert.equal(calls, 1);
});

for (const realm of ['intl', 'cn']) test(`Worker searches ${realm} stock with category/budget and returns only calculation fields`, async (t) => {
  const ids = Array.from({ length: 25 }, (_, i) => i.toString(16).padStart(64, '0'));
  let searches = 0, fetches = 0;
  const mf = new Miniflare({ modules: true, cf: false, compatibilityDate: '2026-07-30',
    scriptPath: fileURLToPath(new URL('../public/_worker.js', import.meta.url)),
    outboundService: async request => {
      const url = new URL(request.url);
      assert.equal(url.origin, realm === 'cn' ? 'https://poe.game.qq.com' : 'https://www.pathofexile.com');
      if (request.method === 'POST') {
        searches++;
        assert.equal(url.pathname, '/api/trade2/search/poe2/Future%20League');
        const body = await request.json();
        assert.equal(body.query.status.option, realm === 'cn' ? 'any' : 'online');
        assert.equal(body.query.filters.type_filters.filters.category.option, 'armour.quiver');
        assert.deepEqual(body.query.filters.trade_filters.filters.price, { option: 'exalted', max: 100 });
        assert.deepEqual(body.query.filters.req_filters.filters.lvl, { max: 80 });
        assert.equal(body.query.stats[0].filters[0].id, 'explicit.stat_123');
        assert.equal(body.sort['statgroup.0'], 'desc');
        return Response.json({ id: 'test-query', total: 200, result: ids });
      }
      fetches++;
      const batch = url.pathname.split('/').pop().split(',');
      assert.equal(batch.length, 10);
      return Response.json({ result: batch.map(id => ({ id, listing: {
        account: { name: 'SyntheticSeller' }, whisper: 'private-message', price: { amount: 10, currency: 'exalted' },
      }, item: { name: 'Synthetic Quiver', baseType: 'Broadhead Quiver', frameType: 2,
        explicitMods: [{ description: '20% increased Projectile Damage' }], account: 'private' } })) });
    },
  });
  t.after(() => mf.dispose());
  const send = body => mf.dispatchFetch('https://pobr.test/api/trade/search', { method: 'POST',
    headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) });
  const response = await send({ realm, league: 'Future League', category: 'armour.quiver',
    weighted: [{ id: 'explicit.stat_123', weight: 2 }], price: { max: 100, currency: 'exalted' }, maxLevel: 80 });
  const body = await response.json();
  assert.equal(response.status, 200, JSON.stringify(body));
  assert.equal(JSON.parse(new URL(body.listings[0].url).searchParams.get('q')).query.filters.trade_filters.filters.account.input, 'SyntheticSeller');
  assert.equal(body.sampled, 20); assert.equal(body.listings.length, 20); assert.equal(body.total, 200);
  assert.equal(JSON.stringify(body).includes('private'), false);
  assert.equal(response.headers.get('cache-control'), 'no-store');
  assert.equal((await send({ realm: 'intl', league: 'Future League', category: '' })).status, 400);
  assert.equal(searches, 1); assert.equal(fetches, 2);
});

test('Worker preserves rate-limit errors without retrying or fetching listings', async t => {
  let calls = 0;
  const mf = new Miniflare({ modules: true, cf: false, compatibilityDate: '2026-07-30',
    scriptPath: fileURLToPath(new URL('../public/_worker.js', import.meta.url)),
    outboundService: async () => { calls++; return new Response('Limited', { status: 429, headers: { 'retry-after': '60' } }); },
  });
  t.after(() => mf.dispose());
  const response = await mf.dispatchFetch('https://pobr.test/api/trade/search', { method: 'POST',
    body: JSON.stringify({ realm: 'intl', league: 'Standard', category: 'gem', gem: { name: 'Fireball', level: 20, quality: 20 } }) });
  assert.equal(response.status, 429); assert.equal((await response.json()).retry_after, '60'); assert.equal(calls, 1);
});

test('anonymous complexity limit uses one explicit budget fallback and filters level requirements', async t => {
  let searches = 0;
  const ids = ['a'.repeat(64), 'b'.repeat(64)];
  const mf = new Miniflare({ modules: true, cf: false, compatibilityDate: '2026-07-30',
    scriptPath: fileURLToPath(new URL('../public/_worker.js', import.meta.url)),
    outboundService: async request => {
      if (request.method === 'POST') {
        searches++;
        if (searches === 1) return Response.json({ error: { code: 2, message: 'Query is too complex' } }, { status: 400 });
        const query = await request.json();
        assert.equal(searches, 2);
        assert.deepEqual(query.query.stats, [{ type: 'and', filters: [] }]);
        assert.equal(query.query.filters.type_filters.filters.category.option, 'accessory.amulet');
        assert.equal(query.query.filters.trade_filters.filters.price.max, 50);
        assert.equal(query.sort.price, 'desc');
        return Response.json({ id: 'simple', result: ids, total: 100 });
      }
      return Response.json({ result: ids.map((id, index) => ({ id,
        listing: { price: { amount: 40, currency: 'exalted' } },
        item: { baseType: 'Amber Amulet', requirements: [{ name: 'Level', values: [[String(50 + index * 40), 0]] }] },
      })) });
    },
  });
  t.after(() => mf.dispose());
  const response = await mf.dispatchFetch('https://pobr.test/api/trade/search', { method: 'POST', body: JSON.stringify({
    realm: 'intl', league: 'Standard', category: 'accessory.amulet', maxLevel: 60,
    weighted: [{ id: 'explicit.stat_123', weight: 1 }], price: { max: 50 },
  }) });
  const data = await response.json();
  assert.equal(response.status, 200, JSON.stringify(data));
  assert.equal(data.search_mode, 'budget'); assert.equal(data.listings.length, 1); assert.equal(searches, 2);
});

test('Pages deployment routes every API to the Worker while static assets bypass it', async () => {
  const { readFile } = await import('node:fs/promises');
  const manifest = JSON.parse(await readFile(new URL('../public/_routes.json', import.meta.url), 'utf8'));
  const matches = (pattern, path) => pattern.endsWith('*') ? path.startsWith(pattern.slice(0, -1)) : path === pattern;
  const routed = path => manifest.include.some(pattern => matches(pattern, path)) && !manifest.exclude.some(pattern => matches(pattern, path));
  // Workerd unit tests invoke fetch directly, so this separately guards the Pages routing boundary.
  for (const path of ['/api/import/wegame', '/api/trade/leagues', '/api/trade/search']) assert.equal(routed(path), true, path);
  for (const path of ['/', '/assets/app.js', '/data/manifest.json']) assert.equal(routed(path), false, path);
});
