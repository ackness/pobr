import { describe, expect, it } from 'vitest';
import { initializeV2Storage, MIGRATION_MARKER, MIGRATION_SNAPSHOT } from './storage';
import { LEGACY_STORAGE, V2_STORAGE, uiStorageKey } from '../lib/sessionStorage';
function store(initial: Record<string, string> = {}) {
  const values = new Map(Object.entries(initial));
  return { values, getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); } };
}
describe('V2 storage isolation', () => {
  it('copies raw saves once and never writes back to the legacy workspace', () => {
    const s = store({ [LEGACY_STORAGE.workspace]: '{original}', [LEGACY_STORAGE.library]: '[items]', [LEGACY_STORAGE.notes]: 'notes' });
    expect(initializeV2Storage(s)).toBe('copied');
    expect(s.getItem(V2_STORAGE.workspace)).toBe('{original}');
    s.setItem(V2_STORAGE.workspace, '{edited}'); s.setItem(LEGACY_STORAGE.workspace, '{legacy edit}');
    expect(initializeV2Storage(s)).toBe('existing');
    expect(s.getItem(V2_STORAGE.workspace)).toBe('{edited}');
    expect(s.getItem(LEGACY_STORAGE.workspace)).toBe('{legacy edit}');
    expect(JSON.parse(s.getItem(MIGRATION_SNAPSHOT)!).workspace).toBe('{original}');
  });
  it('resumes a partial copy from the original snapshot after quota failure', () => {
    const s = store({ [LEGACY_STORAGE.workspace]: 'workspace', [LEGACY_STORAGE.library]: 'original items' });
    const failing = { ...s, setItem: (key: string, value: string) => { if (key === V2_STORAGE.library) throw new Error('QuotaExceeded'); s.setItem(key, value); } };
    expect(() => initializeV2Storage(failing)).toThrow('QuotaExceeded');
    expect(s.getItem(MIGRATION_MARKER)).toBeNull();
    s.setItem(LEGACY_STORAGE.library, 'later items');
    expect(initializeV2Storage(s)).toBe('copied');
    expect(s.getItem(V2_STORAGE.library)).toBe('original items');
  });
  it('preserves an existing V2 workspace without a marker', () => {
    const s = store({ [V2_STORAGE.workspace]: 'v2', [LEGACY_STORAGE.workspace]: 'legacy' });
    expect(initializeV2Storage(s)).toBe('existing'); expect(s.getItem(V2_STORAGE.workspace)).toBe('v2');
  });
  it('does not mark inaccessible or invalid snapshots as successfully migrated', () => {
    const s = store({ [MIGRATION_SNAPSHOT]: '{broken' });
    expect(() => initializeV2Storage(s)).toThrow(); expect(s.getItem(MIGRATION_MARKER)).toBeNull();
  });
});

it('namespaces planner preferences without changing the legacy keys', () => {
  expect(uiStorageKey('pobr-trade-budget', true)).toBe('pobr-v2-trade-budget');
  expect(uiStorageKey('pobr-support-exclusions:Fireball', true)).toBe('pobr-v2-support-exclusions:Fireball');
  expect(uiStorageKey('pobr-trade-budget', false)).toBe('pobr-trade-budget');
});
