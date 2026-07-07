import { useState } from 'react';
import type { ConfigInputValue, EnemyTier } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import './config.css';

interface Props {
  session: BuildSession;
  lang: Lang;
}

const ENEMY_TIERS: EnemyTier[] = ['none', 'boss', 'pinnacle', 'uber'];

/** Config 页：敌人档位 + build 自带 `<Input>` 键值的查看/覆盖 → 重算。 */
export function ConfigPanel({ session, lang }: Props) {
  const tt = bindT(lang);
  const overrides = session.calcParams.config_inputs;
  const [newKey, setNewKey] = useState('');
  const [newValue, setNewValue] = useState('true');

  const effective: Record<string, ConfigInputValue> = {
    ...(session.build?.config_inputs ?? {}),
    ...overrides,
  };
  const keys = Object.keys(effective).sort();

  const parseValue = (raw: string): ConfigInputValue => {
    if (raw === 'true') return true;
    if (raw === 'false') return false;
    const n = Number(raw);
    return Number.isFinite(n) && raw.trim() !== '' ? n : raw;
  };

  return (
    <section aria-labelledby="config-heading">
      <h2 id="config-heading" className="panel-heading">
        {tt('config.title')}
      </h2>

      <div className="config-row">
        <label htmlFor="enemy-tier">{tt('config.enemyTier')}</label>
        <select
          id="enemy-tier"
          value={session.calcParams.enemy_tier ?? 'pinnacle'}
          disabled={session.busy}
          onChange={(e) => session.updateParams({ enemy_tier: e.target.value as EnemyTier })}
        >
          {ENEMY_TIERS.map((tier) => (
            <option key={tier} value={tier}>
              {tier}
            </option>
          ))}
        </select>
      </div>

      <h3 className="panel-subheading">{tt('config.inputs')}</h3>
      <p className="config-hint">
{tt('config.hint')}
      </p>
      <div className="config-grid">
        {keys.map((key) => {
          const value = effective[key];
          const overridden = key in overrides;
          return (
            <div key={key} className={`config-item${overridden ? ' is-overridden' : ''}`}>
              <span className="config-key">{key}</span>
              {typeof value === 'boolean' ? (
                <input
                  type="checkbox"
                  checked={value}
                  disabled={session.busy}
                  onChange={(e) => session.setConfigInput(key, e.target.checked)}
                  aria-label={key}
                />
              ) : (
                <input
                  className="config-value"
                  defaultValue={String(value)}
                  disabled={session.busy}
                  aria-label={key}
                  onBlur={(e) => {
                    const parsed = parseValue(e.target.value);
                    if (parsed !== value) session.setConfigInput(key, parsed);
                  }}
                />
              )}
              {overridden && (
                <button
                  className="config-reset"
                  title={tt('config.reset')}
                  onClick={() => session.setConfigInput(key, null)}
                >
                  ↺
                </button>
              )}
            </div>
          );
        })}
      </div>

      <h3 className="panel-subheading">{tt('config.addTitle')}</h3>
      <div className="config-add">
        <input
          placeholder={tt('config.keyPlaceholder')}
          value={newKey}
          onChange={(e) => setNewKey(e.target.value)}
          aria-label={tt('config.key')}
        />
        <input
          placeholder="true / 40 / text"
          value={newValue}
          onChange={(e) => setNewValue(e.target.value)}
          aria-label={tt('config.valueLabel')}
        />
        <button
          disabled={!newKey.trim() || session.busy}
          onClick={() => {
            session.setConfigInput(newKey.trim(), parseValue(newValue));
            setNewKey('');
          }}
        >
          {tt('config.addButton')}
        </button>
      </div>
    </section>
  );
}
