import { describe, expect, it } from "vitest";
import { sessionTitleContext } from "./session-title";
import type { RpcMessage } from "./rpc-state";

const message = (role: string, content: unknown): RpcMessage => ({ role, content } as RpcMessage);

describe("title model context", () => {
  it("keeps user and assistant text, excluding tools and private thinking", () => {
    expect(sessionTitleContext([
      message("user", [{ type: "text", text: "修复启动" }, { type: "image", data: "secret" }]),
      message("assistant", [{ type: "thinking", thinking: "private" }, { type: "text", text: "已找到原因" }]),
      message("toolResult", "ignore"),
    ])).toEqual([{ role: "user", text: "修复启动" }, { role: "assistant", text: "已找到原因" }]);
  });

  it("retains the initial goal and most recent turns within the request limit", () => {
    const context = sessionTitleContext(Array.from({ length: 30 }, (_, index) => message("user", `prompt ${index}`)));
    expect(context).toHaveLength(16);
    expect(context[0].text).toBe("prompt 0");
    expect(context.at(-1)?.text).toBe("prompt 29");
  });

  it("skips empty and unsupported content while bounding individual messages", () => {
    expect(sessionTitleContext([message("user", "  "), message("toolResult", "result")])).toEqual([]);
    expect(sessionTitleContext([message("user", "长".repeat(900))])[0].text).toHaveLength(700);
  });
});
