export const APP_MESSAGES_CHECKLIST: Record<string, import("./app-messages").AppMessageText> = {
  "checklist.text.invalid": {
    "zh-CN": "清单任务不能为空、不能包含控制字符，且不能超过 500 个字符",
    "zh-TW": "清單任務不能為空、不能包含控制字元，且不能超過 500 個字元",
    en: "A checklist item must be non-empty, contain no control characters, and be at most 500 characters.",
  },
  "checklist.item.not_found": {
    "zh-CN": "找不到该清单任务，可能已被删除",
    "zh-TW": "找不到該清單任務，可能已被刪除",
    en: "That checklist item was not found. It may already have been deleted.",
  },
  "checklist.quadrant.invalid": {
    "zh-CN": "清单象限必须是 1 到 4",
    "zh-TW": "清單象限必須是 1 到 4",
    en: "A checklist item must belong to quadrant 1 through 4.",
  },
};
