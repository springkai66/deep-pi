import assert from 'node:assert/strict';
import { test } from 'node:test';
import { spawn } from 'node:child_process';
import http from 'node:http';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { codexTransportOptions } from '../src-tauri/resources/pi-codex-transport.mjs';

// Optional installed-runtime contract test. All prompts and credentials are synthetic;
// every provider request terminates on a local HTTP fixture.
const cli = process.env.PI_RPC_TEST_CLI;
for (const mode of ['socket', 'idle', 'websocket']) {
  test(`installed Pi RPC keeps its session usable after ${mode} failure`, { skip: !cli, timeout: 25000 }, async () => {
    const dir = await mkdtemp(join(process.env.PI_SCRATCH_DIR || tmpdir(), 'deeppi-codex-local-'));
    const home = join(dir, 'agent'), project = join(dir, 'project');
    await mkdir(home); await mkdir(project);
    let requests = 0;
    const upgraded = new Set();
    const server = http.createServer((req, res) => {
      requests++; req.resume();
      res.writeHead(200, { 'Content-Type': 'text/event-stream' });
      res.write('data: {"type":"response.created","response":{"id":"fixture","status":"in_progress"}}\n\n');
      const timer = setTimeout(() => res.destroy(), mode === 'socket' ? 80 : 8000);
      res.on('close', () => clearTimeout(timer));
    });
    await new Promise(r => server.listen(0, '127.0.0.1', r));
    server.on('upgrade', (req, socket) => {
      requests++; upgraded.add(socket); socket.on('close', () => upgraded.delete(socket));
      const accept = createHash('sha1').update(req.headers['sec-websocket-key'] + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');
      socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
      socket.once('data', () => {
        const body = Buffer.from(JSON.stringify({ type: 'response.created', response: { id: 'fixture', status: 'in_progress' } }));
        assert.ok(body.length < 126);
        socket.write(Buffer.concat([Buffer.from([0x81, body.length]), body]));
        const timer = setTimeout(() => socket.destroy(), 150);
        socket.on('close', () => clearTimeout(timer));
      });
    });
    const token = 'test.' + Buffer.from(JSON.stringify({ 'https://api.openai.com/auth': { chatgpt_account_id: 'fixture' } })).toString('base64url') + '.test';
    await writeFile(join(home, 'settings.json'), JSON.stringify({ transport: mode === 'websocket' ? 'auto' : 'sse', httpIdleTimeoutMs: 1200, retry: { enabled: false }, providerRetry: { maxRetries: 0 } }));
    await writeFile(join(home, 'models.json'), JSON.stringify({ providers: { 'local-codex': {
      baseUrl: `http://127.0.0.1:${server.address().port}`, api: 'openai-codex-responses', apiKey: token,
      models: [{ id: 'fixture', name: 'Fixture', reasoning: true, input: ['text'], cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }, contextWindow: 32000, maxTokens: 1000 }],
    } } }));
    const child = spawn(process.execPath, [resolve(cli), '--mode', 'rpc', '--offline', '--no-extensions', '--no-skills', '--no-context-files', '--no-prompt-templates', '--no-themes', '--provider', 'local-codex', '--model', 'fixture'], {
      cwd: project, env: { ...process.env, PI_CODING_AGENT_DIR: home, PI_CODING_AGENT_SESSION_DIR: join(home, 'sessions'), PI_TELEMETRY: '0', NO_PROXY: '127.0.0.1', no_proxy: '127.0.0.1' },
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    const events = [];
    const waiters = [];
    let lines;
    try {
      lines = createInterface({ input: child.stdout });
      lines.on('line', line => {
        try { const event = JSON.parse(line); events.push(event); for (const notify of [...waiters]) notify(); } catch { /* assertion below detects malformed protocol */ }
      });
      child.stderr.resume(); // Never retain child stderr; it may contain arbitrary provider text.
      const wait = (predicate, label) => new Promise((resolveWait, reject) => {
        const timer = setTimeout(() => { cleanup(); reject(new Error(`Timed out at ${label}; requests=${requests}; eventTypes=${events.map(e => e.type).join(',')}`)); }, 10000);
        const cleanup = () => { clearTimeout(timer); const index = waiters.indexOf(check); if (index >= 0) waiters.splice(index, 1); };
        const check = () => { const event = events.find(predicate); if (event) { cleanup(); resolveWait(event); } };
        waiters.push(check); check();
      });
      child.stdin.write(JSON.stringify({ id: 'before', type: 'get_state' }) + '\n');
      const before = await wait(e => e.type === 'response' && e.id === 'before', 'initial state');
      assert.equal(before.success, true);
      child.stdin.write(JSON.stringify({ id: 'prompt', type: 'prompt', message: 'Local transport test only.' }) + '\n');
      const failure = await wait(e => e.type === 'message_end' && (mode === 'websocket' ? /^WebSocket/.test(e.message?.errorMessage ?? '') : e.message?.errorMessage === 'terminated'), 'model error');
      assert.equal(failure.message.stopReason, 'error');
      await wait(e => e.type === 'agent_settled', 'agent settled');
      child.stdin.write(JSON.stringify({ id: 'after', type: 'get_state' }) + '\n');
      const after = await wait(e => e.type === 'response' && e.id === 'after', 'post-error state');
      assert.equal(after.success, true);
      assert.equal(after.data.sessionId, before.data.sessionId);
      assert.equal(after.data.isStreaming, false);
      assert.equal(requests, 1);
    } finally {
      lines?.close(); child.kill();
      if (child.exitCode === null && child.signalCode === null) await new Promise(r => child.once('exit', r));
      for (const socket of upgraded) socket.destroy();
      server.closeAllConnections(); await new Promise(r => server.close(r));
      await rm(dir, { recursive: true, force: true });
    }
  });
}
test('Codex transport applies the visible setting without changing other request options', () => {
  const signal = new AbortController().signal;
  const onPayload = () => undefined;
  const options = Object.freeze({ transport: 'auto', apiKey: 'fixture', headers: { test: 'fixture' }, signal, onPayload, sessionId: 'fixture', timeoutMs: 1234 });
  const stable = codexTransportOptions(options);
  assert.deepEqual(stable, { ...options, transport: 'sse' });
  assert.equal(options.transport, 'auto');
  assert.equal(stable.signal, signal);
  assert.equal(stable.onPayload, onPayload);
  assert.equal(stable.headers, options.headers);
  assert.equal(codexTransportOptions(undefined).transport, 'sse');
  for (const transport of ['auto', 'websocket', 'websocket-cached', 'sse']) {
    const selected = codexTransportOptions(options, transport);
    assert.deepEqual(selected, { ...options, transport });
    assert.equal(options.transport, 'auto');
  }
  assert.throws(() => codexTransportOptions(options, 'invalid'), /Unsupported/);
});
