/**
 * 后端消息分片：提示词增强。
 *
 * 这个模块的源文件是后加的新文件，其原始中文不在 `git diff` 里，
 * 因此条目为按代码场景回填（`prompt_enhance.rs` 中每个消息码的触发条件都很明确）。
 */
export const APP_MESSAGES_EXTRA: Record<string, import("./app-messages").AppMessageText> = {
  "prompt_enhance.input_empty": {
    "zh-CN": "请先输入要增强的提示词",
    "zh-TW": "請先輸入要增強的提示詞",
    en: "Enter a prompt to enhance first.",
  },
  "prompt_enhance.input_too_long": {
    "zh-CN": "提示词超过 {max} 个字符，请精简后再试",
    "zh-TW": "提示詞超過 {max} 個字元，請精簡後再試",
    en: "The prompt exceeds {max} characters. Shorten it and try again.",
  },
  "prompt_enhance.provider_unconfigured": {
    "zh-CN": "还没有可用的模型，请先到「设置 → 模型设置」里配置",
    "zh-TW": "還沒有可用的模型，請先到「設定 → 模型設定」裡設定",
    en: "No model is available yet. Configure one under Settings → Model settings.",
  },
  "prompt_enhance.base_url_missing": {
    "zh-CN": "所选模型缺少 API 地址，请到「设置 → 模型设置」里补全",
    "zh-TW": "所選模型缺少 API 位址，請到「設定 → 模型設定」裡補齊",
    en: "The selected model has no API base URL. Fill it in under Settings → Model settings.",
  },
  "prompt_enhance.request_failed": {
    "zh-CN": "请求模型失败：{error}",
    "zh-TW": "請求模型失敗：{error}",
    en: "The request to the model failed: {error}",
  },
  "prompt_enhance.http_failed": {
    "zh-CN": "模型返回了 HTTP {status}",
    "zh-TW": "模型回傳了 HTTP {status}",
    en: "The model returned HTTP {status}.",
  },
  "prompt_enhance.response_empty": {
    "zh-CN": "模型没有返回可用的增强结果",
    "zh-TW": "模型沒有回傳可用的增強結果",
    en: "The model did not return a usable enhanced prompt.",
  },
  "prompt_enhance.response_too_long": {
    "zh-CN": "模型返回的内容过长，已放弃使用",
    "zh-TW": "模型回傳的內容過長，已放棄使用",
    en: "The model's response was too long and has been discarded.",
  },
};
