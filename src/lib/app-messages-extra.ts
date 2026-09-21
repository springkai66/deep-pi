/**
 * 后端消息分片：提示词增强。
 *
 * 这个模块的源文件是后加的新文件，其原始中文不在 `git diff` 里，
 * 因此条目为按代码场景回填（`prompt_enhance.rs` 中每个消息码的触发条件都很明确）。
 */
export const APP_MESSAGES_EXTRA: Record<string, import("./app-messages").AppMessageText> = {
  // —— 桌宠形象 ——
  "pet.image.unsupported": {
    "zh-CN": "不支持的图片格式，请使用 PNG/JPG/WebP/SVG/GIF",
    "zh-TW": "不支援的圖片格式，請使用 PNG/JPG/WebP/SVG/GIF",
    en: "Unsupported image format. Use PNG/JPG/WebP/SVG/GIF.",
  },
  "pet.image.missing": {
    "zh-CN": "图片文件不存在或无法读取",
    "zh-TW": "圖片檔案不存在或無法讀取",
    en: "The image file does not exist or cannot be read.",
  },
  "pet.image.too_large": {
    "zh-CN": "图片超过 {max} MB，请换一张小一些的",
    "zh-TW": "圖片超過 {max} MB，請換一張小一些的",
    en: "The image is larger than {max} MB. Pick a smaller one.",
  },
  "pet.generate.input_empty": {
    "zh-CN": "请先输入桌宠形象描述",
    "zh-TW": "請先輸入桌寵形象描述",
    en: "Describe the pet appearance first.",
  },
  "pet.generate.input_too_long": {
    "zh-CN": "描述超过 {max} 个字符，请精简后再试",
    "zh-TW": "描述超過 {max} 個字元，請精簡後再試",
    en: "The description exceeds {max} characters. Shorten it and try again.",
  },
  "pet.generate.no_svg": {
    "zh-CN": "模型没有返回可用的 SVG 形象，请换个描述或模型再试",
    "zh-TW": "模型沒有回傳可用的 SVG 形象，請換個描述或模型再試",
    en: "The model did not return a usable SVG pet. Try a different description or model.",
  },
  "pet.generate.response_invalid": {
    "zh-CN": "生成结果无法解析：{error}",
    "zh-TW": "生成結果無法解析：{error}",
    en: "The generated result could not be parsed: {error}",
  },
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
