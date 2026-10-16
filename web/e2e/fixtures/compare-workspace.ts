import type { SavedSession } from '../../src/hooks/useBuildSession';
import type { BuildWorkspace } from '../../src/lib/buildWorkspace';

export function comparisonWorkspace(): BuildWorkspace {
  const saved = (life: number, bonus: number, level: number): SavedSession => ({ version: 1, notes: `Keep notes ${life}`, state: {
    pobCode: null, treeVersion: '0_5', character: { class_name: 'Ranger', ascendancy_name: '', level: 20 },
    allocatedNodes: [], attributeChoices: {}, items: [{ slot: 'ring1', text: `Rarity: RARE\nLife Ring\nCoral Ring\nImplicits: 0\n+${life} to maximum Life` }],
    flasks: [], limbs: [], jewels: [], annotations: { 'item:ring1': `Keep annotation ${life}` },
    socketGroups: [{ enabled: true, main_active_skill: 1, gems: [{ skill_id: 'SparkPlayer', level, quality: 0 }] }],
    params: { main_socket_group: 0, config_inputs: { conditionFullLife: false, ActiveBrands: 0 }, extra_modifiers: [`+${bonus} to maximum Life`] },
  } });
  return { version: 1, activeBuild: 'test-build', builds: [{ id: 'test-build', name: 'Spark', activeStage: 'stage-b', stages: [
    { id: 'stage-a', name: 'Starter', saved: saved(100, 10, 5), drafts: {} },
    { id: 'stage-b', name: 'Target', saved: saved(200, 50, 6), drafts: {} },
  ] }] };
}
