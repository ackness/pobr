import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from 'react';
import { DEFAULT_OBJECTIVE_STATE, type ObjectiveState } from '../components/shared/OptimizerControls';
import { CONSTRAINT_STATS, OBJECTIVE_PRESETS } from '../lib/optimize';
import { readPreference, uiStorageKey, writePreference } from '../lib/sessionStorage';

const KEY = uiStorageKey('pobr-upgrade-goal');
const Context = createContext<{ goal: ObjectiveState; setGoal: (next: ObjectiveState) => void } | null>(null);

/** Equipment, support and passive planners share the player's intent, not stale results. */
export function UpgradeGoalProvider({ children }: { children: ReactNode }) {
  const [goal, setState] = useState<ObjectiveState>(() => {
    try {
      const saved = JSON.parse(readPreference(KEY) ?? 'null');
      if (saved && OBJECTIVE_PRESETS.some(preset => preset.id === saved.preset)) {
        return { ...DEFAULT_OBJECTIVE_STATE, preset: saved.preset,
          cStat: CONSTRAINT_STATS.includes(saved.cStat) ? saved.cStat : '',
          cMin: typeof saved.cMin === 'string' && (saved.cMin === '' || Number.isFinite(Number(saved.cMin))) ? saved.cMin : '',
          cMax: typeof saved.cMax === 'string' && (saved.cMax === '' || Number.isFinite(Number(saved.cMax))) ? saved.cMax : '',
          keepEhp: typeof saved.keepEhp === 'boolean' ? saved.keepEhp : true,
          resistanceFirst: saved.resistanceFirst === true,
          resistanceTarget: typeof saved.resistanceTarget === 'number' && saved.resistanceTarget >= 0 && saved.resistanceTarget <= 90 ? saved.resistanceTarget : 75,
        };
      }
    } catch { /* Invalid preferences fall back to the default goal. */ }
    return DEFAULT_OBJECTIVE_STATE;
  });
  const setGoal = useCallback((next: ObjectiveState) => {
    setState(next);
    writePreference(KEY, JSON.stringify(next));
  }, []);
  const value = useMemo(() => ({ goal, setGoal }), [goal, setGoal]);
  return <Context value={value}>{children}</Context>;
}

export function useUpgradeGoal() {
  const context = useContext(Context);
  if (!context) throw new Error('UpgradeGoalProvider is required');
  return context;
}
