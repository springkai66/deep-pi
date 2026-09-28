import { spawnSync } from "node:child_process";
import { it, expect } from "vitest";

it("keeps chat initialization tied to task/run/reconnect rather than conversation updates", () => {
  // Compile the actual component effect and run it with Svelte's browser scheduler.
  // A source-text assertion or a mocked $effect would miss the self-invalidation bug.
  const result = spawnSync(process.execPath, ["--conditions=browser", "--input-type=module", "-e", String.raw`
    import assert from 'node:assert/strict';
    import { readFileSync } from 'node:fs';
    import { compileModule, parse } from 'svelte/compiler';
    import ts from 'typescript';
    import { effect_root } from 'svelte/internal/client';
    import { flushSync } from 'svelte';

    const source = readFileSync('src/lib/ChatPane.svelte', 'utf8');
    const ast = parse(source, { modern: true });
    const statement = ast.instance.content.body.find(node =>
      node.type === 'ExpressionStatement' && node.expression.callee?.name === '$effect' &&
      source.slice(node.start, node.end).includes('const run = runId;'));
    assert.ok(statement, 'chat connection effect must exist');
    const effect = source.slice(statement.start, statement.end);
    const harness = [
      "import { untrack } from 'svelte';",
      'export function create() {',
      'const emptyConversation = () => ({ messages: [], closed: false, busy: false });',
      'let taskId = $state("task-a"), runId = $state(null), reconnect = $state(0);',
      'let conversation = $state(emptyConversation());',
      'let models = $state([]), connected = $state(false), initializing = $state(true);',
      'let dormant = $state(false), historySession = $state.raw(null);',
      'let generation = 0, autoSendOnConnect = false, sent = 0;',
      'let turnDurations, runningTurn, turnStartedAt, historyLimit, followScroll, pinnedMessageStart;',
      'let sending, stopping, changingModel, changingThinking, thinkingLevels, thinkingLevel;',
      'let autoNamed, sessionStats, modelsLoadFailed, modelsLoadError, modelsStale, piCommands;',
      'let queuedImages, modelName, selectedModel, historyOffset, error, extensionError;',
      'const calls = [];',
    ];
    harness.push(
      'const record = value => value ?? {};',
      'const t = value => value, tm = value => value;',
      'const onActivity = () => {}, onCancelDialogs = () => {}, refreshStats = () => {};',
      'const handleExtension = () => {}, applyRpcEvent = state => state;',
      'const loadHistory = (state, messages) => ({ ...state, messages });',
      'const send = () => { sent++; };',
      'const applySavedModelChoice = () => { models = [{ id: "model", provider: "provider" }]; };',
      'class Channel {}',
      'const invoke = async (command) => { calls.push(command); return {}; };',
      'const history = { messages: [{ role: "user", content: "history" }], start: 0, eventSequence: 0 };',
      'const reader = kind => ({ open: async () => { calls.push(kind); return history; }, dispose: async () => { calls.push("dispose"); } });',
      'const createDormantHistorySession = () => reader("dormant");',
      'const createRpcHistorySession = () => reader("rpc-history");',
      effect,
      'return { calls, get conversation() { return conversation; }, get connected() { return connected; },',
      'get models() { return models; }, get initializing() { return initializing; }, get sent() { return sent; },',
      'updateMessages() { conversation = { ...conversation, messages: [...conversation.messages, { role: "assistant", content: "reply" }] }; },',
      'start() { autoSendOnConnect = true; runId = "run-a"; }, restart() { runId = "run-b"; },',
      'switchTask() { taskId = "task-b"; }, reconnect() { reconnect++; } };',
      '}'
    );
    const js = ts.transpileModule(harness.join('\n'), { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext } }).outputText;
    const compiled = compileModule(js, { filename: 'chat-initialization.svelte.js', generate: 'client' }).js.code
      .replace(/from (['"])(svelte(?:\/[^'"]*)?)\1/g, (_match, _quote, name) => 'from ' + JSON.stringify(import.meta.resolve(name)));
    const { create } = await import('data:text/javascript;base64,' + Buffer.from(compiled).toString('base64'));
    let chat;
    const dispose = effect_root(() => { chat = create(); });
    const settle = async () => { for (let i = 0; i < 8; i++) { flushSync(); await Promise.resolve(); } };
    const count = command => chat.calls.filter(value => value === command).length;
    try {
      await settle();
      assert.equal(count('dormant'), 1, 'opening a dormant task initializes once');
      assert.equal(chat.conversation.messages.length, 1, 'history survives initialization');
      chat.updateMessages();
      await settle();
      assert.equal(count('dormant'), 1, 'history/message updates must not reopen the session');
      chat.start();
      await settle();
      assert.equal(count('subscribe_rpc'), 1);
      assert.equal(chat.connected, true);
      assert.equal(chat.initializing, false);
      assert.equal(chat.models.length, 1, 'model choices remain populated');
      assert.equal(chat.sent, 1, 'the dormant draft is sent once after connecting');
      chat.updateMessages();
      await settle();
      assert.equal(count('subscribe_rpc'), 1, 'RPC messages must not reconnect');
      assert.equal(chat.models.length, 1);
      for (const [action, expected] of [['restart', 2], ['switchTask', 3], ['reconnect', 4]]) {
        chat[action]();
        await settle();
        assert.equal(count('subscribe_rpc'), expected, action + ' must reconnect exactly once');
      }
      console.log('chat lifecycle regression passed');
    } catch (error) {
      console.error(error.message);
      process.exitCode = 1;
    } finally { dispose(); }
  `], { encoding: "utf8", timeout: 15_000 });
  const diagnostics = (result.stderr || result.stdout)
    .replace(/data:text\/javascript;base64,[A-Za-z0-9+/=]+/g, "<compiled-chat-effect>");
  expect(result.error, diagnostics).toBeUndefined();
  expect(result.status, diagnostics).toBe(0);
  expect(result.stdout).toContain("chat lifecycle regression passed");
}, 20_000);
