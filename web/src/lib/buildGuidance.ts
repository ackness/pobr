import type { BuildJson, CalculateBuildRequest, CalculateBuildResponse, GemCatalogEntry, GemInput, PassiveNode, SocketGroupInput } from '../api/types';
import { nameParts, rarityOf } from '../hooks/useLocalizedLines';
import { groupsForWeaponSet } from './weaponSets';

export interface ReferenceSource { url?: string; league?: string; gameVersion?: string; fetchedAt?: string }
export interface BuildReference { id: string; source: ReferenceSource; build: BuildJson }
export interface BuildProfile {
  className: string;
  ascendancy: string;
  mainSkill: string | null;
  supports: string[];
  uniques: string[];
}

export function mainGem(group: SocketGroupInput | undefined, catalog: Map<string, GemCatalogEntry>): GemInput | undefined {
  return group?.enabled ? group.gems.find(gem => catalog.get(gem.skill_id)?.is_support === false) : undefined;
}

export function buildProfile(request: CalculateBuildRequest, catalog: Map<string, GemCatalogEntry>, mainSkill?: string | null): BuildProfile {
  const group = request.main_socket_group === undefined ? undefined : request.socket_groups?.[request.main_socket_group];
  const selectedSkill = mainSkill && catalog.get(mainSkill)?.is_support === false && group?.gems.some(gem => gem.skill_id === mainSkill)
    ? mainSkill : mainGem(group, catalog)?.skill_id ?? null;
  return {
    className: request.character?.class_name ?? '', ascendancy: request.character?.ascendancy_name ?? '',
    mainSkill: group?.enabled ? selectedSkill : null,
    supports: group?.enabled ? group.gems.filter(gem => catalog.get(gem.skill_id)?.is_support).map(gem => gem.skill_id) : [],
    uniques: (request.items ?? []).filter(item => rarityOf(item.text) === 'unique').map(item => nameParts(item.text)[0]).filter(Boolean),
  };
}

export function referenceRequest(build: BuildJson): CalculateBuildRequest {
  return { character: build.character, items: build.items.equipped, socket_groups: groupsForWeaponSet(build.socket_groups, build.weapon_swap?.active ?? 1),
    main_socket_group: build.main_socket_group ?? undefined, allocated_nodes: build.tree.allocated_nodes };
}

function overlap(a: string[], b: string[]): number {
  const left = new Set(a), right = new Set(b);
  const union = new Set([...left, ...right]);
  return union.size ? [...left].filter(value => right.has(value)).length / union.size : 0;
}

/** Fixed, explained points. Missing and empty signals never count as a match. */
export function matchBuild(current: BuildProfile, other: BuildProfile) {
  const sameSkill = !!current.mainSkill && current.mainSkill === other.mainSkill;
  const sameAscendancy = !!current.ascendancy && current.ascendancy === other.ascendancy;
  const sameClass = !!current.className && current.className === other.className;
  const supportOverlap = sameSkill ? overlap(current.supports, other.supports) : 0;
  const uniqueOverlap = overlap(current.uniques, other.uniques);
  return { sameSkill, sameAscendancy, sameClass, supportOverlap, uniqueOverlap,
    score: Math.round((sameSkill ? 55 : 0) + (sameAscendancy ? 20 : sameClass ? 10 : 0) + supportOverlap * 15 + uniqueOverlap * 10) };
}

/** Canonical English names belong in ninja URLs; localized names are display only. */
export function ninjaSearchUrl(league: string, profile: BuildProfile, catalog: Map<string, GemCatalogEntry>, sameClass = true): string | null {
  if (!/^[a-z0-9][a-z0-9-]{0,79}$/.test(league)) return null;
  const url = new URL(`https://poe.ninja/poe2/builds/${league}`);
  if (sameClass && (profile.ascendancy || profile.className)) url.searchParams.set('class', profile.ascendancy || profile.className);
  const name = profile.mainSkill ? catalog.get(profile.mainSkill)?.name : undefined;
  if (name) url.searchParams.set('skills', name);
  return url.href;
}

export function compareGems(current: GemInput[], reference: GemInput[]) {
  const ids = [...new Set([...reference, ...current].map(gem => gem.skill_id))];
  return ids.map(id => ({ id, current: current.find(gem => gem.skill_id === id), reference: reference.find(gem => gem.skill_id === id) }));
}

export function keyPassives(ids: number[], nodes: PassiveNode[]): PassiveNode[] {
  const allocated = new Set(ids);
  return nodes.filter(node => allocated.has(node.skill) && (node.kind === 'keystone' || node.kind === 'notable'));
}

/** These are measured gaps to inspect, not inferred build-mechanic advice. */
export function resistanceGaps(calc: CalculateBuildResponse | null, target: number): { stat: string; value: number; target: number; gap: number }[] {
  const stats = new Map(calc?.stats.map(stat => [stat.id, stat.value]));
  return ['Fire', 'Cold', 'Lightning'].flatMap(element => {
    const value = stats.get(`${element}Resist`);
    if (typeof value !== 'number' || !Number.isFinite(value) || typeof target !== 'number' || !Number.isFinite(target) || value >= target) return [];
    return [{ stat: `${element}Resist`, value, target, gap: target - value }];
  });
}
