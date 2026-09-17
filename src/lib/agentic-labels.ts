/**
 * agenticskills.io 目录里的结构化词汇本地化。
 *
 * 站点数据是英文（分类、难度、传输、平台、信任等级、价格），这里保存唯一一份映射表：
 *  - `zh-CN` / `zh-TW` 各自写本地区用语（如「数据库」/「資料庫」），不走简繁字符转换，
 *    避免两岸术语被逐字直译；
 *  - `en` 直接返回站点原文，保证英文界面与站点完全一致；
 *  - 表里没有的值**原样返回**，站点新增词汇时不会丢字。
 *
 * 只处理结构化词汇：自由文本（描述、步骤正文）由翻译命令处理，不要放进这张表。
 */

import type { Locale } from "./locale";

/** 一个固定词条的中文写法；英文即词条原文。 */
interface LocalizedTerm {
  "zh-CN": string;
  "zh-TW": string;
}

/** 站点分类（全量 37 个）。 */
export const AGENTIC_CATEGORY_LABELS: Record<string, LocalizedTerm> = {
  "DevOps & Infrastructure": { "zh-CN": "DevOps 与基础设施", "zh-TW": "DevOps 與基礎設施" },
  "Databases & Data": { "zh-CN": "数据库与数据", "zh-TW": "資料庫與資料" },
  "Design & UI/UX": { "zh-CN": "设计与 UI/UX", "zh-TW": "設計與 UI/UX" },
  "AI/ML Development": { "zh-CN": "AI/ML 开发", "zh-TW": "AI/ML 開發" },
  "Code Quality & Testing": { "zh-CN": "代码质量与测试", "zh-TW": "程式碼品質與測試" },
  "Developer Tools": { "zh-CN": "开发者工具", "zh-TW": "開發者工具" },
  "Content & Marketing": { "zh-CN": "内容与营销", "zh-TW": "內容與行銷" },
  Productivity: { "zh-CN": "效率工具", "zh-TW": "生產力工具" },
  "Web Search & Browsing": { "zh-CN": "网页搜索与浏览", "zh-TW": "網頁搜尋與瀏覽" },
  Security: { "zh-CN": "安全", "zh-TW": "安全性" },
  "Web Development": { "zh-CN": "Web 开发", "zh-TW": "Web 開發" },
  "Development": { "zh-CN": "开发", "zh-TW": "開發" },
  "DevOps & Security": { "zh-CN": "DevOps 与安全", "zh-TW": "DevOps 與安全" },
  "SEO & Growth": { "zh-CN": "SEO 与增长", "zh-TW": "SEO 與成長" },
  "Productivity & PM": { "zh-CN": "效率与项目管理", "zh-TW": "生產力與專案管理" },
  "Communication & Email": { "zh-CN": "通信与邮件", "zh-TW": "通訊與電子郵件" },
  "AI & Machine Learning": { "zh-CN": "AI 与机器学习", "zh-TW": "AI 與機器學習" },
  "Finance & Payments": { "zh-CN": "金融与支付", "zh-TW": "金融與支付" },
  "Analytics & Monitoring": { "zh-CN": "分析与监控", "zh-TW": "分析與監控" },
  "Mobile Development": { "zh-CN": "移动开发", "zh-TW": "行動開發" },
  "Backend & APIs": { "zh-CN": "后端与 API", "zh-TW": "後端與 API" },
  "Cloud & Infrastructure": { "zh-CN": "云与基础设施", "zh-TW": "雲端與基礎設施" },
  "Utilities & General": { "zh-CN": "实用工具与通用", "zh-TW": "實用工具與通用" },
  "Document Creation": { "zh-CN": "文档生成", "zh-TW": "文件建立" },
  Database: { "zh-CN": "数据库", "zh-TW": "資料庫" },
  "File Systems & Storage": { "zh-CN": "文件系统与存储", "zh-TW": "檔案系統與儲存" },
  "Content & CMS": { "zh-CN": "内容与 CMS", "zh-TW": "內容與 CMS" },
  "Social Media": { "zh-CN": "社交媒体", "zh-TW": "社群媒體" },
  "Knowledge & Memory": { "zh-CN": "知识与记忆", "zh-TW": "知識與記憶" },
  "Aggregators & Platforms": { "zh-CN": "聚合器与平台", "zh-TW": "聚合器與平台" },
  "Agent Architecture": { "zh-CN": "智能体架构", "zh-TW": "代理架構" },
  "Design & Creative": { "zh-CN": "设计与创意", "zh-TW": "設計與創意" },
  "Security & Identity": { "zh-CN": "安全与身份", "zh-TW": "安全性與身分" },
  "Healthcare & Bio": { "zh-CN": "医疗与生物", "zh-TW": "醫療與生技" },
  "Data Science": { "zh-CN": "数据科学", "zh-TW": "資料科學" },
  "CRM & Sales": { "zh-CN": "CRM 与销售", "zh-TW": "CRM 與銷售" },
  "Maps & Location": { "zh-CN": "地图与定位", "zh-TW": "地圖與定位" },
  "Home & IoT": { "zh-CN": "家居与 IoT", "zh-TW": "居家與 IoT" },
  "Code Execution": { "zh-CN": "代码执行", "zh-TW": "程式碼執行" },
};

/** 工作流难度。 */
export const AGENTIC_LEVEL_LABELS: Record<string, LocalizedTerm> = {
  Beginner: { "zh-CN": "入门", "zh-TW": "入門" },
  Intermediate: { "zh-CN": "中级", "zh-TW": "中級" },
  Advanced: { "zh-CN": "高级", "zh-TW": "進階" },
};

