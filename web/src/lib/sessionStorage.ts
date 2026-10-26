export interface SessionStorageKeys {
  workspace: string;
  library: string;
  notes: string;
  warnBeforeUnload: boolean;
}

export const LEGACY_STORAGE: SessionStorageKeys = {
  workspace: 'pobr-build-state', library: 'pobr-library', notes: 'pobr-notes', warnBeforeUnload: true,
};

export const V2_STORAGE: SessionStorageKeys = {
  workspace: 'pobr-v2-build-state', library: 'pobr-v2-library', notes: 'pobr-v2-notes', warnBeforeUnload: false,
};
