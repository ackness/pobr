import { LEGACY_STORAGE, V2_STORAGE } from '../lib/sessionStorage';

export const MIGRATION_SNAPSHOT = 'pobr-v2-original-backup';
export const MIGRATION_MARKER = 'pobr-v2-storage-initialized';
type Store = Pick<Storage, 'getItem' | 'setItem'>;
type Snapshot = Record<'workspace' | 'library' | 'notes', string | null>;
const keys = ['workspace', 'library', 'notes'] as const;

/** Copy once; never change the source. Retain raw bytes, including invalid saves. */
export function initializeV2Storage(storage: Store): 'copied' | 'empty' | 'existing' {
  if (storage.getItem(MIGRATION_MARKER)) return 'existing';
  const backup = storage.getItem(MIGRATION_SNAPSHOT);
  if (!backup && storage.getItem(V2_STORAGE.workspace) !== null) {
    storage.setItem(MIGRATION_MARKER, 'existing');
    return 'existing';
  }
  // The snapshot makes a failed quota-limited copy retryable without reading
  // a newer version of the legacy workspace midway through the migration.
  const snapshot: Snapshot = backup ? JSON.parse(backup) : {
    workspace: storage.getItem(LEGACY_STORAGE.workspace),
    library: storage.getItem(LEGACY_STORAGE.library),
    notes: storage.getItem(LEGACY_STORAGE.notes),
  };
  if (!snapshot || keys.some(key => snapshot[key] !== null && typeof snapshot[key] !== 'string')) {
    throw new Error('Invalid V2 migration backup');
  }
  if (!backup) storage.setItem(MIGRATION_SNAPSHOT, JSON.stringify(snapshot));
  for (const key of keys) {
    if (snapshot[key] !== null && storage.getItem(V2_STORAGE[key]) === null) storage.setItem(V2_STORAGE[key], snapshot[key]);
  }
  const result = snapshot.workspace === null ? 'empty' : 'copied';
  storage.setItem(MIGRATION_MARKER, result);
  return result;
}
