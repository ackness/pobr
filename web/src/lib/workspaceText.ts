import type { Lang } from './i18n';
const text = {
  'en-US': {
    builds: 'Build', stages: 'Stage', initialBuild: 'My build', initialStage: 'Current', manage: 'Manage builds',
    title: 'Builds & stages', hint: 'Keep separate builds for different skills. Copy a stage to plan leveling, progression or endgame with its own tree, gear and notes.',
    newBuild: 'New build', copyStage: 'Copy current stage', renameBuild: 'Rename build', renameStage: 'Rename stage',
    name: 'Name', save: 'Save', cancel: 'Cancel', saved: 'Saved locally', failed: 'Browser save failed. Download a backup before closing this page.',
    share: 'Share this build · all stages', backup: 'Back up all builds', shareHint: 'The build file includes every stage, passive allocation and note. Import it on Build to add it to your library. PoB codes below share only the current stage.',
    backupConfirm: 'Restore this backup? It will replace all locally saved builds. Download your current backup first.',
    importHint: 'Import replaces the current stage. Copy it first to keep a separate version. Shared multi-stage build files are added as a new build.',
    start: 'Starter', progress: 'Progression', endgame: 'Endgame',
  },
  'zh-CN': {
    builds: 'BD', stages: '阶段', initialBuild: '我的 BD', initialStage: '当前阶段', manage: '管理 BD',
    title: '我的 BD 与阶段', hint: '不同技能玩法独立保存 BD，复制阶段即可规划开荒、过渡与毕业。每个阶段保留自己的天赋、装备、技能和备注。',
    newBuild: '新建 BD', copyStage: '复制当前阶段', renameBuild: '重命名 BD', renameStage: '重命名阶段',
    name: '名称', save: '保存', cancel: '取消', saved: '已保存到本机', failed: '浏览器保存失败，请在关闭页面前下载备份。',
    share: '分享此 BD · 全部阶段', backup: '备份全部 BD', shareHint: 'BD 文件包含所有阶段、加点与备注，可在构筑页导入并加入自己的 BD。下方 PoB 分享码仅分享当前阶段。',
    backupConfirm: '恢复此备份会替换本机全部 BD，请先下载当前备份。是否继续？',
    importHint: '导入会替换当前阶段，保留原方案请先复制阶段。含多个阶段的 BD 分享文件会作为新 BD 加入。',
    start: '开荒', progress: '过渡', endgame: '毕业',
  },
  'zh-TW': {
    builds: 'BD', stages: '階段', initialBuild: '我的 BD', initialStage: '目前階段', manage: '管理 BD',
    title: '我的 BD 與階段', hint: '不同技能玩法獨立儲存 BD，複製階段即可規劃開荒、過渡與畢業。每個階段保留自己的天賦、裝備、技能和備註。',
    newBuild: '新增 BD', copyStage: '複製目前階段', renameBuild: '重新命名 BD', renameStage: '重新命名階段',
    name: '名稱', save: '儲存', cancel: '取消', saved: '已儲存至本機', failed: '瀏覽器儲存失敗，請在關閉頁面前下載備份。',
    share: '分享此 BD · 全部階段', backup: '備份全部 BD', shareHint: 'BD 檔案包含所有階段、加點與備註，可在構築頁匯入並加入自己的 BD。下方 PoB 分享碼僅分享目前階段。',
    backupConfirm: '還原此備份會取代本機全部 BD，請先下載目前備份。是否繼續？',
    importHint: '匯入會取代目前階段，保留原方案請先複製階段。含多個階段的 BD 分享檔案會新增為獨立 BD。',
    start: '開荒', progress: '過渡', endgame: '畢業',
  },
};
export const workspaceText = (lang: Lang) => text[lang];
