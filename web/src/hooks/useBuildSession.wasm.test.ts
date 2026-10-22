/** Actual editing-state conversion against the built WASM, not a second DTO projection. */
import { beforeAll, describe, expect, test } from 'vitest';
import { readFile } from 'node:fs/promises';
import { env } from 'node:process';
import { deflateSync } from 'node:zlib';
import { materialize, paramsFromDecoded, toRequest, type SavedSession } from './useBuildSession';
import { switchWeapons } from '../lib/weaponSets';
import { EXPECTED_SCHEMA_VERSION, type BuildJson, type CalculateBuildRequest, type CalculateBuildResponse, type SkillMetricsResponse } from '../api/types';

const gem = (id: string) => `<Gem skillId="${id}" level="20" quality="0" enabled="true"/>`;
const source = `<PathOfBuilding2>
<Build level="90" className="Ranger" mainSocketGroup="2"/>
<Skills>
<Skill enabled="false" includeInFullDPS="true">${gem('FireballPlayer')}</Skill>
<Skill enabled="true" set1="true" set2="false" includeInFullDPS="true">${gem('IceShotPlayer')}</Skill>
<Skill enabled="true" set1="false" set2="true" includeInFullDPS="true">${gem('IceShotPlayer')}</Skill>
<Skill enabled="true" set1="false" set2="true">${gem('BlinkReservationPlayer')}</Skill>
<Skill enabled="true" set1="false" set2="true">${gem('EternalRagePlayer')}</Skill>
</Skills>
<Items activeItemSet="1">
<Item id="1">Rarity: RARE
First Bow
Crude Bow
Implicits: 0
Adds 10 to 20 Physical Damage
</Item>
<Item id="2">Rarity: RARE
Second Bow
Crude Bow
Implicits: 0
Adds 100 to 200 Physical Damage
</Item>
<ItemSet id="1"><Slot name="Weapon 1" itemId="1"/><Slot name="Weapon 1 Swap" itemId="2"/></ItemSet>
</Items>
</PathOfBuilding2>`;
const code = deflateSync(source).toString('base64url');
const stat = (response: CalculateBuildResponse, id: string) => response.stats.find(row => row.id === id)?.value;