/** 传输方式；协议名（stdio / SSE / HTTP）保持原文，只翻译修饰语。 */
export const AGENTIC_TRANSPORT_LABELS: Record<string, LocalizedTerm> = {
  "Streamable HTTP": { "zh-CN": "流式 HTTP", "zh-TW": "串流 HTTP" },
  stdio: { "zh-CN": "stdio", "zh-TW": "stdio" },
  SSE: { "zh-CN": "SSE", "zh-TW": "SSE" },
  HTTP: { "zh-CN": "HTTP", "zh-TW": "HTTP" },
};

/** 支持的客户端平台。 */
export const AGENTIC_PLATFORM_LABELS: Record<string, LocalizedTerm> = {
  "claude-code": { "zh-CN": "Claude Code", "zh-TW": "Claude Code" },
  codex: { "zh-CN": "Codex", "zh-TW": "Codex" },
  cursor: { "zh-CN": "Cursor", "zh-TW": "Cursor" },
  "multi-platform": { "zh-CN": "多平台", "zh-TW": "多平台" },
  "github-copilot": { "zh-CN": "GitHub Copilot", "zh-TW": "GitHub Copilot" },
  "gemini-cli": { "zh-CN": "Gemini CLI", "zh-TW": "Gemini CLI" },
  "claude-ai": { "zh-CN": "Claude 网页版", "zh-TW": "Claude 網頁版" },
  windsurf: { "zh-CN": "Windsurf", "zh-TW": "Windsurf" },
};

/** 信任等级。 */
export const AGENTIC_TRUST_LABELS: Record<string, LocalizedTerm> = {
  official: { "zh-CN": "官方", "zh-TW": "官方" },
  community: { "zh-CN": "社区", "zh-TW": "社群" },
  verified: { "zh-CN": "已验证", "zh-TW": "已驗證" },
};

/** 价格口径（站点目前实测只有 open-source）。 */
export const AGENTIC_PRICE_LABELS: Record<string, LocalizedTerm> = {
  "open-source": { "zh-CN": "开源", "zh-TW": "開源" },
  free: { "zh-CN": "免费", "zh-TW": "免費" },
};

/**
 * 反向查找：把某个语言下的展示名还原成站点原文（英文键）。
 *
 * 用途是搜索——界面显示中文分类，但站点数据是英文，用户输入「数据库」时
 * 必须能匹配到 `Databases & Data`。中英文都查：`AGENTIC_TERM_SEARCH_ALIASES`
 * 会把原值与它在该语言下的展示名都收进来。
 */
export function agenticTermAliases(
  table: Record<string, LocalizedTerm>,
  value: string | null | undefined,
  locale: Locale,
): string[] {
  if (value === null || value === undefined) return [];
  const trimmed = value.trim();
  const aliases = new Set<string>([value, trimmed]);
  const match = table[trimmed];
  if (match) {
    // 原文与三种语言的展示名都作为可搜索别名。
    aliases.add(match["zh-CN"]);
    aliases.add(match["zh-TW"]);
  }
  if (locale !== "en") aliases.add(termLabel(table, value, locale));
  return [...aliases].filter((alias) => alias.length > 0);
}

/** 把「中文展示名」反查回站点原文；命中返回原文，未命中返回 null。 */
export function agenticTermOrigin(
  table: Record<string, LocalizedTerm>,
  value: string | null | undefined,
): string | null {
  if (value === null || value === undefined) return null;
  const trimmed = value.trim();
  if (table[trimmed]) return trimmed;
  const found = Object.keys(table).find(
    (key) => table[key]["zh-CN"] === trimmed || table[key]["zh-TW"] === trimmed,
  );
  return found ?? null;
}

/**
 * 查表：命中返回对应语言的中文，未命中或空值原样返回（英文直接返回原文）。
 * 词条按去掉首尾空白后的原文匹配，避免站点多打空格时漏译。
 */
function termLabel(
  table: Record<string, LocalizedTerm>,
  value: string | null | undefined,
  locale: Locale,
): string {
  if (value === null || value === undefined) return "";
  if (locale === "en") return value;
  const match = table[value.trim()];
  return match ? match[locale] : value;
}

/** 分类展示名；空值返回空串，未知分类原样返回。 */
export function agenticCategoryLabel(category: string | null | undefined, locale: Locale): string {
  return termLabel(AGENTIC_CATEGORY_LABELS, category, locale);
}

/** 难度展示名；空值返回空串，未知难度原样返回。 */
export function agenticLevelLabel(level: string | null | undefined, locale: Locale): string {
  return termLabel(AGENTIC_LEVEL_LABELS, level, locale);
}

/** 传输方式展示名；未知取值原样返回。 */
export function agenticTransportLabel(value: string, locale: Locale): string {
  return termLabel(AGENTIC_TRANSPORT_LABELS, value, locale);
}

/** 平台展示名；未知取值原样返回。 */
export function agenticPlatformLabel(value: string, locale: Locale): string {
  return termLabel(AGENTIC_PLATFORM_LABELS, value, locale);
}

/** 信任等级展示名；未知取值原样返回。 */
export function agenticTrustLabel(value: string, locale: Locale): string {
  return termLabel(AGENTIC_TRUST_LABELS, value, locale);
}

/** 价格口径展示名；未知取值原样返回。 */
export function agenticPriceLabel(value: string, locale: Locale): string {
  return termLabel(AGENTIC_PRICE_LABELS, value, locale);
}
