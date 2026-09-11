// Cloudflare Pages worker; Vite runs the same handler locally.
const API = 'https://www.wegame.com.cn/api/v1/wegame.pallas.poe2.Profile/';
const LIMIT = 4 * 1024 * 1024;

export function shareKey(input) {
  const url = new URL(input.trim());
  if (url.protocol !== 'https:' || url.hostname !== 'www.wegame.com.cn' ||
      url.port || url.username || url.password || url.pathname !== '/helper/poe2/') {
    throw new Error('Expected a WeGame PoE2 share URL.');
  }
  const match = /^#\/share\/([A-Za-z0-9_-]{16,256})\/?$/.exec(url.hash);
  if (!match) throw new Error('Invalid WeGame share code.');
  return match[1];
}

async function readJson(response, limit = LIMIT) {
  if (!response.body) throw new Error('Empty response.');
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let size = 0, text = '';
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > limit) throw new Error('Response is too large.');
      text += decoder.decode(value, { stream: true });
    }
    return JSON.parse(text + decoder.decode());
  } finally {
    await reader.cancel();
  }
}

// Keep calculation fields, excluding account identifiers, character names and URLs.
function itemFields(item, depth = 0) {
  if (!item || typeof item !== 'object' || depth > 8) throw new Error('Invalid WeGame item.');
  const keys = ['baseType', 'typeLine', 'name', 'frameType', 'ilvl', 'inventoryId', 'x',
    'properties', 'implicitMods', 'explicitMods', 'runeMods', 'enchantMods', 'craftedMods',
    'fracturedMods', 'corrupted', 'support', 'socket', 'sockets'];
  const result = Object.fromEntries(keys.filter(k => k in item).map(k => [k, item[k]]));
  if (item.socketedItems) result.socketedItems = item.socketedItems.map(i => itemFields(i, depth + 1));
  return result;
}

export async function fetchShare(url, fetcher = fetch) {
  const code = shareKey(url);
  const methods = ['GetRoleInfo', 'GetEquipments', 'GetTalentTree', 'GetJewels', 'GetSkills'];
  const responses = await Promise.all(methods.map(async method => {
    const response = await fetcher(API + method, {
      // Workers supports manual redirects; the non-2xx check below rejects them.
      method: 'POST', redirect: 'manual', credentials: 'omit',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ share_code: code, area: 0, from_src: 'poe2_helper' }),
      signal: AbortSignal.timeout(15000),
    });
    if (!response.ok) throw new Error(`WeGame ${method}: HTTP ${response.status}.`);
    const data = await readJson(response);
    if (data.result?.error_code !== 0) {
      throw new Error(`WeGame ${method} failed; the share may have expired or been made private.`);
    }
    return data;
  }));
  const [role, equipment, tree, jewels, skills] = responses;
  if (!role.role || !Array.isArray(equipment.equipments) || !tree.talent_tree ||
      !Array.isArray(skills.skills) || typeof jewels.jewel_data !== 'string') {
    throw new Error('Incomplete WeGame share response.');
  }
  return {
    format: 'wegame', version: 1,
    role: { level: role.role.level, class_name: role.role.class_name },
    equipments: equipment.equipments.map(i => itemFields(i)),
    talent_tree: tree.talent_tree,
    jewel_data: jewels.jewel_data,
    skills: skills.skills.map(i => itemFields(i)),
  };
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname !== '/api/import/wegame') return env.ASSETS.fetch(request);
    const headers = { 'cache-control': 'no-store' };
    if (request.method !== 'POST') return new Response('Method not allowed', { status: 405, headers: { ...headers, allow: 'POST' } });
    const origin = request.headers.get('origin');
    if (origin && origin !== url.origin) return new Response('Forbidden', { status: 403, headers });
    let input;
    try {
      input = (await readJson(request, 2048)).url;
      shareKey(input);
    } catch {
      return Response.json({ error: 'Invalid WeGame PoE2 share URL.' }, { status: 400, headers });
    }
    try {
      return Response.json(await fetchShare(input), { headers });
    } catch (error) {
      return Response.json({ error: error instanceof Error ? error.message : 'WeGame import failed.' }, { status: 502, headers });
    }
  },
};
