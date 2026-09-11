const SUPPORTED = ['en', 'ko', 'ja', 'zh-CN', 'zh-TW'];

const EN = Object.freeze({
  'period.month': 'This month', 'period.week': 'This week', 'period.last7': 'Last 7 days', 'period.last30': 'Last 30 days',
  'window.pin': 'Cycle window behavior', 'window.minimize': 'Minimize', 'window.minimizeBubble': 'Minimize to floating bubble', 'window.minimizeTray': 'Minimize to tray',
  'window.quit': 'Quit Token Lens', 'window.floating': 'Floating above apps', 'window.normal': 'Normal window', 'window.expand': 'Expand Token Lens',
  'settings.floatingTray': 'Floating & Tray', 'settings.trayIcon': 'Tray Icon', 'settings.trayDesc': 'Keep Token Lens available from the system tray or menu bar.',
  'settings.bubble': 'Floating Bubble', 'settings.bubbleDesc': 'Use the minimize button to collapse Token Lens into a draggable quota monitor.',
  'settings.openOn': 'Open on', 'settings.click': 'Click', 'settings.hover': 'Hover', 'settings.bubbleDisplay': 'Bubble display', 'settings.bubbleSize': 'Bubble size',
  'settings.bubble.providerLimits': 'Provider limits', 'settings.bubble.icon': 'Icon only', 'settings.bubble.lowestSession': 'Lowest session',
  'settings.bubble.lowestWeekly': 'Lowest weekly', 'settings.bubble.firstTwoBars': 'First two provider bars', 'settings.bubble.lowestRemaining': 'Lowest remaining',
  'settings.bubbleProviders': 'Displayed providers', 'settings.bubbleProvidersDesc': 'Choose any providers to show. Auto keeps the first two available providers.', 'settings.bubbleProvidersAuto': 'Auto',
  'settings.appearance': 'Appearance', 'settings.language': 'Interface language', 'settings.language.auto': 'Auto (system)', 'settings.language.en': 'English',
  'settings.language.ko': '한국어', 'settings.language.ja': '日本語', 'settings.language.zh-CN': '简体中文', 'settings.language.zh-TW': '繁體中文',
  'settings.theme': 'Theme', 'settings.theme.default': 'Default', 'settings.zoom': 'Zoom', 'settings.compactTotal': 'Compact Total', 'settings.compactTotalDesc': 'Show an approximate K/M/B total beside the full token count.',
  'settings.backdrop': 'Windows Backdrop', 'settings.off': 'Off', 'settings.acrylic': 'Acrylic', 'settings.refresh': 'Refresh', 'settings.settings': 'Settings',
  'settings.troubleshooting': 'Troubleshooting', 'settings.errorLogs': 'Diagnostic logs', 'settings.errorLogsDesc': 'Startup timing keeps the latest 10 runs; provider error incidents are kept for up to 3 days.', 'settings.openErrorLogs': 'Open diagnostic log folder', 'settings.errorLogsOpenFailed': 'Could not open the diagnostic log folder.',
  'dashboard.totalTokens': 'TOTAL TOKENS', 'home.models': 'MODELS', 'home.limits': 'LIMITS', 'home.activity': 'ACTIVITY', 'home.trend': 'TREND',
  'home.noModelUsage': 'No model usage', 'home.noLimits': 'No quota available', 'home.loading': 'Loading', 'home.loadingHistory': 'Loading usage history…',
  'home.historyUnavailable': 'Usage history unavailable', 'home.activeDays': '{count} active days', 'home.noHistory': 'No usage history', 'home.peak': 'Peak {value}',
  'common.unavailable': 'Unavailable', 'common.stale': 'Stale', 'common.noQuotaWindows': 'No quota windows', 'common.loading': 'Loading…', 'common.noUsage': 'No usage',
  'common.refreshing': 'Refreshing…', 'common.failedRefresh': 'Failed to refresh usage', 'common.failedSettings': 'Failed to load settings',
  'view.home': 'Home', 'view.tool': 'Tools', 'view.model': 'Models', 'view.session': 'Sessions', 'view.limits': 'Limits', 'view.choose': 'Choose view',
  'limits.home': 'Home', 'limits.showOnHome': 'Show this quota on Home', 'limits.homeAuto': 'Home: Auto',
  'filter.provider': 'Provider filter', 'filter.all': 'All', 'filter.providersCount': '{count} providers',
  'session.back': '‹ Sessions', 'session.notFound': 'Session detail not found on this machine.', 'session.noActivity': 'No activity in this period.',
  'session.noSessionUsage': 'No session usage', 'session.mostTokens': '↕ Most tokens', 'session.newest': '↕ Newest',
  'session.messages': '{count} msgs', 'session.turn': 'AI {label}', 'session.split': 'in {input} · out {output} · cache {cache}{reason}', 'session.reason': ' · reason {value}',
  'quota.usageCredits': 'Usage credits', 'quota.fiveHour': '5-hour', 'quota.weekly': 'Weekly', 'quota.monthly': 'Monthly', 'quota.quota': 'Quota', 'quota.additional': 'Additional',
  'quota.left': '{value} left', 'quota.resets': 'Resets', 'quota.credits': '{value} credits', 'quota.rateReset': 'Rate-limit reset', 'quota.available': '{count} available',
  'history.tokens': '{value} tokens',
});

