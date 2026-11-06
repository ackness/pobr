import { useEffect, useState } from 'react';
import { getBackend } from '../api/backend';
import type { EnemyPresets } from '../api/enemyPresets';

/** Reuse the backend's verified snapshot transport, including when it lives in a Worker. */
export function useEnemyPresets(version: string | null | undefined): EnemyPresets | null {
  const [loaded, setLoaded] = useState<{ version: typeof version; table: EnemyPresets | null } | null>(null);
  useEffect(() => {
    let cancelled = false;
    getBackend().then(backend => backend.loadEnemyPresets())
      .then(table => { if (!cancelled) setLoaded({ version, table }); })
      .catch(() => { if (!cancelled) setLoaded({ version, table: null }); });
    return () => { cancelled = true; };
  }, [version]);
  return loaded?.version === version ? loaded?.table ?? null : null;
}
