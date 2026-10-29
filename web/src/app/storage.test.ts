import { describe, expect, it } from 'vitest';
import { initializeStorage, MIGRATION_MARKER, MIGRATION_SNAPSHOT } from './storage';
import { LEGACY_STORAGE, STORAGE, uiStorageKey } from '../lib/sessionStorage';
function store(initial: Record<string, string> = {}) {
  const values = new Map(Object.entries(initial));
  return { values, getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); } };
}
describe('carrying over the first interface\'s saves', () => {
  it('copies raw saves once and never writes back to the original workspace', () => {
    const s = store({ [LEGACY_STORAGE.workspace]: '{original}', [LEGACY_STORAGE.library]: '[items]', [LEGACY_STORAGE.notes]: 'notes' });
    expect(initializeStorage(s)).toBe('copied');
    expect(s.getItem(STORAGE.workspace)).toBe('{original}');
    s.setItem(STORAGE.workspace, '{edited}'); s.setItem(LEGACY_STORAGE.workspace, '{legacy edit}');
    expect(initializeStorage(s)).toBe('existing');
    expect(s.getItem(STORAGE.workspace)).toBe('{edited}');
    expect(s.getItem(LEGACY_STORAGE.workspace)).toBe('{legacy edit}');
    expect(JSON.parse(s.getItem(MIGRATION_SNAPSHOT)!).workspace).toBe('{original}');
  });
  it('resumes a partial copy from the original snapshot after quota failure', () => {
    const s = store({ [LEGACY_STORAGE.workspace]: 'workspace', [LEGACY_STORAGE.library]: 'original items' });
    const failing = { ...s, setItem: (key: string, value: string) => { if (key === STORAGE.library) throw new Error('QuotaExceeded'); s.setItem(key, value); } };
    expect(() => initializeStorage(failing)).toThrow('QuotaExceeded');
    expect(s.getItem(MIGRATION_MARKER)).toBeNull();
    s.setItem(LEGACY_STORAGE.library, 'later items');
    expect(initializeStorage(s)).toBe('copied');
    expect(s.getItem(STORAGE.library)).toBe('original items');
  });
  it('preserves an existing workspace without a marker', () => {
    const s = store({ [STORAGE.workspace]: 'current', [LEGACY_STORAGE.workspace]: 'legacy' });
    expect(initializeStorage(s)).toBe('existing'); expect(s.getItem(STORAGE.workspace)).toBe('current');
  });
  it('does not mark inaccessible or invalid snapshots as successfully migrated', () => {
    const s = store({ [MIGRATION_SNAPSHOT]: '{broken' });
    expect(() => initializeStorage(s)).toThrow(); expect(s.getItem(MIGRATION_MARKER)).toBeNull();
  });
});

it('keeps every saved setting under the name it was first written with', () => {
  // These strings are what people already have on their devices; renaming one orphans their save.
  expect(STORAGE).toEqual({ workspace: 'pobr-v2-build-state', library: 'pobr-v2-library', notes: 'pobr-v2-notes' });
  expect(LEGACY_STORAGE).toEqual({ workspace: 'pobr-build-state', library: 'pobr-library', notes: 'pobr-notes' });
  expect(uiStorageKey('pobr-trade-budget')).toBe('pobr-v2-trade-budget');
  expect(uiStorageKey('pobr-support-exclusions:Fireball')).toBe('pobr-v2-support-exclusions:Fireball');
  expect(MIGRATION_SNAPSHOT).toBe('pobr-v2-original-backup');
  expect(MIGRATION_MARKER).toBe('pobr-v2-storage-initialized');
});