// Run after build-wasm + sync-data. The ordinary frontend unit gate needs no
// generated assets; the WASM contract gate explicitly opts into this suite.
describe.skipIf(env.POBR_WASM_CONTRACT !== '1')('real TS request to WASM weapon contexts', () => {
  let calculate: (request: CalculateBuildRequest) => CalculateBuildResponse;
  let decode: (code: string) => BuildJson;
  let encode: (request: CalculateBuildRequest) => string;
  let metrics: (request: CalculateBuildRequest) => SkillMetricsResponse;
  let state: SavedSession['state'];

  beforeAll(async () => {
    const modulePath = '../wasm/pkg/pobr_wasm.js';
    const wasm = await import(/* @vite-ignore */ modulePath);
    wasm.initSync({ module: await readFile(new URL('../wasm/pkg/pobr_wasm_bg.wasm', import.meta.url)) });
    expect(wasm.schemaVersion()).toBe(EXPECTED_SCHEMA_VERSION);
    const root = new URL('../../public/data/', import.meta.url);
    const manifest = JSON.parse(await readFile(new URL('manifest.json', root), 'utf8')) as { version: string; files: string[] };
    expect(manifest.version).toBe((await readFile(new URL('../../../data/CURRENT', import.meta.url), 'utf8')).trim());
    for (const file of manifest.files) wasm.stageDataFile(file, await readFile(new URL(`${manifest.version}/${file}`, root), 'utf8'));
    wasm.initStagedData();
    calculate = request => JSON.parse(wasm.calculateBuildJson(JSON.stringify(request)));
    decode = code => JSON.parse(wasm.decodeBuildJson(code));
    encode = request => wasm.encodeBuildJson(JSON.stringify(request));
    metrics = request => JSON.parse(wasm.skillMetricsJson(JSON.stringify(request)));
    const decoded = decode(code);
    state = {
      ...materialize(decoded), pobCode: code, character: decoded.character,
      allocatedNodes: decoded.tree.allocated_nodes, attributeChoices: decoded.tree.attribute_choices,
      annotations: {}, params: { ...paramsFromDecoded(decoded), main_socket_group: decoded.main_socket_group ?? undefined },
    };
  }, 120_000);

  test('actual materialization preserves all-set reservation, attack contexts, Full DPS and export', () => {
    const request = toRequest(state);
    expect(request.pob_code).toBeUndefined();
    expect(request.socket_groups?.map(group => group.enabled)).toEqual([false, true, true, true, true]);
    expect(request.socket_groups?.every(group => !('full_dps_enabled' in group))).toBe(true);
    const first = calculate(request);
    const direct = calculate({ pob_code: code, full_dps: true });
    expect(first.item_errors).toEqual([]);
    expect(first.stats).toEqual(direct.stats);
    expect(first.main_skill).toEqual(direct.main_skill);
    expect(first.full_dps).toEqual(direct.full_dps);
    expect(stat(first, 'SpiritReserved')).toBe(60);
    expect(first.main_skill?.group_index).toBe(1);
    expect(metrics(request).skills.map(row => row.group_index)).toEqual([1]);
    expect(first.full_dps?.skills.map(row => row.group_index)).toEqual([1, 2]);

    const switched = switchWeapons({ ...state, params: { ...state.params, main_socket_group: 2 } }, 2);
    const secondRequest = toRequest(switched);
    const second = calculate(secondRequest);
    expect(second.item_errors).toEqual([]);
    expect(stat(second, 'SpiritReserved')).toBe(160);
    expect(second.main_skill?.group_index).toBe(2);
    expect(stat(second, 'TotalDPS')).toBeGreaterThan(stat(first, 'TotalDPS')!);
    expect(metrics(secondRequest).skills.map(row => row.group_index)).toEqual([2, 3, 3, 4]);
    expect(first.full_dps).toEqual(second.full_dps);
    expect(first.full_dps?.skills[0].hit_dps).toBeCloseTo(stat(first, 'TotalDPS')!, 8);
    expect(first.full_dps?.skills[1].hit_dps).toBeCloseTo(stat(second, 'TotalDPS')!, 8);

    // Export the real request directly: no restoration of pre-mask metadata.
    const exported = encode(secondRequest);
    const restored = decode(exported);
    expect(restored.socket_groups.map(group => group.enabled)).toEqual([false, true, true, true, true]);
    expect(restored.socket_groups.map(group => group.weapon_set)).toEqual(state.socketGroups.map(group => group.weapon_set));
    expect(restored.main_socket_group).toBe(2);
    expect(restored.weapon_swap?.active).toBe(2);
    expect(calculate({ pob_code: exported, full_dps: true }).stats).toEqual(second.stats);

    const disabled = calculate(toRequest({ ...state, socketGroups: state.socketGroups.map((group, index) =>
      index === 3 ? { ...group, enabled: false } : group) }));
    expect(stat(disabled, 'SpiritReserved')).toBe(0);
    expect(state.socketGroups[3].enabled).toBe(true);
  });

  test('legacy masked Full DPS requests remain readable without changing the inspected skill', () => {
    const current = toRequest(state);
    const legacy: CalculateBuildRequest = { ...current, socket_groups: current.socket_groups!.map(group => ({ ...group,
      ...(group.include_in_full_dps ? { full_dps_enabled: group.enabled } : {}),
      enabled: group.enabled && (!group.weapon_set || group.weapon_set === 1),
    })) };
    const expected = calculate(current);
    const actual = calculate(legacy);
    expect(actual.full_dps).toEqual(expected.full_dps);
    expect(actual.main_skill).toEqual(expected.main_skill);
    expect(stat(actual, 'SpiritReserved')).toBe(0); // Legacy callers discarded Blink's enabled state.
  });
});
