import { spawnSync } from "node:child_process";
import { expect, it } from "vitest";

it("evaluates titles only after acknowledged ordinary prompts and keeps manual names", () => {
  const result = spawnSync(process.execPath, ["--conditions=browser", "--input-type=module", "-e", String.raw`
    import assert from 'node:assert/strict';
    import { readFileSync } from 'node:fs';
    import { compileModule, parse } from 'svelte/compiler';
    import ts from 'typescript';
    import { effect_root } from 'svelte/internal/client';
    import { flushSync } from 'svelte';

    const source = readFileSync('src/lib/ChatPane.svelte', 'utf8');
    const body = parse(source, { modern: true }).instance.content.body;
    const statement = name => {
      const node = body.find(item => item.type === 'FunctionDeclaration' && item.id.name === name);
      assert.ok(node, 'ChatPane must expose ' + name);
      return source.slice(node.start, node.end);
    };
    const gate = body.find(item => item.type === 'VariableDeclaration' &&
      item.declarations.some(declaration => declaration.id.name === 'canSend'));
    assert.ok(gate);
    const harness = [
      'export function create() {',
      'let taskId = $state("task-a"), title = $state("Session 12345678"), autoName = $state(true);',
      'let draft = $state(""), attachments = $state([]), pendingImageReads = 0;',
      'let connected = true, dormant = false, initializing = false, stopping = false, switching = false, starting = false;',
      'let sending = $state(false), generation = 0, error = "", attachmentError = "", notice = "";',
      'let unknownPromptTasks = $state({}), pendingPromptTasks = $state({}), promptSequence = 0;',
      'const unknownPromptOutcome = $derived(Boolean(unknownPromptTasks[taskId]));',
      'const promptAwaitingAck = $derived(Boolean(pendingPromptTasks[taskId]));',
      'let conversation = $state({ busy: false, closed: false, messages: [] });',
      'let titleQueue = Promise.resolve(), streamingBehavior = "followUp", queuedImages = [], followScroll = false, pinnedMessageStart = null;',
      'let piCommands = [], runId = "run-a", scope = "rpc:task-a:run-a";',
      'const calls = [], evaluations = [], contexts = [], displayed = [];',
      'let promptFails = false, modelReply = null, modelFails = false;',
      'const canSubmitPrompt = (busy, value, images, reads) => !busy && !reads && (!!value.trim() || images > 0);',
      'const parseSlashCommand = text => { const match = /^\\/([\\w-]+)(?:\\s+(.*))?$/.exec(text.trim()); return match ? {name:match[1],args:match[2] ?? ""} : null; };',
      'const slashCommandRoute = name => name === "name" || name === "session" ? "rpc" : "unknown";',
      'const sessionTitleContext = messages => { contexts.push(messages); return messages; };',
      'const invoke = async (command, payload) => { evaluations.push({command,payload}); if (modelFails) throw new Error("offline"); return modelReply; };',
      'const call = async command => { calls.push(command); if (command.type === "prompt" && promptFails) throw new Error("send failed"); return {}; };',
      'const onAutoRename = (next, expected) => { if (!autoName || title !== expected) return false; title = next; displayed.push(next); return true; };',
      'const onManualRename = async next => { title = next; autoName = false; };',
      'const t = value => value, tm = value => value, readableRpcError = value => value;',
      'const commandError = cause => { error = String(cause); };',
      'const refreshStats = () => {}, isUnknownPromptOutcome = () => false;',
      source.slice(gate.start, gate.end),
      statement('scheduleTitleEvaluation'),
      statement('runSlashCommand'),
      statement('send'),
      'return { send, calls, evaluations, contexts, displayed,',
      '  get title() { return title; }, get draft() { return draft; }, get error() { return error; },',
      '  get queue() { return titleQueue; }, setDraft(value) { draft = value; },',
      '  setHistory(value) { conversation = { ...conversation, messages: value }; },',
      '  failPrompt(value) { promptFails = value; }, failModel(value) { modelFails = value; },',
      '  reply(value) { modelReply = value; } };',
      '}',
    ];
    const js = ts.transpileModule(harness.join('\n'), { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext } }).outputText;
    const compiled = compileModule(js, { filename: 'chat-title.svelte.js', generate: 'client' }).js.code
      .replace(/from (['"])(svelte(?:\/[^'"]*)?)\1/g, (_match, _quote, name) => 'from ' + JSON.stringify(import.meta.resolve(name)));
    const { create } = await import('data:text/javascript;base64,' + Buffer.from(compiled).toString('base64'));
    let chat;
    const dispose = effect_root(() => { chat = create(); });
    const send = async value => { chat.setDraft(value); flushSync(); await chat.send(); await chat.queue; flushSync(); };
    try {
      await send('');
      assert.equal(chat.calls.length, 0);
      assert.equal(chat.evaluations.length, 0);
      chat.failPrompt(true);
      await send('failed prompt');
      assert.equal(chat.evaluations.length, 0, 'failed RPC must not name');
      assert.equal(chat.draft, 'failed prompt', 'failed prompt remains editable');
      chat.failPrompt(false);
      await send('/session');
      assert.equal(chat.evaluations.length, 0, 'local command must not name');
      chat.reply('Repair startup');
      await send('Repair startup');
      assert.equal(chat.evaluations.length, 1);
      assert.deepEqual(chat.evaluations[0].payload.request.messages, []);
      assert.equal(chat.evaluations[0].payload.request.prompt, 'Repair startup');
      assert.equal(chat.title, 'Repair startup');
      assert.deepEqual(chat.displayed, ['Repair startup']);
      chat.setHistory([{role:'user',content:'Repair startup'},{role:'assistant',content:'Fixed startup'}]);
      chat.reply(null);
      await send('Add startup tests');
      assert.equal(chat.evaluations.length, 2, 'each acknowledged prompt is evaluated');
      assert.equal(chat.evaluations[1].payload.request.messages.length, 2);
      assert.equal(chat.title, 'Repair startup', 'keep decision must preserve displayed title');
      chat.reply('Improve sidebar');
      await send('Now improve sidebar');
      assert.equal(chat.title, 'Improve sidebar');
      assert.deepEqual(chat.displayed, ['Repair startup','Improve sidebar']);
      chat.failModel(true);
      await send('Another ordinary prompt');
      assert.equal(chat.evaluations.length, 4);
      assert.equal(chat.title, 'Improve sidebar', 'model failure must preserve title');
      assert.equal(chat.draft, '', 'model failure must not block sending');
      chat.failModel(false);
      await send('/name My title');
      assert.equal(chat.title, 'My title');
      assert.equal(chat.evaluations.length, 4, 'manual command must not auto-name');
      chat.reply('Must not apply');
      await send('Continue after manual rename');
      assert.equal(chat.evaluations.length, 4, 'manual origin must block model calls');
      assert.equal(chat.title, 'My title');
      console.log('chat title submission regression passed');
    } finally { dispose(); }
  `], { encoding: "utf8", timeout: 20_000 });
  const diagnostics = (result.stderr || result.stdout)
    .replace(/data:text\/javascript;base64,[A-Za-z0-9+/=]+/g, "<compiled-chat-title>");
  expect(result.error, diagnostics).toBeUndefined();
  expect(result.status, diagnostics).toBe(0);
  expect(result.stdout).toContain("chat title submission regression passed");
}, 25_000);
