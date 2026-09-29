import { spawnSync } from "node:child_process";
import { expect, it } from "vitest";

it("preserves uncertain prompts without replay, and does not retain an acknowledged draft after reconnect", () => {
  // Execute the real ChatPane send/recover functions and send gate under Svelte's browser scheduler.
  // An injected RPC command loses its acknowledgement; no mock implementation of send is used.
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
    const sendGate = body.find(item => item.type === 'VariableDeclaration' &&
      item.declarations.some(declaration => declaration.id.name === 'canSend'));
    assert.ok(sendGate, 'ChatPane must gate prompt submission');
    const recovery = readFileSync('src/lib/rpc-recovery.ts', 'utf8');
    const harness = [
      recovery,
      'export function create() {',
      'let taskId = $state("task-a"), runId = $state("run-a"), reconnect = $state(0), generation = 0;',
      'let draft = $state("unique prompt"), attachments = $state([{id:1,data:"aGk=", mimeType:"image/png"}]);',
      'let unknownPromptTasks = $state({}), pendingPromptTasks = $state({}), promptSequence = 0, recovering = $state(false), notice = $state("");',
      'const unknownPromptOutcome = $derived(Boolean(unknownPromptTasks[taskId]));',
      'const promptAwaitingAck = $derived(Boolean(pendingPromptTasks[taskId]));',
      'let connected = $state(true), dormant = false, initializing = false, stopping = false, switching = false;',
      'let conversation = $state({busy:false,closed:false,error:""}), pendingImageReads = 0;',
      'let sending = $state(false), error = $state(""), attachmentError = "", queuedImages = [];',
      'let streamingBehavior = "followUp", followScroll = false, pinnedMessageStart = null;',
      'let probeOpen = true, submitMode = "unknown", pendingSuccess, pendingFailure, restarts = 0;',
      'const calls = [], canSubmitPrompt = (busy, value, images, reads) => !busy && !reads && (!!value.trim() || images > 0);',
      'const parseSlashCommand = () => null, runSlashCommand = async () => "prompt";',
      'const sessionTitleContext = () => [], scheduleTitleEvaluation = () => {};',
      'const onReloadSession = () => {}, t = value => value, tm = value => value, readableRpcError = value => value;',
      'const refreshStats = () => {}, isUnknownPromptOutcome = cause => String(cause).includes("RPC_OUTCOME_UNKNOWN:");',
      'const commandError = cause => { error = tm(readableRpcError(String(cause))); if (error.includes("RPC_OUTCOME_UNKNOWN:")) connected = false; };',
      'const invoke = async (name, args) => { calls.push({name,args});',
      '  if (name === "rpc_run_open") return probeOpen;',
      '  if (name === "rpc_command" && args.command.type === "prompt") {',
      '    if (submitMode === "unknown") throw new Error("RPC_OUTCOME_UNKNOWN: request timed out");',
      '    if (submitMode === "pending") return new Promise(resolve => { pendingSuccess = resolve; });',
      '    if (submitMode === "pendingUnknown") return new Promise((_resolve, reject) => { pendingFailure = reject; });',
      '    return {};',
      '  } return {}; };',
      'const call = command => invoke("rpc_command", {taskId,runId,command});',
      'const onRecoverSession = async expected => { assert.equal(expected, "run-a"); restarts++; runId = "run-b"; return runId; };',
      source.slice(sendGate.start, sendGate.end),
      statement('send'),
      statement('recoverSession'),
      'return {send, recoverSession, calls,',
      '  get draft() {return draft;}, get attachments() {return attachments;},',
      '  get blocked() {return unknownPromptOutcome;}, get canSend() {return canSend;},',
      '  get reconnects() {return reconnect;}, get restarts() {return restarts;},',
      '  setProbe(value) {probeOpen = value;}, setMode(value) {submitMode = value;},',
      '  acknowledge() {const {[taskId]: _acknowledged, ...remaining} = unknownPromptTasks; unknownPromptTasks = remaining; connected = true;},',
      '  supersede() {generation++; sending = false;}, settleSuccess() {pendingSuccess();},',
      '  settleUnknown() {pendingFailure(new Error("RPC_OUTCOME_UNKNOWN: request timed out"));},',
      '  editDraft(value) {draft = value;}, addImage() {attachments = [...attachments, {id:2,data:"bmV3",mimeType:"image/png"}];},',
      '  switchTask(value) {taskId = value; runId = "run-" + value; generation++; sending = false; connected = true;} };',
      '}',
    ];
    const js = ts.transpileModule(harness.join('\n'), {compilerOptions: {target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext}}).outputText;
    const compiled = compileModule(js, {filename:'chat-recovery.svelte.js',generate:'client'}).js.code
      .replace(/from (['"])(svelte(?:\/[^'"]*)?)\1/g, (_match,_quote,name) => 'from ' + JSON.stringify(import.meta.resolve(name)));
    const { create } = await import('data:text/javascript;base64,' + Buffer.from(compiled).toString('base64'));
    let chat;
    const dispose = effect_root(() => { chat = create(); });
    const settle = () => { flushSync(); };
    const prompts = () => chat.calls.filter(call => call.name === 'rpc_command' && call.args.command.type === 'prompt').length;
    try {
      await chat.send(); settle();
      assert.equal(prompts(), 1);
      assert.equal(chat.blocked, true);
      assert.equal(chat.canSend, false);
      assert.equal(chat.draft, 'unique prompt');
      assert.equal(chat.attachments.length, 1);
      await chat.send(); assert.equal(prompts(), 1, 'uncertain outcome must not resubmit');
      await chat.recoverSession(); settle();
      assert.equal(chat.reconnects, 1, 'living run should resubscribe');
      assert.equal(chat.restarts, 0);
      assert.equal(prompts(), 1, 'resubscription must not replay');
      chat.setProbe(false);
      await chat.recoverSession(); settle();
      assert.equal(chat.restarts, 1, 'exited run should restart');
      assert.equal(prompts(), 1, 'restart must not replay');
      chat.acknowledge(); chat.setMode('success'); settle();
      await chat.send(); settle();
      assert.equal(prompts(), 2, 'user can send manually after acknowledging');
      assert.equal(chat.draft, '');
      assert.equal(chat.attachments.length, 0);
    } finally { dispose(); }
    let inFlight;
    const cleanup = effect_root(() => { inFlight = create(); });
    try {
      inFlight.setMode('pending');
      const completion = inFlight.send();
      assert.equal(inFlight.calls.filter(call => call.name === 'rpc_command').length, 1);
      inFlight.supersede(); settle();
      assert.equal(inFlight.canSend, false, 'reconnect must not unlock a prompt still waiting for its RPC acknowledgement');
      // A blocked send must return immediately rather than waiting for the outstanding call.
      await inFlight.send();
      assert.equal(inFlight.calls.filter(call => call.name === 'rpc_command').length, 1, 'pending prompt cannot be duplicated');
      inFlight.settleSuccess();
      await completion; settle();
      assert.equal(inFlight.draft, '', 'acknowledged prompt cannot remain as a resendable draft');
      assert.equal(inFlight.attachments.length, 0);
      assert.equal(inFlight.canSend, false, 'empty composer cannot send');
      inFlight.editDraft('next prompt'); settle();
      assert.equal(inFlight.canSend, true, 'acknowledged request must release send gate');
      inFlight.setMode('success'); await inFlight.send();
      assert.equal(inFlight.calls.filter(call => call.name === 'rpc_command' && call.args.command.type === 'prompt').length, 2);
    } finally { cleanup(); }
    let edited;
    const disposeEdited = effect_root(() => { edited = create(); });
    try {
      edited.setMode('pending');
      const completion = edited.send();
      edited.supersede(); edited.editDraft('edited while waiting'); edited.addImage(); settle();
      assert.equal(edited.canSend, false, 'edited draft cannot submit while prior RPC is pending');
      edited.settleSuccess(); await completion; settle();
      assert.equal(edited.draft, 'edited while waiting');
      assert.deepEqual(edited.attachments.map(file => file.id), [2], 'only the sent attachment is cleared');
      assert.equal(edited.canSend, true);
    } finally { disposeEdited(); }
    let switched;
    const disposeSwitched = effect_root(() => { switched = create(); });
    try {
      switched.setMode('pending');
      const completion = switched.send();
      switched.switchTask('task-b'); switched.editDraft('task b prompt'); switched.setMode('success'); settle();
      await switched.send(); settle();
      assert.equal(switched.draft, '');
      switched.editDraft('task b next');
      switched.switchTask('task-a'); settle();
      assert.equal(switched.canSend, false, 'task a is still pending even after task b settles');
      switched.switchTask('task-b'); settle();
      switched.settleSuccess(); await completion; settle();
      assert.equal(switched.draft, 'task b next', 'old task acknowledgement must not edit current task');
      switched.switchTask('task-a'); settle();
      assert.equal(switched.canSend, true, 'settled old task must release its own gate');
    } finally { disposeSwitched(); }
    let delayedUnknown;
    const disposeUnknown = effect_root(() => { delayedUnknown = create(); });
    try {
      delayedUnknown.setMode('pendingUnknown');
      const completion = delayedUnknown.send();
      delayedUnknown.supersede(); settle();
      delayedUnknown.settleUnknown(); await completion; settle();
      assert.equal(delayedUnknown.blocked, true, 'late unknown outcome must still require confirmation');
      assert.equal(delayedUnknown.draft, 'unique prompt');
      assert.equal(delayedUnknown.attachments.length, 1);
      assert.equal(delayedUnknown.canSend, false);
      await delayedUnknown.send();
      assert.equal(delayedUnknown.calls.filter(call => call.name === 'rpc_command' && call.args.command.type === 'prompt').length, 1);
    } finally { disposeUnknown(); }
    let twoUnknown;
    const disposeTwo = effect_root(() => { twoUnknown = create(); });
    try {
      await twoUnknown.send(); settle(); assert.equal(twoUnknown.blocked, true);
      twoUnknown.switchTask('task-b'); twoUnknown.editDraft('task b prompt'); settle();
      await twoUnknown.send(); settle(); assert.equal(twoUnknown.blocked, true);
      twoUnknown.switchTask('task-a'); settle();
      assert.equal(twoUnknown.blocked, true, 'task b unknown result must not overwrite task a');
      twoUnknown.acknowledge(); settle(); assert.equal(twoUnknown.canSend, true);
      twoUnknown.switchTask('task-b'); settle();
      assert.equal(twoUnknown.blocked, true, 'confirming task a must not confirm task b');
      assert.equal(twoUnknown.canSend, false);
    } finally { disposeTwo(); }
    console.log('chat recovery end-to-end seam passed');
  `], { encoding: "utf8", timeout: 20_000 });
  const diagnostics = (result.stderr || result.stdout)
    .replace(/data:text\/javascript;base64,[A-Za-z0-9+/=]+/g, "<compiled-chat-recovery>");
  expect(result.error, diagnostics).toBeUndefined();
  expect(result.status, diagnostics).toBe(0);
  expect(result.stdout).toContain("chat recovery end-to-end seam passed");
}, 25_000);