const KO = Object.freeze({
  'period.month': '이번 달', 'period.week': '이번 주', 'period.last7': '최근 7일', 'period.last30': '최근 30일',
  'window.pin': '창 표시 방식 전환', 'window.minimize': '최소화', 'window.minimizeBubble': '플로팅 버블로 최소화', 'window.minimizeTray': '트레이로 최소화',
  'window.quit': 'Token Lens 종료', 'window.floating': '항상 위', 'window.normal': '일반 창', 'window.expand': 'Token Lens 열기',
  'settings.floatingTray': '플로팅 및 트레이', 'settings.trayIcon': '트레이 아이콘', 'settings.trayDesc': '시스템 트레이에서 Token Lens를 사용할 수 있게 유지합니다.',
  'settings.bubble': '플로팅 버블', 'settings.bubbleDesc': '최소화 버튼을 누르면 Token Lens를 드래그 가능한 한도 모니터로 축소합니다.',
  'settings.openOn': '열기 방식', 'settings.click': '클릭', 'settings.hover': '마우스 오버', 'settings.bubbleDisplay': '버블 표시', 'settings.bubbleSize': '버블 크기',
  'settings.bubble.providerLimits': 'Provider 한도', 'settings.bubble.icon': '아이콘만', 'settings.bubble.lowestSession': '가장 낮은 세션 한도',
  'settings.bubble.lowestWeekly': '가장 낮은 주간 한도', 'settings.bubble.firstTwoBars': '앞의 두 Provider 바', 'settings.bubble.lowestRemaining': '가장 낮은 잔여 한도',
  'settings.bubbleProviders': '표시할 Provider', 'settings.bubbleProvidersDesc': '표시할 Provider를 원하는 만큼 선택합니다. 자동은 사용 가능한 앞의 두 Provider를 표시합니다.', 'settings.bubbleProvidersAuto': '자동',
  'settings.appearance': '화면', 'settings.language': '인터페이스 언어', 'settings.language.auto': '자동 (시스템 설정)', 'settings.language.en': 'English',
  'settings.language.ko': '한국어', 'settings.language.ja': '日本語', 'settings.language.zh-CN': '简体中文', 'settings.language.zh-TW': '繁體中文',
  'settings.theme': '테마', 'settings.theme.default': '기본', 'settings.zoom': '확대/축소', 'settings.compactTotal': '간략 합계', 'settings.compactTotalDesc': '전체 토큰 수 옆에 K/M/B 단위의 간략한 값을 표시합니다.',
  'settings.backdrop': 'Windows 배경 효과', 'settings.off': '끄기', 'settings.acrylic': 'Acrylic', 'settings.refresh': '새로고침', 'settings.settings': '설정',
  'settings.troubleshooting': '문제 해결', 'settings.errorLogs': '진단 로그', 'settings.errorLogsDesc': '시작 시간 기록은 최근 10회, Provider 오류 기록은 최대 3일 동안 보관합니다.', 'settings.openErrorLogs': '진단 로그 폴더 열기', 'settings.errorLogsOpenFailed': '진단 로그 폴더를 열 수 없습니다.',
  'dashboard.totalTokens': '전체 토큰', 'home.models': '모델', 'home.limits': '한도', 'home.activity': '활동', 'home.trend': '추세',
  'home.noModelUsage': '모델 사용량 없음', 'home.noLimits': '표시할 한도 없음', 'home.loading': '불러오는 중', 'home.loadingHistory': '사용 기록을 불러오는 중…',
  'home.historyUnavailable': '사용 기록을 불러올 수 없음', 'home.activeDays': '활동일 {count}일', 'home.noHistory': '사용 기록 없음', 'home.peak': '최고 {value}',
  'common.unavailable': '사용 불가', 'common.stale': '이전 값', 'common.noQuotaWindows': '표시할 한도 구간 없음', 'common.loading': '불러오는 중…', 'common.noUsage': '사용량 없음',
  'common.refreshing': '새로고침 중…', 'common.failedRefresh': '사용량을 새로고침하지 못했습니다', 'common.failedSettings': '설정을 불러오지 못했습니다',
  'view.home': '홈', 'view.tool': '도구', 'view.model': '모델', 'view.session': '세션', 'view.limits': '한도', 'view.choose': '화면 선택',
  'limits.home': '홈', 'limits.showOnHome': '이 한도를 홈에 표시', 'limits.homeAuto': '홈: 자동',
  'filter.provider': 'Provider 필터', 'filter.all': '전체', 'filter.providersCount': 'Provider {count}개',
  'session.back': '‹ 세션', 'session.notFound': '이 PC에서 세션 상세 정보를 찾을 수 없습니다.', 'session.noActivity': '이 기간의 활동이 없습니다.',
  'session.noSessionUsage': '세션 사용량 없음', 'session.mostTokens': '↕ 토큰 많은 순', 'session.newest': '↕ 최신 순',
  'session.messages': '{count}개 메시지', 'session.turn': 'AI {label}', 'session.split': '입력 {input} · 출력 {output} · 캐시 {cache}{reason}', 'session.reason': ' · 추론 {value}',
  'quota.usageCredits': '사용 크레딧', 'quota.fiveHour': '5시간', 'quota.weekly': '주간', 'quota.monthly': '월간', 'quota.quota': '한도', 'quota.additional': '추가 한도',
  'quota.left': '{value} 남음', 'quota.resets': '초기화', 'quota.credits': '{value} credits', 'quota.rateReset': 'Rate-limit 초기화', 'quota.available': '{count}개 사용 가능',
  'history.tokens': '{value} 토큰',
});

