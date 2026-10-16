import type { Lang } from '../../lib/i18n';

const en = {
  title: 'Node power report', stat: 'Report stat', search: 'Filter nodes', small: 'Include small nodes',
  hint: 'Gain/point uses the full connected route, including travel points. Single node uses the heat-map probe. Ascendancy routes spend ascendancy points. Each run evaluates up to 128 filtered nodes in cancellable batches; repeat to cover the remaining nodes.',
  scope: 'Masteries and other weapon-set routes are unavailable. New unsupported effects are diagnosed instead of scored. These are route comparisons, not automatic allocations or a guarantee that your current point budget can afford every route.',
  attribute: 'New travel attributes', run: 'Evaluate next 128 nodes', cancel: 'Cancel report', refresh: 'Reset report',
  node: 'Node', kind: 'Type', points: 'Points', gain: 'Route gain', perPoint: 'Gain / point', single: 'Single node',
  pending: 'Not evaluated', unavailable: 'No supported route', unsupported: 'Unsupported effects', error: 'Calculation unavailable',
  ready: 'Evaluated', cancelled: 'Report cancelled; completed rows retained.', complete: 'Batch complete.',
  results: 'Filtered nodes', evaluated: 'Evaluated', remaining: 'Remaining', empty: 'No matching unallocated nodes.',
  previous: 'Previous rows', next: 'Next rows', page: 'Page', limits: 'Baseline calculation limits',
  asc: 'Ascendancy', str: 'Strength', dex: 'Dexterity', int: 'Intelligence',
  normal: 'Small', notable: 'Notable', keystone: 'Keystone',
};
type Labels = typeof en;
export const powerLabels: Record<Lang, Labels> = {
  'en-US': en,
  'zh-TW': {
    title: '節點威力報告', stat: '報告屬性', search: '篩選節點', small: '包含小點',
    hint: '每點收益計算包含過路點的完整連通路線；單點收益使用熱力圖試算。昇華路線消耗昇華點。每次分批評估最多 128 個符合篩選的節點，可取消並繼續評估剩餘節點。',
    scope: '不支援精通選項及其他武器組路線。新增未建模效果會列出診斷而非評分。這是路線比較，不會自動加點，也不代表目前點數預算足夠投入每條路線。',
    attribute: '新增過路屬性', run: '評估接下來 128 個節點', cancel: '取消報告', refresh: '重設報告',
    node: '節點', kind: '類型', points: '點數', gain: '路線收益', perPoint: '每點收益', single: '單點收益',
    pending: '尚未評估', unavailable: '無支援的路線', unsupported: '未支援效果', error: '無法計算',
    ready: '已評估', cancelled: '已取消報告，保留完成的列。', complete: '本批評估完成。',
    results: '符合篩選的節點', evaluated: '已評估', remaining: '待評估', empty: '沒有符合條件的未分配節點。',
    previous: '上一頁', next: '下一頁', page: '頁', limits: '基準計算限制',
    asc: '昇華', str: '力量', dex: '敏捷', int: '智慧', normal: '小點', notable: '核心天賦', keystone: '關鍵天賦',
  },
  'zh-CN': {
    title: '节点威力报告', stat: '报告属性', search: '筛选节点', small: '包含小点',
    hint: '每点收益计算包含过路点的完整连通路线；单点收益使用热力图试算。升华路线消耗升华点。每次分批评估最多 128 个符合筛选的节点，可取消并继续评估剩余节点。',
    scope: '不支持精通选项及其他武器组路线。新增未建模效果会列出诊断而非评分。这是路线比较，不会自动加点，也不代表当前点数预算足够投入每条路线。',
    attribute: '新增过路属性', run: '评估接下来 128 个节点', cancel: '取消报告', refresh: '重置报告',
    node: '节点', kind: '类型', points: '点数', gain: '路线收益', perPoint: '每点收益', single: '单点收益',
    pending: '尚未评估', unavailable: '无支持的路线', unsupported: '未支持效果', error: '无法计算',
    ready: '已评估', cancelled: '已取消报告，保留完成的行。', complete: '本批评估完成。',
    results: '符合筛选的节点', evaluated: '已评估', remaining: '待评估', empty: '没有符合条件的未分配节点。',
    previous: '上一页', next: '下一页', page: '页', limits: '基准计算限制',
    asc: '升华', str: '力量', dex: '敏捷', int: '智慧', normal: '小点', notable: '核心天赋', keystone: '关键天赋',
  },
};
