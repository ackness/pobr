import type { BuildSession } from '../hooks/useBuildSession';

/** Selection is a calculation perspective, so it cannot be an edit's comparison baseline. */
export function calculatedSkillKey(session: Pick<BuildSession, 'calcParams' | 'socketGroups'>): string {
  const index = session.calcParams.main_socket_group;
  return `${index ?? ''}:${index === undefined ? '' : session.socketGroups[index]?.main_active_skill ?? 1}`;
}
