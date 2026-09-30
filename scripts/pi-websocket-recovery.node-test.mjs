import assert from 'node:assert/strict';
import { test } from 'node:test';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import http from 'node:http';
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { createInterface } from 'node:readline';

const cli = process.env.PI_RPC_TEST_CLI;
const proxy = process.env.PI_TEST_RELAY_PROXY;
const item = { type: 'message', id: 'fixture-message', role: 'assistant', status: 'completed', content: [{ type: 'output_text', text: 'RECOVERED', annotations: [] }] };
const completed = [
  { type: 'response.created', response: { id: 'fixture', status: 'in_progress' } },
  { type: 'response.output_item.added', output_index: 0, item: { ...item, content: [], status: 'in_progress' } },
  { type: 'response.content_part.added', output_index: 0, content_index: 0, part: { type: 'output_text', text: '', annotations: [] } },
  { type: 'response.output_text.delta', output_index: 0, content_index: 0, delta: 'RECOVERED' },
  { type: 'response.output_item.done', output_index: 0, item },
  { type: 'response.completed', response: { id: 'fixture', status: 'completed', output: [item], usage: { input_tokens: 1, output_tokens: 1, total_tokens: 2 } } },
];

for (const scenario of ['handshake-rejected', 'stream-interrupted']) {
  for (const policy of ['native-auto', 'sse', 'auto', 'websocket', 'websocket-cached']) {
  const stable = policy === 'sse';
  const extension = policy !== 'native-auto';
  test(`Codex ${policy} completes despite ${scenario}${proxy ? ' through DeepPi relay' : ''}`, { skip: !cli, timeout: 20000 }, async () => {
    const dir = await mkdtemp(join(process.env.PI_SCRATCH_DIR || tmpdir(), 'deeppi-ws-recovery-'));
    const home = join(dir, 'agent'), project = join(dir, 'project');
    await mkdir(home); await mkdir(project);
    const sockets = new Set();
    let wsAttempts = 0, sseAttempts = 0, malformed = 0;
    const server = http.createServer((req, res) => {
      sseAttempts++; req.resume();
      res.writeHead(200, { 'Content-Type': 'text/event-stream' });
      res.end(completed.map(e => `data: ${JSON.stringify(e)}\n\n`).join(''));
    });
    server.on('upgrade', (req, socket) => {
      wsAttempts++; sockets.add(socket); socket.on('close', () => sockets.delete(socket));
      if (scenario === 'handshake-rejected') {
        socket.end('HTTP/1.1 426 Upgrade Required\r\nContent-Length: 0\r\nConnection: close\r\n\r\n');
        return;
      }
      const accept = createHash('sha1').update(req.headers['sec-websocket-key'] + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');
      socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
      socket.once('data', () => {
        const event = Buffer.from(JSON.stringify(completed[0]));
        socket.write(Buffer.concat([Buffer.from([0x81, event.length]), event]));
        const timer = setTimeout(() => socket.destroy(), 100);
        socket.on('close', () => clearTimeout(timer));
      });
    });
    await new Promise(r => server.listen(0, '127.0.0.1', r));
    const token = 'test.' + Buffer.from(JSON.stringify({ 'https://api.openai.com/auth': { chatgpt_account_id: 'fixture' } })).toString('base64url') + '.test';
    await writeFile(join(home, 'settings.json'), JSON.stringify({ transport: extension ? 'sse' : 'auto', retry: { enabled: !stable, maxRetries: 2, baseDelayMs: 10 }, httpIdleTimeoutMs: 2000 }));
    await writeFile(join(home, 'models.json'), JSON.stringify({ providers: { 'openai-codex': {
      api: 'openai-codex-responses', apiKey: token, baseUrl: `http://127.0.0.1:${server.address().port}`,
      models: [{ id: 'fixture', name: 'Fixture', reasoning: false, input: ['text'], cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }, contextWindow: 32000, maxTokens: 1000 }],
    } } }));
    const env = { ...process.env, PI_CODING_AGENT_DIR: home, PI_CODING_AGENT_SESSION_DIR: join(home, 'sessions'), PI_TELEMETRY: '0', NO_PROXY: proxy ? '' : '127.0.0.1', no_proxy: proxy ? '' : '127.0.0.1' };
    for (const key of ['HTTP_PROXY', 'HTTPS_PROXY', 'http_proxy', 'https_proxy', 'ALL_PROXY', 'all_proxy']) { delete env[key]; }
    if (proxy) Object.assign(env, { HTTP_PROXY: proxy, HTTPS_PROXY: proxy, NODE_USE_ENV_PROXY: '1' });
    if (extension) {
      args.push('--extension', resolve('src-tauri/resources/pi-codex-transport.mjs'));
      env.DEEPPI_CODEX_API_MODULE = pathToFileURL(join(dirname(dirname(dirname(resolve(cli)))), 'node_modules/@earendil-works/pi-ai/dist/api/openai-codex-responses.js')).href;
      env.DEEPPI_CODEX_TRANSPORT = policy;
    }
    const child = spawn(process.execPath, args, { cwd: project, env, stdio: ['pipe', 'pipe', 'pipe'] });
    const events = [];
    const lines = createInterface({ input: child.stdout });
    const waiters = new Set();
    lines.on('line', line => { try { events.push(JSON.parse(line)); for (const notify of waiters) notify(); } catch { malformed++; } });
    child.stderr.resume();
    const wait = (predicate, label) => new Promise((resolveWait, reject) => {
      const timer = setTimeout(() => { waiters.delete(check); reject(new Error(`Timeout at ${label}; ws=${wsAttempts}; sse=${sseAttempts}; types=${events.map(e => e.type).join(',')}`)); }, 10000);
      const check = () => { const event = events.find(predicate); if (event) { clearTimeout(timer); waiters.delete(check); resolveWait(event); } };
      waiters.add(check); check();
    });
    try {
      child.stdin.write(JSON.stringify({ id: 'before', type: 'get_state' }) + '\n');
      const before = await wait(e => e.id === 'before', 'initial state');
      assert.equal(before.success, true);
      const settingsBefore = await readFile(join(home, 'settings.json'), 'utf8');
      child.stdin.write(JSON.stringify({ id: 'prompt', type: 'prompt', message: 'Reply RECOVERED. Local test only.' }) + '\n');
      await wait(e => e.type === 'agent_settled', 'settled');
      const successes = events.filter(e => e.type === 'message_end' && e.message?.role === 'assistant' && e.message.stopReason === 'stop');
      assert.equal(successes.length, 1, 'one final successful reply');
      assert.equal(successes[0].message.content.find(e => e.type === 'text')?.text, 'RECOVERED');
      assert.equal(wsAttempts, stable ? 0 : 1);
      assert.equal(sseAttempts, 1);
      assert.equal(events.filter(e => e.type === 'auto_retry_start').length, !stable && scenario === 'stream-interrupted' ? 1 : 0);
      if (!stable && scenario === 'stream-interrupted') assert.ok(events.some(e => e.type === 'auto_retry_end' && e.success === true));
      child.stdin.write(JSON.stringify({ id: 'after', type: 'get_state' }) + '\n');
      const after = await wait(e => e.id === 'after', 'post recovery state');
      assert.equal(after.success, true);
      assert.equal(after.data.sessionId, before.data.sessionId);
      assert.equal(after.data.isStreaming, false);
      assert.equal(malformed, 0);
      assert.equal(await readFile(join(home, 'settings.json'), 'utf8'), settingsBefore, 'transport choice must not rewrite personal settings');
      assert.equal(after.data.model.provider, 'openai-codex');
      assert.equal(after.data.model.id, 'fixture');
    } finally {
      lines.close(); child.kill();
      if (child.exitCode === null && child.signalCode === null) await new Promise(r => child.once('exit', r));
      for (const socket of sockets) socket.destroy();
      server.closeAllConnections(); await new Promise(r => server.close(r));
      await rm(dir, { recursive: true, force: true });
    }
  });
  }
}
