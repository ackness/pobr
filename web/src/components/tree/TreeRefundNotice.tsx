import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import { AppButton } from '../shared/controls';
import { refundDelta } from './treePresentation';

export interface RefundReceipt { version: number; points: number; baseline: BuildSession['calc'] }

/** Remains beside the tree until another edit replaces the receipt or the player undoes it. */
export function TreeRefundNotice({ receipt, session, lang, onUndo }: {
  receipt: RefundReceipt | null; session: BuildSession; lang: Lang; onUndo: () => void;
}) {
  const tt = bindT(lang);
  if (!receipt || receipt.version !== session.stateVersion) return null;
  const delta = session.calcError ? tt('tree.deltaUnavailable')
    : session.busy || session.calc === receipt.baseline ? tt('tree.deltaPending')
      : refundDelta(receipt.baseline, session.calc, lang);
  return <div className="tree-refund-result" role="status">
    {tt('tree.refundResult').replace('{points}', String(receipt.points)).replace('{delta}', delta)}
    <AppButton disabled={session.busy || !session.canUndoTree} onClick={onUndo}>{tt('tree.undoRefund')}</AppButton>
  </div>;
}
