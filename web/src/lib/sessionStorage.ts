export interface SessionStorageKeys {
  workspace: string;
  library: string;
  notes: string;
}

/** Where the first interface kept its saves. They are only read, once, to carry them over (`app/storage.ts`). */
export const LEGACY_STORAGE: SessionStorageKeys = {
  workspace: 'pobr-build-state', library: 'pobr-library', notes: 'pobr-notes',
};

/**
 * Every saved setting keeps the `pobr-v2-` prefix it was first written under. Renaming it
 * would orphan what people already have saved in their browsers, so it is spelled here only.
 */
export function uiStorageKey(key: string) {
  return key.replace(/^pobr-/, 'pobr-v2-');
}

export const STORAGE: SessionStorageKeys = {
  workspace: uiStorageKey('pobr-build-state'), library: uiStorageKey('pobr-library'), notes: uiStorageKey('pobr-notes'),
};
