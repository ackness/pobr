import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';

export function WeaponSetControl({ session, lang }: { session: BuildSession; lang: Lang }) {
  const tt = bindT(lang);
  return <div className="weapon-set-control" role="group" aria-label={tt('weapons.active')}>
    <span>{tt('weapons.active')}</span>
    {([1, 2] as const).map(set => <button type="button" key={set}
      aria-pressed={session.activeWeaponSet === set} disabled={session.busy}
      onClick={() => session.setWeaponSet(set)}>{tt('weapons.set')} {set}</button>)}
  </div>;
}