const JA = Object.freeze({
  'period.month':'今月','period.week':'今週','period.last7':'直近7日','period.last30':'直近30日',
  'window.pin':'ウィンドウ表示を切り替え','window.minimize':'最小化','window.minimizeBubble':'フローティングバブルに最小化','window.minimizeTray':'トレイに最小化','window.quit':'Token Lensを終了','window.floating':'常に手前に表示','window.normal':'通常のウィンドウ','window.expand':'Token Lensを開く',
  'settings.floatingTray':'フローティングとトレイ','settings.trayIcon':'トレイアイコン','settings.trayDesc':'システムトレイからToken Lensを利用できるようにします。','settings.bubble':'フローティングバブル','settings.bubbleDesc':'最小化ボタンでToken Lensをドラッグ可能なクォータモニターに縮小します。','settings.openOn':'開く操作','settings.click':'クリック','settings.hover':'ホバー','settings.bubbleDisplay':'バブル表示','settings.bubbleSize':'バブルサイズ','settings.bubble.providerLimits':'Provider制限','settings.bubble.icon':'アイコンのみ','settings.bubble.lowestSession':'最小セッション制限','settings.bubble.lowestWeekly':'最小週間制限','settings.bubble.firstTwoBars':'先頭2 Providerのバー','settings.bubble.lowestRemaining':'最小残量','settings.bubbleProviders':'表示するProvider','settings.bubbleProvidersDesc':'表示するProviderを選択します。自動では利用可能な先頭2件を表示します。','settings.bubbleProvidersAuto':'自動','settings.appearance':'外観','settings.language':'インターフェース言語','settings.language.auto':'自動（システム設定）','settings.language.en':'English','settings.language.ko':'한국어','settings.language.ja':'日本語','settings.language.zh-CN':'简体中文','settings.language.zh-TW':'繁體中文','settings.theme':'テーマ','settings.theme.default':'デフォルト','settings.zoom':'ズーム','settings.compactTotal':'合計を短縮表示','settings.compactTotalDesc':'合計トークン数の横にK/M/B形式の概算値を表示します。','settings.backdrop':'Windows背景効果','settings.off':'オフ','settings.acrylic':'Acrylic','settings.refresh':'更新','settings.settings':'設定','settings.troubleshooting':'トラブルシューティング','settings.errorLogs':'診断ログ','settings.errorLogsDesc':'起動タイミングは直近10回、Providerエラー記録は最大3日間保持します。','settings.openErrorLogs':'診断ログフォルダーを開く','settings.errorLogsOpenFailed':'診断ログフォルダーを開けませんでした。',
  'dashboard.totalTokens':'合計トークン','home.models':'モデル','home.limits':'制限','home.activity':'アクティビティ','home.trend':'トレンド','home.noModelUsage':'モデル使用量なし','home.noLimits':'表示できる制限なし','home.loading':'読み込み中','home.loadingHistory':'使用履歴を読み込み中…','home.historyUnavailable':'使用履歴を取得できません','home.activeDays':'アクティブ {count} 日','home.noHistory':'使用履歴なし','home.peak':'ピーク {value}',
  'common.unavailable':'利用不可','common.stale':'古い値','common.noQuotaWindows':'表示できるクォータ期間なし','common.loading':'読み込み中…','common.noUsage':'使用量なし','common.refreshing':'更新中…','common.failedRefresh':'使用量を更新できませんでした','common.failedSettings':'設定を読み込めませんでした',
  'view.home':'ホーム','view.tool':'ツール','view.model':'モデル','view.session':'セッション','view.limits':'制限','view.choose':'表示を選択',
  'limits.home':'ホーム','limits.showOnHome':'このクォータをホームに表示','limits.homeAuto':'ホーム: 自動',
  'filter.provider':'Providerフィルター','filter.all':'すべて','filter.providersCount':'Provider {count}件',
  'session.back':'‹ セッション','session.notFound':'このPCでセッション詳細が見つかりません。','session.noActivity':'この期間のアクティビティはありません。','session.noSessionUsage':'セッション使用量なし','session.mostTokens':'↕ トークン数順','session.newest':'↕ 新しい順','session.messages':'{count}件のメッセージ','session.turn':'AI {label}','session.split':'入力 {input} · 出力 {output} · キャッシュ {cache}{reason}','session.reason':' · 推論 {value}',
  'quota.usageCredits':'使用クレジット','quota.fiveHour':'5時間','quota.weekly':'週間','quota.monthly':'月間','quota.quota':'クォータ','quota.additional':'追加制限','quota.left':'残り {value}','quota.resets':'リセット','quota.credits':'{value} credits','quota.rateReset':'Rate-limitリセット','quota.available':'{count}件利用可能','history.tokens':'{value} トークン',
});
const ZH_CN = Object.freeze({
  'period.month':'本月','period.week':'本周','period.last7':'最近 7 天','period.last30':'最近 30 天',
  'window.pin':'切换窗口显示方式','window.minimize':'最小化','window.minimizeBubble':'最小化为浮动气泡','window.minimizeTray':'最小化到托盘','window.quit':'退出 Token Lens','window.floating':'始终置顶','window.normal':'普通窗口','window.expand':'打开 Token Lens',
  'settings.floatingTray':'浮动与托盘','settings.trayIcon':'托盘图标','settings.trayDesc':'让 Token Lens 可从系统托盘访问。','settings.bubble':'浮动气泡','settings.bubbleDesc':'使用最小化按钮将 Token Lens 收起为可拖动的额度监视器。','settings.openOn':'打开方式','settings.click':'点击','settings.hover':'悬停','settings.bubbleDisplay':'气泡显示','settings.bubbleSize':'气泡大小','settings.bubble.providerLimits':'Provider 额度','settings.bubble.icon':'仅图标','settings.bubble.lowestSession':'最低会话额度','settings.bubble.lowestWeekly':'最低周额度','settings.bubble.firstTwoBars':'前两个 Provider 条','settings.bubble.lowestRemaining':'最低剩余额度','settings.bubbleProviders':'显示的 Provider','settings.bubbleProvidersDesc':'选择要显示的 Provider。自动模式显示前两个可用 Provider。','settings.bubbleProvidersAuto':'自动','settings.appearance':'外观','settings.language':'界面语言','settings.language.auto':'自动（跟随系统）','settings.language.en':'English','settings.language.ko':'한국어','settings.language.ja':'日本語','settings.language.zh-CN':'简体中文','settings.language.zh-TW':'繁體中文','settings.theme':'主题','settings.theme.default':'默认','settings.zoom':'缩放','settings.compactTotal':'简略总计','settings.compactTotalDesc':'在完整 Token 数旁显示 K/M/B 格式的近似值。','settings.backdrop':'Windows 背景效果','settings.off':'关闭','settings.acrylic':'Acrylic','settings.refresh':'刷新','settings.settings':'设置','settings.troubleshooting':'故障排查','settings.errorLogs':'诊断日志','settings.errorLogsDesc':'启动计时保留最近 10 次，Provider 错误记录最长保留 3 天。','settings.openErrorLogs':'打开诊断日志文件夹','settings.errorLogsOpenFailed':'无法打开诊断日志文件夹。',
  'dashboard.totalTokens':'总 Token','home.models':'模型','home.limits':'额度','home.activity':'活动','home.trend':'趋势','home.noModelUsage':'暂无模型使用量','home.noLimits':'暂无可显示额度','home.loading':'加载中','home.loadingHistory':'正在加载使用记录…','home.historyUnavailable':'无法获取使用记录','home.activeDays':'活跃 {count} 天','home.noHistory':'暂无使用记录','home.peak':'峰值 {value}',
  'common.unavailable':'不可用','common.stale':'旧数据','common.noQuotaWindows':'暂无可显示额度周期','common.loading':'加载中…','common.noUsage':'暂无使用量','common.refreshing':'刷新中…','common.failedRefresh':'无法刷新使用量','common.failedSettings':'无法加载设置',
  'view.home':'主页','view.tool':'工具','view.model':'模型','view.session':'会话','view.limits':'额度','view.choose':'选择视图',
  'limits.home':'主页','limits.showOnHome':'在主页显示此额度','limits.homeAuto':'主页：自动',
  'filter.provider':'Provider 筛选','filter.all':'全部','filter.providersCount':'{count} 个 Provider',
  'session.back':'‹ 会话','session.notFound':'在此电脑上找不到会话详情。','session.noActivity':'此时间段没有活动。','session.noSessionUsage':'暂无会话使用量','session.mostTokens':'↕ Token 最多','session.newest':'↕ 最新','session.messages':'{count} 条消息','session.turn':'AI {label}','session.split':'输入 {input} · 输出 {output} · 缓存 {cache}{reason}','session.reason':' · 推理 {value}',
  'quota.usageCredits':'使用额度','quota.fiveHour':'5 小时','quota.weekly':'每周','quota.monthly':'每月','quota.quota':'额度','quota.additional':'附加额度','quota.left':'剩余 {value}','quota.resets':'重置','quota.credits':'{value} credits','quota.rateReset':'Rate-limit 重置','quota.available':'可用 {count} 次','history.tokens':'{value} Token',
});
const ZH_TW = Object.freeze({
  'period.month':'本月','period.week':'本週','period.last7':'最近 7 天','period.last30':'最近 30 天',
  'window.pin':'切換視窗顯示方式','window.minimize':'最小化','window.minimizeBubble':'最小化為浮動氣泡','window.minimizeTray':'最小化到系統匣','window.quit':'結束 Token Lens','window.floating':'永遠置頂','window.normal':'一般視窗','window.expand':'開啟 Token Lens',
  'settings.floatingTray':'浮動與系統匣','settings.trayIcon':'系統匣圖示','settings.trayDesc':'讓 Token Lens 可從系統匣存取。','settings.bubble':'浮動氣泡','settings.bubbleDesc':'使用最小化按鈕將 Token Lens 收合為可拖曳的額度監視器。','settings.openOn':'開啟方式','settings.click':'點擊','settings.hover':'懸停','settings.bubbleDisplay':'氣泡顯示','settings.bubbleSize':'氣泡大小','settings.bubble.providerLimits':'Provider 額度','settings.bubble.icon':'僅圖示','settings.bubble.lowestSession':'最低工作階段額度','settings.bubble.lowestWeekly':'最低每週額度','settings.bubble.firstTwoBars':'前兩個 Provider 長條','settings.bubble.lowestRemaining':'最低剩餘額度','settings.bubbleProviders':'顯示的 Provider','settings.bubbleProvidersDesc':'選擇要顯示的 Provider。自動模式顯示前兩個可用 Provider。','settings.bubbleProvidersAuto':'自動','settings.appearance':'外觀','settings.language':'介面語言','settings.language.auto':'自動（跟隨系統）','settings.language.en':'English','settings.language.ko':'한국어','settings.language.ja':'日本語','settings.language.zh-CN':'简体中文','settings.language.zh-TW':'繁體中文','settings.theme':'主題','settings.theme.default':'預設','settings.zoom':'縮放','settings.compactTotal':'簡略總計','settings.compactTotalDesc':'在完整 Token 數旁顯示 K/M/B 格式的近似值。','settings.backdrop':'Windows 背景效果','settings.off':'關閉','settings.acrylic':'Acrylic','settings.refresh':'重新整理','settings.settings':'設定','settings.troubleshooting':'疑難排解','settings.errorLogs':'診斷日誌','settings.errorLogsDesc':'啟動計時保留最近 10 次，Provider 錯誤記錄最長保留 3 天。','settings.openErrorLogs':'開啟診斷日誌資料夾','settings.errorLogsOpenFailed':'無法開啟診斷日誌資料夾。',
  'dashboard.totalTokens':'總 Token','home.models':'模型','home.limits':'額度','home.activity':'活動','home.trend':'趨勢','home.noModelUsage':'沒有模型使用量','home.noLimits':'沒有可顯示額度','home.loading':'載入中','home.loadingHistory':'正在載入使用記錄…','home.historyUnavailable':'無法取得使用記錄','home.activeDays':'活躍 {count} 天','home.noHistory':'沒有使用記錄','home.peak':'峰值 {value}',
  'common.unavailable':'無法使用','common.stale':'舊資料','common.noQuotaWindows':'沒有可顯示額度週期','common.loading':'載入中…','common.noUsage':'沒有使用量','common.refreshing':'重新整理中…','common.failedRefresh':'無法重新整理使用量','common.failedSettings':'無法載入設定',
  'view.home':'首頁','view.tool':'工具','view.model':'模型','view.session':'工作階段','view.limits':'額度','view.choose':'選擇檢視',
  'limits.home':'首頁','limits.showOnHome':'在首頁顯示此額度','limits.homeAuto':'首頁：自動',
  'filter.provider':'Provider 篩選','filter.all':'全部','filter.providersCount':'{count} 個 Provider',
  'session.back':'‹ 工作階段','session.notFound':'在此電腦上找不到工作階段詳細資料。','session.noActivity':'此期間沒有活動。','session.noSessionUsage':'沒有工作階段使用量','session.mostTokens':'↕ Token 最多','session.newest':'↕ 最新','session.messages':'{count} 則訊息','session.turn':'AI {label}','session.split':'輸入 {input} · 輸出 {output} · 快取 {cache}{reason}','session.reason':' · 推理 {value}',
  'quota.usageCredits':'使用額度','quota.fiveHour':'5 小時','quota.weekly':'每週','quota.monthly':'每月','quota.quota':'額度','quota.additional':'附加額度','quota.left':'剩餘 {value}','quota.resets':'重設','quota.credits':'{value} credits','quota.rateReset':'Rate-limit 重設','quota.available':'可用 {count} 次','history.tokens':'{value} Token',
});

