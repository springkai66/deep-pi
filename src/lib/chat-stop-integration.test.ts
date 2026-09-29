import { spawnSync } from "node:child_process";
import { expect, it } from "vitest";

it("records only a successful composer stop and leaves failed and other interrupts alone", () => {
  const result = spawnSync(process.execPath, ["--conditions=browser", "--input-type=module", "-e", String.raw`
    import assert from 'node:assert/strict';
    import { readFileSync } from 'node:fs';
    import { compileModule, parse } from 'svelte/compiler';
    import ts from 'typescript';
    import { effect_root } from 'svelte/internal/client';
    import { flushSync } from 'svelte';

    const source = readFileSync('src/lib/ChatPane.svelte', 'utf8');
    const body = parse(source, { modern: true }).instance.content.body;
    const node = body.find(item => item.type === 'FunctionDeclaration' && item.id.name === 'interrupt');
    const sendNode = body.find(item => item.type === 'FunctionDeclaration' && item.id.name === 'send');
    const sendGate = body.find(item => item.type === 'VariableDeclaration' &&
      item.declarations.some(declaration => declaration.id.name === 'canSend'));
    assert.ok(sendNode && sendGate, 'ChatPane send and send gate must exist');
    assert.ok(node, 'ChatPane interrupt must exist');
    assert.match(source, /class="send-button stop-button"[^\n]+onclick=\{\(\) => void interrupt\(true\)\}/);
    assert.match(source, /class="tool-call-interrupt"[^>]*onclick=\{\(\) => void interrupt\(\)\}/);
    const harness = [
      'export function create() {',
      'let conversation = $state({ busy: true, closed: false, messages: [{ role: "user", content: "task", timestamp: 100 }], error: "", queue: [] });',
      'let stopping = $state(false), pendingStopAnchor = $state(null), stopRecords = $state([]);',
      'let generation = 0, taskId = "task-a", draft = $state(""), restoredDraft = "", error = $state("");',
      'let connected = $state(true), dormant = false, initializing = false, switching = false, sending = $state(false);',
      'let unknownPromptTasks = $state({}), pendingPromptTasks = $state({}), promptSequence = 0, pendingImageReads = 0;',
      'let attachments = $state([]), attachmentError = "", queuedImages = [], streamingBehavior = "followUp";',
      'let followScroll = false, pinnedMessageStart = null;',
      'const unknownPromptOutcome = $derived(Boolean(unknownPromptTasks[taskId]));',
      'const promptAwaitingAck = $derived(Boolean(pendingPromptTasks[taskId]));',
      'let rejectCommand = "", releaseAbort, calls = [];',
      'const window = { localStorage: { setItem(key, value) { this[key] = value; } } };',
      'const crypto = { randomUUID: () => "stop-1" };',
      'const lastUserAnchor = messages => messages.findLast(item => item.role === "user")?.timestamp ?? null;',
      'const saveStopRecord = (storage, id, value) => { storage.setItem(id, JSON.stringify([...stopRecords, value])); return [...stopRecords, value]; };',
      'const suppressStoppedAbort = (state, records, pending) => pending === 100 || records.some(item => item.anchor === 100) ? { ...state, error: "" } : state;',
      'const canSubmitPrompt = (inFlight, value, images, reads) => !inFlight && !reads && (!!value.trim() || images > 0);',
      'const parseSlashCommand = () => null, runSlashCommand = async () => "prompt";',
      'const sessionTitleContext = () => [], scheduleTitleEvaluation = () => {};',
      'const isUnknownPromptOutcome = () => false, refreshStats = () => {}, onReloadSession = () => {};',
      'const commandError = cause => { error = String(cause); };',
      'const call = async command => { calls.push(command.type); if (rejectCommand === command.type) throw new Error(command.type + " failed");',
      '  if (command.type === "clear_queue") return { steering: [], followUp: [] };',
      '  if (command.type === "abort") return new Promise(resolve => { releaseAbort = resolve; }); return {}; };',
      source.slice(sendGate.start, sendGate.end),
      source.slice(node.start, node.end),
      source.slice(sendNode.start, sendNode.end),
      'return { interrupt, send, calls, get stopping() { return stopping; }, get pending() { return pendingStopAnchor; },',
      '  get records() { return stopRecords; }, get error() { return error; }, get busy() { return conversation.busy; },',
      '  get storage() { return window.localStorage; }, get conversation() { return conversation; },',
      '  settleAbort() { releaseAbort({}); }, fail(value) { rejectCommand = value; },',
      '  idle() { conversation = { ...conversation, busy: false, queue: ["queued"] }; },',
      '  finishTurn() { conversation = { ...conversation, busy: false }; draft = "next prompt"; },',
      '  abortEvent() { conversation = suppressStoppedAbort({ ...conversation, error: "Request aborted" }, stopRecords, pendingStopAnchor); } };',
      '}',
    ];
    const js = ts.transpileModule(harness.join('\n'), { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext } }).outputText;
    const compiled = compileModule(js, { filename: 'chat-stop-integration.svelte.js', generate: 'client' }).js.code
      .replace(/from (['"])(svelte(?:\/[^'"]*)?)\1/g, (_match, _quote, name) => 'from ' + JSON.stringify(import.meta.resolve(name)));
    const { create } = await import('data:text/javascript;base64,' + Buffer.from(compiled).toString('base64'));
    const settle = async () => { for (let i = 0; i < 5; i++) { flushSync(); await Promise.resolve(); } };
    const scenario = async (check) => {
      let chat;
      const dispose = effect_root(() => { chat = create(); });
      try { await check(chat); } finally { dispose(); }
    };
    await scenario(async chat => {
      const pending = chat.interrupt(true);
      await settle();
      assert.equal(chat.stopping, true);
      assert.equal(chat.pending, 100);
      chat.abortEvent();
      assert.equal(chat.conversation.error, '', 'abort event is hidden while command is pending');
      chat.settleAbort(); await pending; await settle();
      assert.equal(chat.stopping, false);
      assert.equal(chat.pending, null);
      assert.equal(chat.records.length, 1);
      assert.equal(chat.records[0].anchor, 100);
      assert.equal(JSON.parse(chat.storage['task-a']).length, 1);
      assert.deepEqual(chat.calls, ['clear_queue', 'abort']);
      assert.equal(chat.error, '');
      chat.finishTurn(); await settle();
      await chat.send(); await settle();
      assert.deepEqual(chat.calls, ['clear_queue', 'abort', 'prompt'], 'next prompt is sent in the same session');
    });
    await scenario(async chat => {
      chat.fail('abort');
      await chat.interrupt(true);
      assert.equal(chat.records.length, 0, 'failed abort must not write stop record');
      assert.equal(chat.storage['task-a'], undefined);
      assert.equal(chat.error, 'Error: abort failed');
      assert.equal(chat.pending, null);
      assert.equal(chat.stopping, false);
    });
    await scenario(async chat => {
      chat.fail('clear_queue');
      await chat.interrupt(true);
      assert.deepEqual(chat.calls, ['clear_queue']);
      assert.equal(chat.records.length, 0);
      assert.equal(chat.error, 'Error: clear_queue failed');
    });
    await scenario(async chat => {
      const pending = chat.interrupt(); await settle();
      assert.equal(chat.pending, null, 'tool interrupt does not mark a composer stop');
      chat.settleAbort(); await pending;
      assert.equal(chat.records.length, 0);
    });
    await scenario(async chat => {
      chat.idle();
      const pending = chat.interrupt(true); await settle();
      assert.equal(chat.pending, null, 'queue-only stop has no responding turn');
      chat.settleAbort(); await pending;
      assert.equal(chat.records.length, 0);
    });
    console.log('composer stop integration passed');
  `], { encoding: "utf8", timeout: 15_000 });
  const diagnostics = (result.stderr || result.stdout)
    .replace(/data:text\/javascript;base64,[A-Za-z0-9+/=]+/g, "<compiled-chat-interrupt>").slice(-3000);
  expect(result.error, diagnostics).toBeUndefined();
  expect(result.status, diagnostics).toBe(0);
  expect(result.stdout).toContain("composer stop integration passed");
}, 20_000);
