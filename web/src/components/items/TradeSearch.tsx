import { useEffect, useMemo, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { TradeStatEntry, VariantInput } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang, type UiKey } from '../../lib/i18n';
import { OBJECTIVE_PRESETS, evaluateVariants, scoreOf } from '../../lib/optimize';
import {
  buildTradeUrl,
  lineValue,
  loadTradeMap,
  normalizeTradeLine,
  type WeightedStat,
} from '../../lib/trade';
import { AppSelect } from '../shared/AppSelect';
import { CopyButton } from '../shared/CopyButton';
import { OptimizerProgress } from '../shared/OptimizerControls';

interface Props {
  session: BuildSession;
  lang: Lang;
  slot: string;
  itemText: string;
}

const LEAGUE_KEY = 'pobr-trade-league';

/**
 * 市集加权搜索（PoB2 式「找更好的同类」）：当前物品的可映射词条逐条摘除重算
 * 得边际权重，拼 trade2 weighted-sum 查询直开官方市集（无需登录/API）。
 */
export function TradeSearch({ session, lang, slot, itemText }: Props) {
  const tt = bindT(lang);
  const [expanded, setExpanded] = useState(false);
  const [tradeMap, setTradeMap] = useState<Record<string, TradeStatEntry> | null>(null);
  const [league, setLeague] = useState(() => localStorage.getItem(LEAGUE_KEY) ?? 'Standard');
  const [preset, setPreset] = useState('dps');
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<{ weighted: WeightedStat[]; url: string } | null>(null);
  const [explicitLines, setExplicitLines] = useState<string[]>([]);

  useEffect(() => {
    loadTradeMap().then(setTradeMap);
  }, []);
  useEffect(() => {
    setResult(null);
    let cancelled = false;
    getBackend()
      .then((b) => b.classifyItemLines(itemText))
      .then((lines) => {
        if (!cancelled) {
          setExplicitLines(lines.filter((l) => l.kind === 'explicit').map((l) => l.text));
        }
      })
      .catch(() => setExplicitLines([]));
    return () => {
      cancelled = true;
    };
  }, [itemText]);

  /** 可映射词条：模板命中 + 有数值（权重分母）。 */
  const mapped = useMemo(() => {
    if (!tradeMap) return [];
    return explicitLines.flatMap((line) => {
      const entry = tradeMap[normalizeTradeLine(line)];
      const value = lineValue(line);
      return entry && value !== null ? [{ line, entry, value }] : [];
    });
  }, [tradeMap, explicitLines]);

  // 映射表为空（mock 后端 / 数据包缺文件）时整体隐藏。
  if (tradeMap && Object.keys(tradeMap).length === 0) return null;

  /** 摘掉物品文本中与词条对应的那一行（比对剥 {…} 后的整行）。 */
  const removeLine = (text: string, target: string): string => {
    const lines = text.split('\n');
    const idx = lines.findIndex((raw) => raw.replace(/\{[^}]*\}/g, '').trim() === target);
    if (idx < 0) return text;
    return [...lines.slice(0, idx), ...lines.slice(idx + 1)].join('\n');
  };

  const run = async () => {
    const request = session.currentRequest();
    if (!request || mapped.length === 0) return;
    const presetDef = OBJECTIVE_PRESETS.find((p) => p.id === preset) ?? OBJECTIVE_PRESETS[0];
    const objective = { stat: presetDef.stat, per: presetDef.per, constraints: [] };
    const variants: VariantInput[] = mapped.map((m) => ({
      label: m.line,
      set_items: [{ slot, text: removeLine(itemText, m.line) }],
    }));
    setProgress({ done: 0, total: variants.length });
    setError(null);
    try {
      const { baseline, results } = await evaluateVariants({
        request,
        variants,
        onProgress: (done, total) => setProgress({ done, total }),
      });
      const baseScore = scoreOf(baseline, objective);
      const weighted: WeightedStat[] = results.flatMap((r) => {
        const m = mapped[r.index];
        if (!m || r.error) return [];
        // 边际收益：摘掉该词条后目标下降多少（≤0 = 对目标无贡献，不进权重）。
        const delta = baseScore - scoreOf(r.stats, objective);
        if (delta <= 0) return [];
        return [{ id: m.entry.id, weight: delta / m.value, line: m.line, value: m.value }];
      });
      if (weighted.length === 0) {
        setResult(null);
        setError(tt('trade.noWeights'));
        return;
      }
      setResult({ weighted, url: buildTradeUrl(league, weighted) });
    } catch (err: unknown) {
      setResult(null);
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setProgress(null);
    }
  };

  return (
    <div className="gem-optimizer">
      <button
        className="gem-optimizer-toggle"
        aria-expanded={expanded}
        onClick={() => setExpanded(!expanded)}
      >
        <span className="row-caret" aria-hidden>
          ▸
        </span>
        {tt('trade.open')}
      </button>
      {expanded && (
        <div className="gem-optimizer-body">
          <p className="skills-hint">{tt('trade.hint')}</p>
          {mapped.length === 0 ? (
            <p className="skills-hint">{tt('trade.noMapped')}</p>
          ) : (
            <div className="opt-row">
              <label>
                {tt('trade.league')}
                <input
                  value={league}
                  onChange={(e) => {
                    setLeague(e.target.value);
                    localStorage.setItem(LEAGUE_KEY, e.target.value);
                  }}
                />
              </label>
              <label>
                {tt('opt.objective')}
                <AppSelect
                  value={preset}
                  options={OBJECTIVE_PRESETS.map((p) => ({
                    value: p.id,
                    label: tt(p.labelKey as UiKey),
                  }))}
                  onChange={setPreset}
                  ariaLabel={tt('opt.objective')}
                />
              </label>
              <button
                className="opt-run"
                disabled={session.busy || progress !== null}
                onClick={run}
              >
                {progress
                  ? tt('opt.running')
                  : `${tt('trade.generate')}（${mapped.length} ${tt('trade.lines')}）`}
              </button>
            </div>
          )}
          {progress && (
            <OptimizerProgress
              done={progress.done}
              total={progress.total}
              onCancel={() => {}}
              lang={lang}
            />
          )}
          {error && <p className="opt-error">{error}</p>}
          {result && (
            <>
              <table className="opt-table">
                <tbody>
                  {result.weighted.map((w) => (
                    <tr key={w.id}>
                      <td>{w.line}</td>
                      <td className="opt-score">{w.weight.toFixed(3)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <div className="opt-row">
                <a href={result.url} target="_blank" rel="noreferrer">
                  <button>{tt('trade.openSite')}</button>
                </a>
                <CopyButton text={result.url} lang={lang} />
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
