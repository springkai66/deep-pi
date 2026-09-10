import { describe, expect, it } from "vitest";
import { diffScrollPosition, parseUnifiedDiff } from "./git-diff";

describe("unified diff display", () => {
  it("numbers old and new lines across multiple hunks", () => {
    const rows = parseUnifiedDiff("diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -2,2 +2,3 @@\n same\n-old\n+new\n+extra\n@@ -8 +9 @@\n-last\n+final\n");
    expect(rows.filter((row) => row.oldLine !== null || row.newLine !== null).map(({kind,oldLine,newLine}) => [kind,oldLine,newLine]))
      .toEqual([["context",2,2],["delete",3,null],["add",null,3],["add",null,4],["delete",8,null],["add",null,9]]);
  });

  it("does not mistake patch-looking file content for metadata", () => {
    const rows = parseUnifiedDiff("@@ -0,0 +1,2 @@\n+++payload\n+<script>alert(1)</script>\n\\ No newline at end of file\n");
    expect(rows[1]).toMatchObject({kind:"add",text:"++payload",newLine:1});
    expect(rows[2]).toMatchObject({kind:"add",text:"<script>alert(1)</script>",newLine:2});
    expect(rows[3].kind).toBe("note");
  });

  it("keeps rename, empty-file metadata and truncated hunks visible", () => {
    expect(parseUnifiedDiff("new file mode 100644\n").map((row) => row.kind)).toEqual(["meta"]);
    const rows = parseUnifiedDiff("rename from old\nrename to new\n@@ -1,2 +1,2 @@\n-old\n+partial");
    expect(rows[0].text).toBe("rename from old");
    expect(rows.at(-1)).toMatchObject({kind:"add",newLine:1,text:"partial"});
  });

  it("supports keyboard paging and full-range navigation without stealing modified input", () => {
    const viewport = { top: 120, left: 0, height: 400, scrollHeight: 220000 };
    expect(diffScrollPosition({key:"End",ctrlKey:true}, viewport)).toEqual({top:219600,left:0});
    expect(diffScrollPosition({key:"Home"}, viewport)).toEqual({top:0,left:0});
    expect(diffScrollPosition({key:"PageDown"}, viewport)).toEqual({top:520,left:0});
    expect(diffScrollPosition({key:"ArrowDown",isComposing:true}, viewport)).toBeNull();
    expect(diffScrollPosition({key:"ArrowDown",shiftKey:true}, viewport)).toBeNull();
  });
});
