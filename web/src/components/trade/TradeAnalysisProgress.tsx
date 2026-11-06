import type { Lang } from '../../lib/i18n';
import { tradeAnalysisText } from '../../lib/tradeAnalysisText';
import { AppButton } from '../shared/controls';

export interface SlotProgress {
  done: number;
  total: number;
  measuredMs: number;
  measuredSlots: number;
}

/** Cached slots have no timing sample; estimate only from positions finished in this run. */
export function remainingMinutes(progress: SlotProgress): number | null {
  if (progress.measuredSlots === 0 || progress.done >= progress.total) return null;
  return Math.max(1, Math.ceil(progress.measuredMs / progress.measuredSlots * (progress.total - progress.done) / 60_000));
}

export function TradeAnalysisProgress({ slots, position, fraction = 0, lang, onStop }: {
  slots: SlotProgress; position: string; fraction?: number; lang: Lang; onStop: () => void;
}) {
  const t = (key: Parameters<typeof tradeAnalysisText>[1]) => tradeAnalysisText(lang, key);
  const estimate = remainingMinutes(slots);
  const label = t('positionProgress').replace('{index}', String(Math.min(slots.done + 1, slots.total)))
    .replace('{total}', String(slots.total));
  return <div className="trade-progress">
    <span role="status">{label} · {position}{estimate !== null && <> · {t('remaining').replace('{minutes}', String(estimate))}</>}</span>
    <div className="trade-progress-controls">
      <progress aria-label={label} value={slots.done + Math.max(0, Math.min(1, fraction))} max={slots.total} />
      <AppButton onClick={onStop}>{t('stop')}</AppButton>
    </div>
  </div>;
}
