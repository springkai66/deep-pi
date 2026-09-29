import { spawnSync } from "node:child_process";
import { expect, it } from "vitest";

it("keeps list and reopened task titles in sync, with manual renames taking precedence", () => {
  const result = spawnSync(process.execPath, ["--input-type=module", "-e", String.raw`
    import assert from 'node:assert/strict';
    import { readFileSync } from 'node:fs';
    import { parse } from 'svelte/compiler';
    import ts from 'typescript';

    const source = readFileSync('src/routes/+page.svelte', 'utf8');
    const body = parse(source, { modern: true }).instance.content.body;
    const fn = name => {
      const node = body.find(item => item.type === 'FunctionDeclaration' && item.id.name === name);
      assert.ok(node, 'page must expose ' + name);
      return source.slice(node.start, node.end);
    };
    const harness = [
      'const task = { id: "task-a", title: "Session 12345678", titleOrigin: "auto" };',
      'let stored = { ...task }, dialogResult = null, opened = 0;',
      'const terminalTaskIds = [];',
      'const t = value => value, showError = error => { throw error; };',
      'const inputDialog = async () => dialogResult;',
      'const invoke = async (command, payload) => { assert.equal(command, "rename_task");',
      '  stored = { ...stored, title: payload.title.trim(), titleOrigin: "manual" }; };',
      'const openTask = () => { opened++; };',
      fn('renameTask'), fn('manualRenameTask'), fn('autoRenameTask'), fn('applyTaskRestart'),
      'export { task, renameTask, manualRenameTask, autoRenameTask, applyTaskRestart, terminalTaskIds };',
      'export const getStored = () => ({ ...stored });',
      'export const setStored = next => { stored = { ...stored, ...next }; };',
      'export const setDialog = next => { dialogResult = next; };',
      'export const getOpened = () => opened;',
    ];
    const js = ts.transpileModule(harness.join('\n'), { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext } }).outputText;
    const page = await import('data:text/javascript;base64,' + Buffer.from(js).toString('base64'));
    const { task } = page;
    assert.equal(page.autoRenameTask(task, 'Repair startup', 'wrong title'), false);
    assert.equal(task.title, 'Session 12345678');
    page.setStored({ title: 'Repair startup' });
    assert.equal(page.autoRenameTask(task, 'Repair startup', 'Session 12345678'), true);
    assert.equal(task.title, page.getStored().title, 'list reflects saved model title');
    page.applyTaskRestart(task, page.getStored());
    assert.equal(task.title, 'Repair startup', 'reopening keeps saved title');
    assert.equal(task.titleOrigin, 'auto');
    assert.deepEqual(page.terminalTaskIds, ['task-a']);
    assert.equal(page.getOpened(), 1);
    await page.manualRenameTask(task, ' My title ');
    assert.equal(task.title, 'My title');
    assert.equal(task.titleOrigin, 'manual');
    assert.equal(page.autoRenameTask(task, 'New model title', 'My title'), false);
    page.applyTaskRestart(task, page.getStored());
    assert.equal(task.title, 'My title');
    assert.equal(task.titleOrigin, 'manual', 'manual origin survives reopening');
    page.setDialog('My title');
    await page.renameTask(task);
    assert.equal(page.getStored().titleOrigin, 'manual', 'confirming the same list title marks it manual');
    assert.equal(page.autoRenameTask(task, 'New model title', 'My title'), false);
    console.log('task title page regression passed');
  `], { encoding: "utf8", timeout: 20_000 });
  const diagnostics = (result.stderr || result.stdout)
    .replace(/data:text\/javascript;base64,[A-Za-z0-9+/=]+/g, "<compiled-task-title-page>");
  expect(result.error, diagnostics).toBeUndefined();
  expect(result.status, diagnostics).toBe(0);
  expect(result.stdout).toContain("task title page regression passed");
}, 25_000);
