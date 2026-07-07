import { useState } from 'react';
import type { ConfigInputValue, EnemyTier } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { Lang } from '../../lib/statDisplay';
import './config.css';

interface Props {
  session: BuildSession;
  lang: Lang;
}

const ENEMY_TIERS: EnemyTier[] = ['none', 'boss', 'pinnacle', 'uber'];

/** Config 页：敌人档位 + build 自带 `<Input>` 键值的查看/覆盖 → 重算。 */
export function ConfigPanel({ session, lang }: Props) {
  const zh = lang === 'zh-TW';
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
        {zh ? '戰鬥配置' : 'Configuration'}
      </h2>

      <div className="config-row">
        <label htmlFor="enemy-tier">{zh ? '敵人檔位' : 'Enemy tier'}</label>
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

      <h3 className="panel-subheading">{zh ? 'Config 輸入（<Input> 鍵值）' : 'Config inputs'}</h3>
      <p className="config-hint">
        {zh
          ? '來自 build 的原始配置；修改值即重算。鍵名與 PoB2 Config 頁一致（如 conditionEnemyChilled）。'
          : 'Raw config inputs from the build; edits trigger recalculation. Keys match PoB2 config vars (e.g. conditionEnemyChilled).'}
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
                  title={zh ? '還原' : 'Reset'}
                  onClick={() => session.setConfigInput(key, null)}
                >
                  ↺
                </button>
              )}
            </div>
          );
        })}
      </div>

      <h3 className="panel-subheading">{zh ? '新增配置項' : 'Add config input'}</h3>
      <div className="config-add">
        <input
          placeholder={zh ? '鍵名（如 enemyDistance）' : 'key (e.g. enemyDistance)'}
          value={newKey}
          onChange={(e) => setNewKey(e.target.value)}
          aria-label={zh ? '配置鍵名' : 'Config key'}
        />
        <input
          placeholder="true / 40 / text"
          value={newValue}
          onChange={(e) => setNewValue(e.target.value)}
          aria-label={zh ? '配置值' : 'Config value'}
        />
        <button
          disabled={!newKey.trim() || session.busy}
          onClick={() => {
            session.setConfigInput(newKey.trim(), parseValue(newValue));
            setNewKey('');
          }}
        >
          {zh ? '加入並重算' : 'Add & recalc'}
        </button>
      </div>
    </section>
  );
}
