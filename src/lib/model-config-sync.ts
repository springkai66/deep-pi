/**
 * 模型配置变更广播：设置页保存 Provider / 凭据后，通知所有打开的对话面板。
 *
 * Pi 在会话创建时一次性加载 models.json（`ModelConfig.load`），RPC 协议里没有任何
 * reload 命令——新模型或新的推理强度档位必须重载会话才能出现在输入框。这里只广播
 * 「配置发生了变更」这个信号；由 ChatPane 决定：空闲时自动重载，忙时提示用户手动重载。
 */

type Listener = () => void;

const listeners = new Set<Listener>();

/** 设置页在 models.json / 凭据实际落盘后调用；异常的监听者不影响其它面板。 */
export function notifyModelsChanged(): void {
  for (const listener of [...listeners]) {
    try {
      listener();
    } catch {
      // 监听者内部错误只影响它自己。
    }
  }
}

/** 订阅变更；返回取消订阅函数。 */
export function onModelsChanged(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
