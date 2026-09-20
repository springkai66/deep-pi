import { t } from "./i18n.svelte";
import { record } from "./rpc-state";

export type MessagePart =
  | { kind: "text" | "thinking" | "unknown"; content: string }
  | { kind: "tool"; content: string; name: string }
  | { kind: "image"; content: string; source: string | null };

function cleanThinkingText(value: string): string {
  return value.replace(/^\s*<thinking>\s*/i, "").replace(/\s*<\/thinking>\s*$/i, "").trim();
}
export function messageParts(content: unknown): MessagePart[] {
  if (typeof content === "string") return [{ kind: "text", content }];
  if (!Array.isArray(content)) return content == null ? [] : [{ kind: "unknown", content: JSON.stringify(content, null, 2) }];
  return content.map((value): MessagePart => {
    const part = record(value);
    if (part.type === "text" && typeof part.text === "string") return { kind: "text", content: part.text };
    if (part.type === "thinking") return { kind: "thinking", content: cleanThinkingText(typeof part.thinking === "string" ? part.thinking : t("思考内容不可用")) };
    if (part.type === "toolCall") return { kind: "tool", name: String(part.name ?? t("工具")), content: JSON.stringify(part.arguments ?? {}, null, 2) };
    if (part.type === "image") {
      const mime = String(part.mimeType ?? "");
      const data = part.data;
      const valid = ["image/png", "image/jpeg", "image/gif", "image/webp"].includes(mime)
        && typeof data === "string" && data.length > 0 && data.length <= 4 * 1024 * 1024
        && data.length % 4 === 0 && /^[A-Za-z0-9+/]*={0,2}$/.test(data);
      return { kind: "image", content: t("[图片 {mime}]", { mime }), source: valid ? `data:${mime};base64,${data}` : null };
    }
    return { kind: "unknown", content: JSON.stringify(value, null, 2) ?? String(value) };
  });
}

export function messageSource(content: unknown): string {
  return messageParts(content).map((part) => part.kind === "tool" ? `${part.name}\n${part.content}` : part.content).join("\n");
}
