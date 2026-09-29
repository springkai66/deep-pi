import assert from 'node:assert/strict';
import { test } from 'node:test';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { mkdtemp, writeFile, readFile, rm } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';

const cli = process.env.PI_RPC_TEST_CLI;
function peer(args, env, cwd, protocol = 'bridge') {
  const child = spawn(process.execPath, args, { cwd, env, stdio: ['pipe', 'pipe', 'pipe'] });
  child.stderr.resume();
  const lines = createInterface({ input: child.stdout });
  const pending = new Map(); let next = 0;
  lines.on('line', line => {
    let event; try { event = JSON.parse(line); } catch { return; }
    const id = protocol === 'bridge' ? event.reply : event.id;
    const item = pending.get(id);
    if (item) { clearTimeout(item.timer); pending.delete(id); item.resolve(event); }
  });
  return {
    request(body) {
      return new Promise((resolveRequest, reject) => {
        const id = protocol === 'bridge' ? ++next : String(++next);
        const timer = setTimeout(() => { pending.delete(id); reject(new Error('Local model settings test timed out')); }, 15000);
        pending.set(id, { resolve: resolveRequest, timer });
        child.stdin.write(JSON.stringify({ v: 1, id, ...body }) + '\n');
      });
    },
    async close() {
      lines.close(); child.kill();
      if (child.exitCode === null && child.signalCode === null) await new Promise(r => child.once('exit', r));
      for (const item of pending.values()) clearTimeout(item.timer);
    },
  };
}

test('subscription model defaults and limits use managed Pi settings without changing credentials', { skip: !cli, timeout: 45000 }, async () => {
  const home = await mkdtemp(join(process.env.PI_SCRATCH_DIR || tmpdir(), 'subscription-model-settings-'));
  const authPath = join(home, 'auth.json');
  const auth = JSON.stringify({ fixture: { type: 'api_key', key: 'credential-sentinel' } });
  await writeFile(authPath, auth);
  await writeFile(join(home, 'settings.json'), JSON.stringify({ unknownFixture: { preserve: true }, modelThinkingLevels: { 'other/model': 'low' } }));
  const sdk = join(dirname(dirname(dirname(resolve(cli)))), 'dist', 'index.js');
  const env = { ...process.env, PI_CODING_AGENT_DIR: home, PI_CODING_AGENT_SESSION_DIR: join(home, 'sessions'), PI_AUTH_PI_SDK: sdk, PI_AUTH_AUTH_JSON: authPath, PI_TELEMETRY: '0' };
  delete env.PI_AUTH_BRIDGE_TEST_INJECT;
  const bridge = peer([resolve('src-tauri/resources/pi-auth-bridge.mjs')], env, home);
  let rpc;
  try {
    const catalog = await bridge.request({ op: 'models', provider: 'openai-codex' });
    assert.equal(catalog.ok, true);
    const model = catalog.models.find(m => m.id === 'gpt-6-sol') ?? catalog.models.find(m => m.reasoning) ?? catalog.models[0];
    assert.ok(model, 'installed Pi must contain a Codex model');
    assert.ok(Array.isArray(model.thinkingLevels) && model.thinkingLevels.length > 0);
    const thinking = model.thinkingLevels.includes('high') ? 'high' : model.thinkingLevels[0];
    const saved = await bridge.request({ op: 'set_model_defaults', provider: 'openai-codex', modelId: model.id, thinkingLevel: thinking });
    assert.equal(saved.ok, true);
    assert.equal(saved.settings.defaultModel, model.id);
    assert.equal(saved.settings.modelThinkingLevels[model.id], thinking);
    assert.ok(!JSON.stringify(saved).includes('credential-sentinel'));
    const settingsText = await readFile(join(home, 'settings.json'), 'utf8');
    const settings = JSON.parse(settingsText);
    assert.equal(settings.defaultProvider, 'openai-codex');
    assert.equal(settings.defaultModel, model.id);
    assert.equal(settings.modelThinkingLevels['openai-codex/' + model.id], thinking);
    assert.equal(settings.modelThinkingLevels['other/model'], 'low');
    assert.deepEqual(settings.unknownFixture, { preserve: true });
    for (const bad of [{ modelId: 'nonexistent-local-fixture', thinkingLevel: thinking }, { modelId: model.id, thinkingLevel: 'invalid-level' }]) {
      const rejected = await bridge.request({ op: 'set_model_defaults', provider: 'openai-codex', ...bad });
      assert.equal(rejected.ok, false);
      assert.equal(await readFile(join(home, 'settings.json'), 'utf8'), settingsText);
    }
    await writeFile(join(home, 'models.json'), JSON.stringify({ providers: { 'openai-codex': { apiKey: 'local-fixture-only', modelOverrides: { [model.id]: { contextWindow: 64000, maxTokens: 4096 } } } } }));
    const refreshed = await bridge.request({ op: 'models', provider: 'openai-codex' });
    assert.equal(refreshed.ok, true);
    assert.equal(refreshed.models.find(m => m.id === model.id).contextWindow, 64000);
    assert.equal(refreshed.models.find(m => m.id === model.id).maxTokens, 4096);
    rpc = peer([resolve(cli), '--mode', 'rpc', '--offline', '--no-extensions', '--no-skills', '--no-context-files', '--no-prompt-templates', '--no-themes'], env, home, 'rpc');
    const state = await rpc.request({ type: 'get_state' });
    assert.equal(state.success, true);
    assert.equal(state.data.model.provider, 'openai-codex');
    assert.equal(state.data.model.id, model.id);
    assert.equal(state.data.thinkingLevel, thinking);
    assert.equal(state.data.model.contextWindow, 64000);
    assert.equal(state.data.model.maxTokens, 4096);
    assert.equal(await readFile(authPath, 'utf8'), auth);
  } finally { await rpc?.close(); await bridge.close(); await rm(home, { recursive: true, force: true }); }
});
