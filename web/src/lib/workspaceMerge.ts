import type { BuildWorkspace, LocalBuild } from './buildWorkspace';

/**
 * Several tabs of one browser share the library, and each writes the whole of it. Merging what
 * another tab wrote with what this tab holds, against the library as this tab last saw it (`base`),
 * keeps what either side changed instead of letting the later writer erase the other's work.
 *
 * Builds and stages are matched by id. An entry only one side changed takes that side's version;
 * an entry both sides changed keeps this tab's (the edit just made) and is reported as a conflict.
 * An entry one side deleted is gone only if the other side did not touch it since `base`: an edit
 * beats a deletion, so nothing the player changed is lost.
 */
export interface MergedWorkspace {
  workspace: BuildWorkspace;
  /** Stages both tabs changed since `base`; this tab's version was kept. */
  conflicts: string[];
}

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/** Whether a side's entry has to be kept when the other side does not have it. */
function keepsWithoutOther<T>(base: T | undefined, entry: T): boolean {
  // Never seen at `base`: created since. Seen and changed: edited since. Seen and unchanged: the
  // other side deleted it and nobody touched it, so it stays deleted.
  return base === undefined || !same(base, entry);
}

/** Ids in the order of `first`, then those only `second` has, in its order. */
function orderOf<T extends { id: string }>(first: T[], second: T[]): string[] {
  const seen = new Set(first.map(entry => entry.id));
  return [...first.map(entry => entry.id), ...second.filter(entry => !seen.has(entry.id)).map(entry => entry.id)];
}

/** The merge of lists of entries matched by id (stages of a build, items and skill sets of the saved library). */
function mergeEntries<T extends { id: string }>(base: T[], mine: T[], theirs: T[], conflicts: string[]): T[] {
  const of = (list: T[]) => new Map(list.map(entry => [entry.id, entry]));
  const [b, m, t] = [of(base), of(mine), of(theirs)];
  const merged: T[] = [];
  for (const id of orderOf(theirs, mine)) {
    const [sb, sm, st] = [b.get(id), m.get(id), t.get(id)];
    if (sm && st) {
      if (same(sm, st)) merged.push(sm);
      else if (sb && same(sb, sm)) merged.push(st);
      else if (sb && same(sb, st)) merged.push(sm);
      else { merged.push(sm); conflicts.push(id); }
    } else if (sm) {
      if (keepsWithoutOther(sb, sm)) merged.push(sm);
    } else if (st && keepsWithoutOther(sb, st)) merged.push(st);
  }
  return merged;
}

function mergeBuild(base: LocalBuild | undefined, mine: LocalBuild, theirs: LocalBuild, conflicts: string[]): LocalBuild {
  const stages = mergeEntries(base?.stages ?? [], mine.stages, theirs.stages, conflicts);
  // A build keeps a stage: if the sides disagree so that none is left, this tab's stay.
  const kept = stages.length > 0 ? stages : mine.stages;
  const has = (id: string) => kept.some(stage => stage.id === id);
  return {
    id: mine.id,
    name: !base || mine.name !== base.name ? mine.name : theirs.name,
    activeStage: has(mine.activeStage) ? mine.activeStage : has(theirs.activeStage) ? theirs.activeStage : kept[0].id,
    stages: kept,
  };
}

/**
 * `base` is the library as this tab last read or wrote it (`null` when unknown: then an entry both
 * sides have is a conflict unless they agree, and one only a side has is kept).
 */
export function mergeWorkspaces(base: BuildWorkspace | null, mine: BuildWorkspace, theirs: BuildWorkspace): MergedWorkspace {
  const conflicts: string[] = [];
  const of = (list: LocalBuild[]) => new Map(list.map(build => [build.id, build]));
  const [b, m, t] = [of(base?.builds ?? []), of(mine.builds), of(theirs.builds)];
  const builds: LocalBuild[] = [];
  for (const id of orderOf(theirs.builds, mine.builds)) {
    const [eb, em, et] = [b.get(id), m.get(id), t.get(id)];
    if (em && et) builds.push(mergeBuild(eb, em, et, conflicts));
    else if (em) {
      if (keepsWithoutOther(eb, em)) builds.push(em);
    } else if (et && keepsWithoutOther(eb, et)) builds.push(et);
  }
  // The library keeps a build: if the sides disagree so that none is left, this tab's stay.
  const kept = builds.length > 0 ? builds : mine.builds;
  const has = (id: string) => kept.some(build => build.id === id);
  return {
    workspace: {
      version: 1,
      activeBuild: has(mine.activeBuild) ? mine.activeBuild : has(theirs.activeBuild) ? theirs.activeBuild : kept[0].id,
      builds: kept,
    },
    conflicts,
  };
}

/**
 * The saved library of items, jewels and skill sets is shared by the tabs the same way: entries are
 * matched by id and merged against the library as this tab last saw it (`base`), with the same rules.
 */
export function mergeLibraries<L extends { items: { id: string }[]; skillSets: { id: string }[] }>(base: L, mine: L, theirs: L): L {
  const ignored: string[] = [];
  return {
    ...mine,
    items: mergeEntries(base.items, mine.items, theirs.items, ignored),
    skillSets: mergeEntries(base.skillSets, mine.skillSets, theirs.skillSets, ignored),
  };
}
