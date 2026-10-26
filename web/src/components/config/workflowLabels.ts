import type { Lang } from '../../lib/i18n';

const labels = {
  options: ['Conditions & options', '条件与选项', '條件與選項'],
  jump: ['Jump to a group', '定位配置分组', '定位設定群組'],
  help: ['Help & source', '说明与来源', '說明與來源'],
  rawKey: ['Internal key', '原始键名', '原始鍵名'],
  sourceHelp: ['Help is shown in its source language.', '说明保留来源语言。', '說明保留來源語言。'],
  guidance: ['How configuration works', '配置说明', '設定說明'],
  advanced: ['Advanced values', '高级原始值', '進階原始值'],
  type: ['Value type', '值类型', '值類型'],
  boolean: ['Boolean', '布尔值', '布林值'],
  number: ['Number', '数字', '數字'],
  string: ['Text', '文本', '文字'],
  unset: ['Unset', '未设置', '未設定'],
  true: ['True', '是', '是'],
  false: ['False', '否', '否'],
  savedType: ['Saved value', '保存的原始值', '儲存的原始值'],
  party: ['Party', '队伍', '隊伍'],
  partyHint: ['Party auras, curses and other effects contribute to calculations. Blocks retain their exported effect strength.', '队友的光环、诅咒和其他效果会计入计算。文本块保留导出时的效果强度。', '隊友的光環、詛咒和其他效果會計入計算。文字區塊保留匯出時的效果強度。'],
  member: ['Party member', '队友', '隊友'],
  memberCode: ['Member build code', '队友构筑码', '隊友構築碼'],
  importMember: ['Import member', '导入队友', '匯入隊友'],
  addMember: ['Add member', '添加队友', '新增隊友'],
  name: ['Member name', '队友名称', '隊友名稱'],
  enabled: ['Enable member', '启用队友', '啟用隊友'],
  remove: ['Remove member', '移除队友', '移除隊友'],
  changed: ['The build changed. Import the member again.', '构筑已更改，请重新导入。', '構築已變更，請重新匯入。'],
  PartyMemberStats: ['Member attributes', '队友属性', '隊友屬性'],
  Aura: ['Auras', '光环', '光環'],
  Curse: ['Curses', '诅咒', '詛咒'],
  'Warcry Skills': ['Warcries', '战吼', '戰吼'],
  'Link Skills': ['Link skills', '链接技能', '連結技能'],
  EnemyConditions: ['Enemy conditions', '敌人状态', '敵人狀態'],
  EnemyMods: ['Enemy modifiers', '敌人词条', '敵人詞綴'],
} as const;

export function workflowLabel(lang: Lang, key: keyof typeof labels): string {
  return labels[key][lang === 'en-US' ? 0 : lang === 'zh-CN' ? 1 : 2];
}
