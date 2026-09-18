/**
 * 后端消息分片：模型设置（官方登录桥、Provider 与凭据校验）。
 *
 * 键是稳定消息码，值是三种语言的文案；`{name}` 为参数占位符，由前端
 * `resolveAppMessage()` 在渲染时替换（见 `app-messages.ts`）。
 */
export const APP_MESSAGES_PROVIDERS: Record<string, import("./app-messages").AppMessageText> = {
  /* —— 官方登录（Pi OAuth 桥） —— */
  "pi.auth.bridge_unavailable": {
    "zh-CN": "官方登录桥接未运行，请重试",
    "zh-TW": "官方登入橋接未執行，請重試",
    en: "The official sign-in bridge is not running. Try again.",
  },
  "pi.auth.internal_unavailable": {
    "zh-CN": "官方登录内部状态不可用，请重启应用后重试",
    "zh-TW": "官方登入內部狀態不可用，請重新啟動應用程式後重試",
    en: "The official sign-in state is unavailable. Restart the app and try again.",
  },
  "pi.auth.request_invalid": {
    "zh-CN": "官方登录请求构造失败：{error}",
    "zh-TW": "官方登入請求建構失敗：{error}",
    en: "Failed to build the official sign-in request: {error}",
  },
  "pi.auth.request_write_failed": {
    "zh-CN": "官方登录请求发送失败：{error}",
    "zh-TW": "官方登入請求傳送失敗：{error}",
    en: "Failed to send the official sign-in request: {error}",
  },
  "pi.auth.bridge_request_failed": {
    "zh-CN": "官方登录桥接返回失败",
    "zh-TW": "官方登入橋接回報失敗",
    en: "The official sign-in bridge reported a failure.",
  },
  "pi.auth.bridge_timeout": {
    "zh-CN": "官方登录桥接响应超时，请重试",
    "zh-TW": "官方登入橋接回應逾時，請重試",
    en: "The official sign-in bridge timed out. Try again.",
  },
  "pi.auth.bridge_exited": {
    "zh-CN": "官方登录桥接已退出，请重试",
    "zh-TW": "官方登入橋接已結束，請重試",
    en: "The official sign-in bridge exited. Try again.",
  },
  "pi.auth.bridge_output_invalid": {
    "zh-CN": "官方登录桥接返回了异常数据",
    "zh-TW": "官方登入橋接回傳了異常資料",
    en: "The official sign-in bridge returned unexpected output.",
  },
  "pi.auth.runtime_missing": {
    "zh-CN": "托管 Pi 运行时缺少 SDK 入口，请在“运行时与更新”里安装或更新 Pi",
    "zh-TW": "託管 Pi 執行環境缺少 SDK 入口，請在「執行環境與更新」安裝或更新 Pi",
    en: "The managed Pi runtime has no SDK entry. Install or update Pi under “Runtime & Updates”.",
  },
  "pi.auth.bridge_install_failed": {
    "zh-CN": "官方登录桥接安装失败：{error}",
    "zh-TW": "官方登入橋接安裝失敗：{error}",
    en: "Failed to install the official sign-in bridge: {error}",
  },
  "pi.auth.bridge_launch_failed": {
    "zh-CN": "官方登录桥接启动失败：{error}",
    "zh-TW": "官方登入橋接啟動失敗：{error}",
    en: "Failed to launch the official sign-in bridge: {error}",
  },
  "pi.auth.bridge_pipe_unavailable": {
    "zh-CN": "官方登录桥接管道不可用",
    "zh-TW": "官方登入橋接管道不可用",
    en: "The official sign-in bridge pipe is unavailable.",
  },
  "pi.auth.providers_invalid": {
    "zh-CN": "官方供应商列表读取失败：{error}",
    "zh-TW": "官方供應商清單讀取失敗：{error}",
    en: "Failed to read the official provider list: {error}",
  },
  "pi.auth.status_invalid": {
    "zh-CN": "官方登录状态读取失败：{error}",
    "zh-TW": "官方登入狀態讀取失敗：{error}",
    en: "Failed to read the official sign-in status: {error}",
  },
  "pi.auth.models_invalid": {
    "zh-CN": "官方模型目录读取失败：{error}",
    "zh-TW": "官方模型目錄讀取失敗：{error}",
    en: "Failed to read the official model catalogue: {error}",
  },
  "pi.auth.worker_failed": {
    "zh-CN": "官方登录服务失败：{error}",
    "zh-TW": "官方登入服務失敗：{error}",
    en: "The official sign-in worker failed: {error}",
  },
  "pi.auth.login_not_acknowledged": {
    "zh-CN": "官方登录未获得桥接确认",
    "zh-TW": "官方登入未取得橋接確認",
    en: "The official sign-in bridge did not acknowledge the login.",
  },
  "pi.auth.login_busy": {
    "zh-CN": "已有官方登录正在进行（{provider}），请先取消或等待完成",
    "zh-TW": "已有官方登入正在進行（{provider}），請先取消或等待完成",
    en: "Another official sign-in is already in progress ({provider}). Cancel it or wait for it to finish.",
  },
  "pi.auth.login_not_owned": {
    "zh-CN": "进行中的官方登录属于 {provider}，与当前请求不匹配",
    "zh-TW": "進行中的官方登入屬於 {provider}，與目前請求不符",
    en: "The official sign-in in progress belongs to {provider}, not to this request.",
  },
  "pi.auth.login_missing": {
    "zh-CN": "当前没有进行中的官方登录",
    "zh-TW": "目前沒有進行中的官方登入",
    en: "No official sign-in is in progress.",
  },
  "pi.auth.bridge_login_busy": {
    "zh-CN": "官方登录桥接中已有登录在进行，请稍后重试",
    "zh-TW": "官方登入橋接中已有登入正在進行，請稍後重試",
    en: "The official sign-in bridge is already handling a login. Try again later.",
  },
  "pi.auth.bridge_prompt_missing": {
    "zh-CN": "该输入提示已失效，请重试",
    "zh-TW": "該輸入提示已失效，請重試",
    en: "That input prompt is no longer valid. Try again.",
  },
  "pi.auth.bridge_login_missing": {
    "zh-CN": "当前没有可取消的官方登录",
    "zh-TW": "目前沒有可取消的官方登入",
    en: "There is no official sign-in to cancel.",
  },
  "pi.auth.bridge_provider_missing": {
    "zh-CN": "缺少供应商标识",
    "zh-TW": "缺少供應商識別",
    en: "The provider id is missing.",
  },
  "pi.auth.bridge_unknown_op": {
    "zh-CN": "官方登录桥接收到未知指令：{op}",
    "zh-TW": "官方登入橋接收到未知指令：{op}",
    en: "The official sign-in bridge received an unknown command: {op}",
  },

  /* —— Provider 与凭据（模型设置） —— */
  "provider.id.invalid": {
    "zh-CN": "标识无效：只能包含字母、数字、连字符和下划线，且不能为空或过长",
    "zh-TW": "識別碼無效：只能包含字母、數字、連字號和底線，且不能為空或過長",
    en: "Invalid id: use letters, digits, “-” and “_” only, and keep it non-empty and within the length limit.",
  },
  "provider.name.invalid": {
    "zh-CN": "名称无效或过长",
    "zh-TW": "名稱無效或過長",
    en: "The name is invalid or too long.",
  },
  "provider.api.invalid": {
    "zh-CN": "API 类型无效或过长",
    "zh-TW": "API 類型無效或過長",
    en: "The API type is invalid or too long.",
  },
  "provider.base_url.invalid": {
    "zh-CN": "Base URL 无效或过长",
    "zh-TW": "Base URL 無效或過長",
    en: "The base URL is invalid or too long.",
  },
  "provider.base_url.unparsable": {
    "zh-CN": "Base URL 解析失败：{error}",
    "zh-TW": "Base URL 解析失敗：{error}",
    en: "Failed to parse the base URL: {error}",
  },
  "provider.base_url.scheme": {
    "zh-CN": "Base URL 必须使用 HTTPS 或回环 HTTP",
    "zh-TW": "Base URL 必須使用 HTTPS 或回送 HTTP",
    en: "The base URL must use HTTPS or loopback HTTP.",
  },
  "provider.base_url.query": {
    "zh-CN": "Base URL 不能包含凭据或查询参数",
    "zh-TW": "Base URL 不能包含憑證或查詢參數",
    en: "The base URL must not contain credentials or a query.",
  },
  "provider.proxy.invalid": {
    "zh-CN": "代理无效或过长",
    "zh-TW": "代理無效或過長",
    en: "The proxy is invalid or too long.",
  },
  "provider.proxy.scheme": {
    "zh-CN": "代理必须使用 HTTPS 或回环 HTTP",
    "zh-TW": "代理必須使用 HTTPS 或回送 HTTP",
    en: "The proxy must use HTTPS or loopback HTTP.",
  },
  "provider.proxy.query": {
    "zh-CN": "代理不能包含凭据或查询参数",
    "zh-TW": "代理不能包含憑證或查詢參數",
    en: "The proxy must not contain credentials or a query.",
  },
  "provider.headers.too_many": {
    "zh-CN": "自定义 Header 数量过多",
    "zh-TW": "自訂 Header 數量過多",
    en: "Too many custom headers.",
  },
  "provider.header.name.invalid": {
    "zh-CN": "Header 名称无效",
    "zh-TW": "Header 名稱無效",
    en: "The header name is invalid.",
  },
  "provider.header.value.invalid": {
    "zh-CN": "Header 值无效",
    "zh-TW": "Header 值無效",
    en: "The header value is invalid.",
  },
  "provider.cost.invalid": {
    "zh-CN": "模型价格必须是非负有限数",
    "zh-TW": "模型價格必須是非負有限數",
    en: "Model prices must be finite and non-negative.",
  },
  "provider.context_window.invalid": {
    "zh-CN": "上下文窗口超出范围",
    "zh-TW": "上下文視窗超出範圍",
    en: "The context window is out of range.",
  },
  "provider.max_tokens.invalid": {
    "zh-CN": "最大输出超出范围",
    "zh-TW": "最大輸出超出範圍",
    en: "The max output is out of range.",
  },
  "provider.input.invalid": {
    "zh-CN": "模型的输入类型无效",
    "zh-TW": "模型的輸入類型無效",
    en: "The model input types are invalid.",
  },
  "provider.levels.invalid": {
    "zh-CN": "模型的思考级别无效",
    "zh-TW": "模型的思考等級無效",
    en: "The model thinking levels are invalid.",
  },
  "provider.levels.duplicate": {
    "zh-CN": "模型的思考级别不能重复",
    "zh-TW": "模型的思考等級不能重複",
    en: "Model thinking levels must be unique.",
  },
  "provider.levels.not_reasoning": {
    "zh-CN": "非推理模型不能设置思考级别",
    "zh-TW": "非推理模型不能設定思考等級",
    en: "Non-reasoning models cannot define thinking levels.",
  },
  "provider.model_api.whitespace": {
    "zh-CN": "模型 API 类型不能包含空白字符",
    "zh-TW": "模型 API 類型不能包含空白字元",
    en: "The model API type must not contain whitespace.",
  },
  "provider.models.too_many": {
    "zh-CN": "模型数量过多",
    "zh-TW": "模型數量過多",
    en: "Too many models.",
  },
  "provider.models.duplicate": {
    "zh-CN": "模型 ID 不能重复",
    "zh-TW": "模型 ID 不能重複",
    en: "Model ids must be unique.",
  },
  "provider.model_id.invalid": {
    "zh-CN": "模型 ID 无效",
    "zh-TW": "模型 ID 無效",
    en: "The model id is invalid.",
  },
  "provider.model_name.invalid": {
    "zh-CN": "模型名称无效或过长",
    "zh-TW": "模型名稱無效或過長",
    en: "The model name is invalid or too long.",
  },
  "provider.model_request.failed": {
    "zh-CN": "拉取供应商模型失败：HTTP {status}",
    "zh-TW": "擷取供應商模型失敗：HTTP {status}",
    en: "Failed to fetch provider models: HTTP {status}",
  },
  "provider.file.root_invalid": {
    "zh-CN": "模型配置文件根节点必须是对象",
    "zh-TW": "模型設定檔根節點必須是物件",
    en: "The model profile root must be an object.",
  },
  "provider.count.too_many": {
    "zh-CN": "Provider 数量过多",
    "zh-TW": "Provider 數量過多",
    en: "Too many providers.",
  },
  "credentials.api_key.invalid": {
    "zh-CN": "API Key 无效或过长",
    "zh-TW": "API Key 無效或過長",
    en: "The API key is invalid or too long.",
  },
  "credentials.read.failed": {
    "zh-CN": "读取 Provider 凭据失败：{error}",
    "zh-TW": "讀取 Provider 憑證失敗：{error}",
    en: "Failed to read the provider credential: {error}",
  },
  "credentials.delete.failed": {
    "zh-CN": "删除 Provider 凭据失败：{error}",
    "zh-TW": "刪除 Provider 憑證失敗：{error}",
    en: "Failed to delete the provider credential: {error}",
  },
  "credentials.api.unsupported": {
    "zh-CN": "不支持的 Provider API 类型：{api}",
    "zh-TW": "不支援的 Provider API 類型：{api}",
    en: "Unsupported provider API type: {api}",
  },
  "provider.proxy.unparsable": {
    "zh-CN": "代理解析失败：{error}",
    "zh-TW": "代理解析失敗：{error}",
    en: "Failed to parse the proxy: {error}",
  },
  "provider.model_response.read_failed": {
    "zh-CN": "读取供应商模型响应失败：{error}",
    "zh-TW": "讀取供應商模型回應失敗：{error}",
    en: "Failed to read the provider model response: {error}",
  },
  "provider.model_response.invalid": {
    "zh-CN": "供应商模型响应不是合法 JSON：{error}",
    "zh-TW": "供應商模型回應不是合法 JSON：{error}",
    en: "The provider model response is not valid JSON: {error}",
  },
  "provider.model_request.error": {
    "zh-CN": "拉取供应商模型失败：{error}",
    "zh-TW": "擷取供應商模型失敗：{error}",
    en: "Failed to fetch provider models: {error}",
  },
  "provider.not_found": {
    "zh-CN": "找不到该 Provider：{provider}",
    "zh-TW": "找不到該 Provider：{provider}",
    en: "Provider not found: {provider}",
  },
  "provider.base_url.missing": {
    "zh-CN": "该 Provider 没有配置 Base URL",
    "zh-TW": "該 Provider 沒有設定 Base URL",
    en: "This provider has no base URL configured.",
  },
  "provider.test_url.invalid": {
    "zh-CN": "模型探活 URL 无效：{error}",
    "zh-TW": "模型探活 URL 無效：{error}",
    en: "The model probe URL is invalid: {error}",
  },
  "provider.connection.failed": {
    "zh-CN": "Provider 连接测试失败：{error}",
    "zh-TW": "Provider 連線測試失敗：{error}",
    en: "Provider connection test failed: {error}",
  },
  "provider.connection.worker_failed": {
    "zh-CN": "Provider 连接测试服务失败：{error}",
    "zh-TW": "Provider 連線測試服務失敗：{error}",
    en: "The provider connection worker failed: {error}",
  },
  "provider.model_test.worker_failed": {
    "zh-CN": "模型连通测试服务失败：{error}",
    "zh-TW": "模型連通測試服務失敗：{error}",
    en: "The model test worker failed: {error}",
  },
  "credentials.not_configured": {
    "zh-CN": "该 Provider 尚未配置凭据",
    "zh-TW": "該 Provider 尚未設定憑證",
    en: "This provider has no credential configured yet.",
  },
  "credentials.open.failed": {
    "zh-CN": "打开 Windows 凭据项失败：{error}",
    "zh-TW": "開啟 Windows 認證項目失敗：{error}",
    en: "Failed to open the Windows Credential Manager entry: {error}",
  },
  "credentials.write.failed": {
    "zh-CN": "写入 Provider 凭据失败：{error}",
    "zh-TW": "寫入 Provider 憑證失敗：{error}",
    en: "Failed to write the provider credential: {error}",
  },
  "credentials.save.failed": {
    "zh-CN": "保存 Provider 凭据失败：{error}",
    "zh-TW": "儲存 Provider 憑證失敗：{error}",
    en: "Failed to save the provider credential: {error}",
  },
};
