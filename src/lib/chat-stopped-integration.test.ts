import { spawnSync } from "node:child_process";
import { expect, it } from "vitest";

it("resumes stopped RPC chats only after connection and selected model are ready", () => {
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
      assert.ok(node, 'missing ChatPane function ' + name);
      return source.slice(node.start, node.end);
    };
    const gate = body.find(item => item.type === 'VariableDeclaration' &&
      item.declarations.some(declaration => declaration.id.name === 'canSend'));
    const effect = body.find(node => node.type === 'ExpressionStatement' && node.expression.callee?.name === '$effect' &&
      source.slice(node.start, node.end).includes('const offline = !run || stopped;'));
    assert.ok(gate && effect);
    const harness = [
      "import { untrack } from 'svelte';",
      'export function create({ startFails = false, modelFails = false, historyMissing = false, unknown = false } = {}) {',
      'globalThis.window = { localStorage: {} };',
      'let taskId = "task-a", runId = $state("old-run"), stopped = $state(true), reconnect = $state(0);',
      'let draft = $state("continue"), attachments = $state([{ id: 1, data: "aGk=", mimeType: "image/png" }]);',
      'let connected = $state(false), dormant = $state(false), initializing = $state(true), starting = $state(false), sending = $state(false);',
      'let stopping = false, switching = false, pendingImageReads = 0, attachmentError = "";',
      'let unknownPromptTasks = $state({}), pendingPromptTasks = $state({}), promptSequence = 0;',
      'const unknownPromptOutcome = $derived(Boolean(unknownPromptTasks[taskId]));',
      'const promptAwaitingAck = $derived(Boolean(pendingPromptTasks[taskId]));',
      'let pendingModelOnStart = $state(null), autoSendOnConnect = false;',
      'let models = $state([]), selectedModel = $state(""), modelName = $state(""), changingModel = false;',
      'let conversation = $state({ messages: [], closed: false, busy: false, queue: [] });',
      'let error = $state(""), generation = 0, historySession = $state.raw(null), stopRecords = [], pendingStopAnchor = null;',
      'let turnDurations, runningTurn, turnStartedAt, historyLimit, followScroll, pinnedMessageStart, historyOffset;',
      'let changingThinking, thinkingLevels, thinkingLevel, autoNamed, sessionStats, modelsLoadFailed, modelsLoadError, modelsStale, piCommands, queuedImages, extensionError;',
      'let streamingBehavior = "followUp", calls = [], starts = 0, releaseStart;',
      'const t = value => value, tm = value => value, record = value => value ?? {};',
      'const emptyConversation = () => ({ messages: [], closed: false, busy: false, queue: [] });',
      'const readStopRecords = () => [], loadHistory = (state, messages) => ({ ...state, messages });',
      'const suppressStoppedAbort = state => state, applyRpcEvent = state => state;',
      'const canSubmitPrompt = (busy, text, images, reads) => !busy && !reads && (!!text.trim() || images > 0);',
      'const parseSlashCommand = () => null, runSlashCommand = async () => "prompt";',
      'const sessionTitleContext = () => [], scheduleTitleEvaluation = () => {};',
      'const isUnknownPromptOutcome = cause => String(cause).includes("RPC_OUTCOME_UNKNOWN:");',
      'const commandError = cause => { error = String(cause); };',
      'const refreshStats = () => {}, onActivity = () => {}, onCancelDialogs = () => {}, handleExtension = () => {};',
      'const applySavedModelChoice = async () => {};',
      'const loadStoppedModels = async alive => { if (alive()) models = [{provider:"provider", id:"old", name:"Old"}, {provider:"provider", id:"new", name:"New"}]; };',
      'const refreshModels = async () => { models = [{provider:"provider", id:"old", name:"Old"}, {provider:"provider", id:"new", name:"New"}]; return models; };',
      'class Channel {}',
      'const invoke = async (name, args) => { calls.push({ name, args });',
      '  if (name === "rpc_command") {',
      '    if (args.command.type === "get_state") return {model:{provider:"provider", id:"old", name:"Old"}};',
      '    if (args.command.type === "set_model" && modelFails) throw new Error("model rejected");',
      '    if (args.command.type === "prompt" && unknown) throw new Error("RPC_OUTCOME_UNKNOWN: timeout");',
      '    return {}; } return 1; };',
      'const history = { messages:[{role:"user", content:"previous"}], model:{provider:"provider", id:"old"}, start:0, eventSequence:0 };',
      'const reader = kind => ({ open: async () => historyMissing && kind === "dormant" ? null : history, dispose: async () => {} });',
      'const createDormantHistorySession = () => reader("dormant"), createRpcHistorySession = () => reader("rpc");',
      'const onStartSession = async () => { starts++; if (startFails) return false;',
      '  await new Promise(resolve => { releaseStart = resolve; }); stopped = false; runId = "new-run-" + starts; return true; };',
      'const call = command => invoke("rpc_command", {taskId, runId, command});',
      source.slice(gate.start, gate.end),
      statement('changeModel'),
      statement('send'),
      source.slice(effect.start, effect.end),
      'return { send, changeModel, calls, get starts() {return starts;}, get error() {return error;},',
      'get draft() {return draft;}, get attachments() {return attachments;}, get dormant() {return dormant;},',
      'get selectedModel() {return selectedModel;}, get messages() {return conversation.messages;},',
      'get canSend() {return canSend;}, get blocked() {return unknownPromptOutcome;},',
      'get runId() {return runId;}, get models() {return models;},',
      'release() {releaseStart?.();}, allowStart() {startFails = false;}, allowModel() {modelFails = false;},',
      'acknowledge() {const {[taskId]: _value, ...rest} = unknownPromptTasks; unknownPromptTasks = rest;} };',
      '}',
    ];
    const js = ts.transpileModule(harness.join('\n'), {compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.ESNext}}).outputText;
    const compiled = compileModule(js, {filename:'chat-stopped.svelte.js',generate:'client'}).js.code
      .replace(/from (['"])(svelte(?:\/[^'"]*)?)\1/g, (_match,_quote,name) => 'from ' + JSON.stringify(import.meta.resolve(name)));
    const { create } = await import('data:text/javascript;base64,' + Buffer.from(compiled).toString('base64'));
    const settle = async () => { for (let i = 0; i < 16; i++) { flushSync(); await Promise.resolve(); } };
    const prompts = chat => chat.calls.filter(call => call.name === 'rpc_command' && call.args.command.type === 'prompt');
    const scenario = async (options, check) => {
      let chat;
      const dispose = effect_root(() => { chat = create(options); });
      try { await settle(); await check(chat); } finally { dispose(); }
    };
    await scenario({}, async chat => {
      assert.equal(chat.dormant, true);
      assert.equal(chat.models.length, 2);
      assert.equal(chat.messages.length, 1);
      assert.equal(chat.runId, 'old-run');
      assert.equal(chat.canSend, true);
      assert.equal(await chat.changeModel('provider/new'), true);
      const pending = chat.send(); await settle();
      await chat.send(); await settle();
      assert.equal(chat.starts, 1, 'repeated submissions cannot start twice');
      assert.equal(prompts(chat).length, 0, 'do not send before the startup reply and RPC readiness');
      assert.equal(chat.attachments.length, 1);
      chat.release(); await pending; await settle();
      assert.equal(chat.runId, 'new-run-1');
      assert.equal(prompts(chat).length, 1, 'one prompt after model selection: ' + JSON.stringify({error:chat.error, dormant:chat.dormant, canSend:chat.canSend, calls:chat.calls.map(call => [call.name, call.args?.command?.type])}));
      assert.equal(prompts(chat)[0].args.command.images.length, 1);
      const commands = chat.calls.filter(call => call.name === 'rpc_command').map(call => call.args.command.type);
      assert.ok(commands.indexOf('set_model') < commands.indexOf('prompt'));
      assert.equal(chat.selectedModel, 'provider/new');
      assert.equal(chat.draft, ''); assert.equal(chat.attachments.length, 0);
      assert.equal(chat.messages.length, 1, 'historical message survives restart');
      await chat.send(); assert.equal(prompts(chat).length, 1);
    });
    await scenario({}, async chat => {
      const pending = chat.send(); await settle(); chat.release(); await pending; await settle();
      assert.equal(chat.selectedModel, 'provider/old', 'without reselection, use the session model');
      assert.equal(prompts(chat).length, 1);
      assert.equal(chat.calls.filter(call => call.name === 'rpc_command' && call.args.command.type === 'set_model').length, 0);
    });
    await scenario({startFails:true, historyMissing:true}, async chat => {
      assert.equal(chat.models.length, 2, 'model catalog loads even if history is missing');
      await chat.send(); await settle();
      assert.match(chat.error, /无法启动会话/);
      assert.equal(chat.draft, 'continue'); assert.equal(chat.attachments.length, 1);
      assert.equal(prompts(chat).length, 0);
      chat.allowStart(); const pending = chat.send(); await settle(); chat.release(); await pending; await settle();
      assert.equal(chat.starts, 2); assert.equal(prompts(chat).length, 1);
    });
    await scenario({modelFails:true}, async chat => {
      await chat.changeModel('provider/new');
      const pending = chat.send(); await settle(); chat.release(); await pending; await settle();
      assert.equal(prompts(chat).length, 0, 'rejected model cannot send under a different model');
      assert.match(chat.error, /model rejected/);
      assert.equal(chat.dormant, true); assert.equal(chat.draft, 'continue');
      assert.equal(chat.attachments.length, 1);
      chat.allowModel(); const retry = chat.send(); await settle(); chat.release(); await retry; await settle();
      assert.equal(prompts(chat).length, 1);
      assert.equal(chat.selectedModel, 'provider/new');
    });
    await scenario({unknown:true}, async chat => {
      const pending = chat.send(); await settle(); chat.release(); await pending; await settle();
      assert.equal(chat.blocked, true); assert.equal(chat.draft, 'continue');
      assert.equal(chat.attachments.length, 1);
      await chat.send(); await settle();
      assert.equal(prompts(chat).length, 1, 'uncertain delivery never automatically retries');
    });
    console.log('stopped chat integration passed');
  `], { encoding: "utf8", timeout: 20_000 });
  const diagnostics = (result.stderr || result.stdout)
    .replace(/data:text\/javascript;base64,[A-Za-z0-9+/=]+/g, "<compiled-stopped-chat>").slice(-4000);
  expect(result.error, diagnostics).toBeUndefined();
  expect(result.status, diagnostics).toBe(0);
  expect(result.stdout).toContain("stopped chat integration passed");
}, 25_000);