const DICTIONARIES = Object.freeze({ en: EN, ko: KO, ja: JA, 'zh-CN': ZH_CN, 'zh-TW': ZH_TW });

export function normalizeLanguage(value) {
  const raw = String(value || '').trim();
  return raw === 'auto' || SUPPORTED.includes(raw) ? raw : 'auto';
}

function languageFromLocale(value) {
  const raw = String(value || '').trim().replace('_', '-');
  const lower = raw.toLowerCase();
  if (lower.startsWith('ko')) return 'ko';
  if (lower.startsWith('ja')) return 'ja';
  if (lower.startsWith('zh')) {
    return /(?:tw|hk|mo|hant)/i.test(raw) ? 'zh-TW' : 'zh-CN';
  }
  return 'en';
}

export function resolveLanguage(setting, preferred = []) {
  const normalized = normalizeLanguage(setting);
  if (normalized !== 'auto') return normalized;
  const candidates = Array.isArray(preferred) && preferred.length ? preferred : ['en'];
  return languageFromLocale(candidates[0]);
}

export function translate(language, key, params = {}) {
  const resolved = SUPPORTED.includes(language) ? language : 'en';
  let text = DICTIONARIES[resolved]?.[key] ?? EN[key] ?? key;
  for (const [name, value] of Object.entries(params)) {
    text = text.replaceAll(`{${name}}`, String(value));
  }
  return text;
}

export function applyTranslations(root, language) {
  for (const element of root.querySelectorAll('[data-i18n]')) {
    element.textContent = translate(language, element.dataset.i18n);
  }
  for (const element of root.querySelectorAll('[data-i18n-title]')) {
    element.title = translate(language, element.dataset.i18nTitle);
  }
  for (const element of root.querySelectorAll('[data-i18n-aria-label]')) {
    element.setAttribute('aria-label', translate(language, element.dataset.i18nAriaLabel));
  }
}

export const LANGUAGE_OPTIONS = Object.freeze(['auto', 'en', 'ko', 'ja', 'zh-CN', 'zh-TW']);
