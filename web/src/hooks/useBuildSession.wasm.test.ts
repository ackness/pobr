/** Actual editing-state conversion against the built WASM, not a second DTO projection. */
import { beforeAll, describe, expect, test } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { env } from 'node:process';
import { deflateSync } from 'node:zlib';
import { backfillFromDecoded, materialize, paramsFromDecoded, parseSaved, toRequest, type SavedSession } from './useBuildSession';
import { replaceUtility } from '../lib/utilityItems';
import { tradeItemVariant } from '../lib/tradeOptimizer';
import { switchWeapons } from '../lib/weaponSets';
import { buildPassiveGraph, classStartSkill, implicitClassStarts } from '../lib/passiveGraph';
import { passivePointSummary, passivePointSummaryTarget } from '../components/tree/treePresentation';
import { EXPECTED_SCHEMA_VERSION, type BuildJson, type CalculateBuildRequest, type CalculateBuildResponse, type LineUsageResponse, type PassiveNode, type PassiveTreeMeta, type SkillMetricsResponse, type TreeEditResponse } from '../api/types';

const gem = (id: string) => `<Gem skillId="${id}" level="20" quality="0" enabled="true"/>`;
const source = `<PathOfBuilding2>
<Build level="90" className="Ranger" mainSocketGroup="2"/>
<Skills>
<Skill enabled="false" includeInFullDPS="true">${gem('FireballPlayer')}</Skill>
<Skill enabled="true" set1="true" set2="false" includeInFullDPS="true">${gem('IceShotPlayer')}</Skill>
<Skill enabled="true" set1="false" set2="true" includeInFullDPS="true">${gem('IceShotPlayer')}</Skill>
<Skill enabled="true" set1="false" set2="true">${gem('BlinkReservationPlayer')}</Skill>
<Skill enabled="true" set1="false" set2="true">${gem('AlchemistsBoonPlayer')}</Skill>
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
  let encode: (request: CalculateBuildRequest & { base_code?: string }) => string;
  let metrics: (request: CalculateBuildRequest) => SkillMetricsResponse;
  let usage: (request: CalculateBuildRequest) => LineUsageResponse;
  let preview: (input: object) => TreeEditResponse;
  let state: SavedSession['state'];

  beforeAll(async () => {
    const modulePath = '../wasm/pkg/pobr_wasm.js';
    const wasm = await import(/* @vite-ignore */ modulePath);
    wasm.initSync({ module: await readFile(new URL('../wasm/pkg/pobr_wasm_bg.wasm', import.meta.url)) });
    expect(wasm.schemaVersion()).toBe(EXPECTED_SCHEMA_VERSION);
    const root = new URL('../../public/data/', import.meta.url);
    const manifest = JSON.parse(await readFile(new URL('manifest.json', root), 'utf8')) as { version: string; files: string[] };
    expect(manifest.version).toBe(env.POBR_DATA_VERSION ?? (await readFile(new URL('../../../data/CURRENT', import.meta.url), 'utf8')).trim());
    for (const file of manifest.files) wasm.stageDataFile(file, await readFile(new URL(`${manifest.version}/${file}`, root), 'utf8'));
    wasm.initStagedDataVerified();
    calculate = request => JSON.parse(wasm.calculateBuildJson(JSON.stringify(request)));
    decode = code => JSON.parse(wasm.decodeBuildJson(code));
    encode = request => wasm.encodeBuildJson(JSON.stringify(request));
    metrics = request => JSON.parse(wasm.skillMetricsJson(JSON.stringify(request)));
    usage = request => JSON.parse(wasm.lineUsageJson(JSON.stringify(request)));
    preview = input => JSON.parse(wasm.previewTreeEditJson(JSON.stringify(input)));
    const decoded = decode(code);
    state = {
      ...materialize(decoded), pobCode: code, character: decoded.character,
      allocatedNodes: decoded.tree.allocated_nodes, attributeChoices: decoded.tree.attribute_choices,
      annotations: {}, params: { ...paramsFromDecoded(decoded), main_socket_group: decoded.main_socket_group ?? undefined },
    };
  }, 120_000);

  test('an empty level-90 tree gets its current budget from an exclusive neighbor preview', async () => {
    const request: CalculateBuildRequest = { character: { class_name: 'Witch', level: 90 }, allocated_nodes: [] };
    const effects = calculate(request).tree_effects!;
    const manifest = JSON.parse(await readFile(new URL('../../public/data/manifest.json', import.meta.url), 'utf8')) as { version: string };
    const nodes = JSON.parse(await readFile(new URL(`../../public/data/${manifest.version}/base/passive_tree.json`, import.meta.url), 'utf8')) as PassiveNode[];
    const root = classStartSkill(effects, 'Witch')!;
    const target = passivePointSummaryTarget(nodes, new Set(), implicitClassStarts(effects), buildPassiveGraph(nodes).get(root) ?? []);
    expect(target.node).not.toBeNull();
    const response = preview({ request, ...target, action: 'allocate' });
    expect(response.valid).toBe(true);
    expect(response.result?.allocation.shared).toEqual([]);
    expect(passivePointSummary(nodes, [], undefined, effects, response.result)).toEqual({ used: 0, max: 113, weapon: [0, 0] });

    const meta = JSON.parse(await readFile(new URL(`../../public/data/${manifest.version}/base/passive_tree_meta.json`, import.meta.url), 'utf8')) as PassiveTreeMeta;
    const ascendancy = meta.classes.find(entry => entry.name === 'Witch')!.ascendancies![0];
    const ascRoot = nodes.find(node => node.kind === 'ascendancy_start' && node.ascendancy_id === ascendancy.id)!;
    const cases: CalculateBuildRequest[] = [
      { ...request, allocated_nodes: [root] },
      { ...request, character: { ...request.character!, ascendancy_name: ascendancy.name }, allocated_nodes: [root, ascRoot.skill] },
      { ...request, weapon_swap: { active: 1, alternate_items: [], exclusive_nodes: [[], [target.node!]] } },
      { ...request, allocated_nodes: [target.node!], weapon_swap: { active: 2, alternate_items: [], exclusive_nodes: [[], [target.node!]] } },
    ];
    const asc = preview({ request: cases[1], node: buildPassiveGraph(nodes).get(ascRoot.skill)![0], mode: 0, action: 'allocate' });
    expect(asc.valid).toBe(true);
    cases.push({ ...cases[1], allocated_nodes: asc.result!.allocation.shared });
    for (const current of cases) {
      const target = passivePointSummaryTarget(nodes, new Set(current.allocated_nodes), implicitClassStarts(effects),
        buildPassiveGraph(nodes).get(root) ?? [], current.weapon_swap);
      expect(target.node).not.toBeNull();
      const response = preview({ request: current, ...target, action: 'allocate' });
      expect(response.valid, JSON.stringify(current)).toBe(true);
      expect(passivePointSummary(nodes, current.allocated_nodes!, current.weapon_swap, effects, response.result).max).toBe(113);
      if (current.weapon_swap || current.allocated_nodes!.includes(asc.result!.added[0])) {
        expect(response.result!.added).toEqual([]);
        expect(response.result!.promoted).toEqual([]);
      }
    }
  });

  test('equipped unselected utilities survive the real edit, save, reload, request and XML workflow', async () => {
    const xml = await readFile(new URL('../../../apps/pobr-wasm/tests/fixtures/utility-selection.xml', import.meta.url), 'utf8');
    const importedCode = deflateSync(xml).toString('base64url');
    const decoded = decode(importedCode);
    const imported: SavedSession['state'] = {
      ...materialize(decoded), pobCode: importedCode, character: decoded.character,
      allocatedNodes: decoded.tree.allocated_nodes, attributeChoices: decoded.tree.attribute_choices,
      annotations: {}, params: paramsFromDecoded(decoded),
    };
    const restore = (state: SavedSession['state']) => {
      const saved = parseSaved(JSON.stringify({ version: 1, state, notes: '' }));
      expect(saved).not.toBeNull();
      return backfillFromDecoded(saved!.state, decoded);
    };
    const restored = restore(imported);
    expect(restored.flasks.find(item => item.slot === 'Flask 1')?.active).toBe(false);
    expect(restored.flasks.find(item => item.slot === 'Charm 1')?.active).toBe(false);
    const request = { ...toRequest(restored), breakdowns: [] };
    const calculated = calculate(request);
    expect(calculated).toEqual(calculate({ pob_code: importedCode, breakdowns: [] }));
    expect(stat(calculated, 'Flask1Recovery')).toBeCloseTo(506);
    expect(stat(calculated, 'Flask1ManaRecovery')).toBeCloseTo(506);
    expect(stat(calculated, 'FireResist')).toBe(-10);
    expect(usage(request).usage.lines.flatMap(line => line.modifiers))
      .toContainEqual(expect.objectContaining({ name: 'LifeFlaskAppliesToMana', status: 'applied' }));
    const life = restored.flasks.find(item => item.slot === 'Flask 1')!;
    const edited = restore({ ...restored, flasks: replaceUtility(restored.flasks, life.slot, life.text.replace('+40%', '+12%')) });
    expect(edited.flasks.find(item => item.slot === life.slot)?.active).toBe(false);
    expect(stat(calculate(toRequest(edited)), 'FireResist')).toBe(-38);
    const traded = restore({ ...edited, flasks: tradeItemVariant(toRequest(edited), life.slot, life.text).flasks! });
    expect(calculate({ ...toRequest(traded), breakdowns: [] })).toEqual(calculated);
    const withoutReverie = { ...restored, items: restored.items.filter(item => item.slot !== 'bodyarmour') };
    const unique = { ...restored, flasks: replaceUtility(restored.flasks, life.slot, life.text.replace('Rarity: NORMAL', 'Rarity: UNIQUE\nFixture Unique')) };
    const cleared = restore({ ...restored, flasks: [] });
    const selected = { ...withoutReverie, flasks: withoutReverie.flasks.map(item => ({ ...item, active: true })) };
    expect(stat(calculate(toRequest(withoutReverie)), 'FireResist')).toBe(-50);
    expect(stat(calculate(toRequest(unique)), 'FireResist')).toBe(-50);
    expect(stat(calculate(toRequest(selected)), 'FireResist')).toBe(15);
    expect(cleared.flasks).toEqual([]);
    for (const snapshot of [restored, edited, traded, withoutReverie, unique, selected, cleared]) {
      const current = { ...toRequest(restore(snapshot)), breakdowns: [] };
      for (const base_code of [undefined, importedCode]) {
        const exported = encode({ ...current, base_code });
        const reopened = decode(exported);
        const normalized = (items: typeof current.flasks) => items?.map(item => ({ ...item, text: item.text.trim() })).sort((a, b) => a.slot.localeCompare(b.slot));
        expect(normalized(reopened.items.flasks)).toEqual(normalized(current.flasks));
        expect(calculate({ pob_code: exported, breakdowns: [] })).toEqual(calculate(current));
        expect(calculate({ ...current, pob_code: importedCode })).toEqual(calculate(current));
      }
    }
  });

  test('worked build codes agree with real frontend requests and optional native CLI', async () => {
    const directory = new URL('../../../examples/wasm-benchmark/builds/', import.meta.url);
    const builds = (await readdir(directory, { withFileTypes: true })).filter(entry => entry.isDirectory()).sort((a, b) => a.name.localeCompare(b.name));
    expect(builds.length).toBeGreaterThan(0);
    for (const entry of builds) {
      const path = new URL(`${entry.name}/code.txt`, directory);
      const original = (await readFile(path, 'utf8')).trim();
      const decoded = decode(original);
      const edited: SavedSession['state'] = {
        ...materialize(decoded), pobCode: original, character: decoded.character,
        allocatedNodes: decoded.tree.allocated_nodes, attributeChoices: decoded.tree.attribute_choices,
        annotations: {}, params: { ...paramsFromDecoded(decoded), main_socket_group: decoded.main_socket_group ?? undefined },
      };
      const request = { ...toRequest(edited), full_dps: true, breakdowns: [] };
      const actual = calculate(request);
      const direct = calculate({ pob_code: original, full_dps: true, breakdowns: [] });
      expect(actual, entry.name).toEqual(direct);
      for (const base_code of [undefined, original]) {
        const savedCode = encode({ ...request, base_code });
        expect(calculate({ pob_code: savedCode, full_dps: true, breakdowns: [] }), `${entry.name}: save/reopen`).toEqual(actual);
        expect(decode(savedCode).items.equipment_jewels?.map(jewel => ({ ...jewel, text: jewel.text.trim() })))
          .toEqual(decoded.items.equipment_jewels?.map(jewel => ({ ...jewel, text: jewel.text.trim() })));
      }
      // Explicit opt-in uses an already built binary; this suite never compiles one.
      if (env.POBR_CLI_CONTRACT_BIN) {
        const dataRoot = env.POBR_DATA_ROOT ?? fileURLToPath(new URL('../../../data/', import.meta.url));
        const native = JSON.parse(execFileSync(env.POBR_CLI_CONTRACT_BIN, ['calculate-build', '--file', fileURLToPath(path),
          '--data-dir', `${dataRoot}/${actual.provenance!.data_version}`, '--full-dps', '--breakdown', 'Life', '--json'],
        { encoding: 'utf8', timeout: 60_000, maxBuffer: 16 * 1024 * 1024 }));
        expect(native.provenance, entry.name).toEqual(actual.provenance);
        expect(native.main_skill, entry.name).toEqual(actual.main_skill);
        // CLI omits empty skill lists; WASM exposes the empty list in its Full DPS object.
        expect(native.output.full_dps_skills ?? [], entry.name).toEqual(actual.full_dps?.skills ?? []);
        expect(native.output.full_dps, entry.name).toBe(actual.full_dps?.dps);
        expect(native.output.full_dot_dps, entry.name).toBe(actual.full_dps?.dot_dps);
        for (const [field, id] of [['life', 'Life'], ['mana', 'Mana'], ['energy_shield', 'EnergyShield'],
          ['armour', 'Armour'], ['evasion', 'Evasion'], ['dps', 'TotalDPS'], ['mana_cost', 'ManaCost']] as const) {
          expect(native.output[field], `${entry.name}: ${id}`).toBe(stat(actual, id));
        }
      }
    }
  }, 120_000);

  test('actual materialization preserves all-set reservation, attack contexts, Full DPS and export', () => {
    const request = toRequest(state);
    expect(request.pob_code).toBeUndefined();
    expect(request.socket_groups?.map(group => group.enabled)).toEqual([false, true, true, true, true]);
    expect(request.socket_groups?.map(group => group.weapon_set ?? null)).toEqual([null, 1, 2, 2, 2]);
    expect(request.weapon_swap?.active).toBe(1);
    expect(request.socket_groups?.every(group => !('full_dps_enabled' in group))).toBe(true);
    const first = calculate(request);
    const direct = calculate({ pob_code: code, full_dps: true });
    expect(first.item_errors).toEqual([]);
    expect(first.stats).toEqual(direct.stats);
    expect(first.main_skill).toEqual(direct.main_skill);
    expect(first.full_dps).toEqual(direct.full_dps);
    // Blink reserves 60 in all sets; Alchemist's Boon reserves 30 only in set 2.
    expect(stat(first, 'SpiritReserved')).toBe(60);
    expect(first.main_skill?.group_index).toBe(1);
    expect(metrics(request).skills.map(row => row.group_index)).toEqual([1]);
    expect(first.full_dps?.skills.map(row => row.group_index)).toEqual([1, 2]);

    const switched = switchWeapons({ ...state, params: { ...state.params, main_socket_group: 2 } }, 2);
    const secondRequest = toRequest(switched);
    const second = calculate(secondRequest);
    expect(second.item_errors).toEqual([]);
    expect(stat(second, 'SpiritReserved')).toBe(90);
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

  test('ordinary Infernalist export preserves omitted conditions and real edit-state calculations', async () => {
    const original = (await readFile(new URL('../../../examples/reference-builds/ninja-top10-2026-10-26/builds/infernalist-01/code.txt', import.meta.url), 'utf8')).trim();
    const importedState = (code: string): SavedSession['state'] => {
      const decoded = decode(code);
      return {
        ...materialize(decoded), pobCode: code, character: decoded.character,
        allocatedNodes: decoded.tree.allocated_nodes, attributeChoices: decoded.tree.attribute_choices,
        annotations: {}, params: { ...paramsFromDecoded(decoded), main_socket_group: decoded.main_socket_group ?? undefined },
      };
    };
    const imported = importedState(original);
    expect(imported.params.config_inputs).not.toHaveProperty('inDemonForm');
    const request = { ...toRequest(imported), breakdowns: [] };
    expect(request.pob_code).toBeUndefined();
    const before = calculate(request);
    expect(before.energy_trigger?.events_per_second).toBeGreaterThan(0);
    expect(before).toEqual(calculate({ pob_code: original, full_dps: request.full_dps, breakdowns: [] }));

    for (const edit of ['unchanged', 'level', 'false', 'true'] as const) {
      const edited = importedState(original);
      if (edit === 'level') edited.character = { ...edited.character, level: edited.character.level - 1 };
      if (edit === 'false' || edit === 'true') edited.params.config_inputs.inDemonForm = edit === 'true';
      const current = { ...toRequest(edited), breakdowns: [] };
      const expected = calculate(current);
      for (const base_code of [original, undefined]) {
        const saved = encode({ ...current, base_code });
        const reopened = importedState(saved);
        expect(reopened.params.config_inputs).toEqual(edited.params.config_inputs);
        expect(reopened.character).toEqual(edited.character);
        expect(calculate({ ...toRequest(reopened), breakdowns: [] })).toEqual(expected);
        expect(calculate({ pob_code: saved, full_dps: current.full_dps, breakdowns: [] })).toEqual(expected);
      }
    }
  }, 120_000);
});
