import assert from 'node:assert/strict';
import { test } from 'node:test';
import { channel } from 'node:diagnostics_channel';
import http from 'node:http';
import { installTransportObserver } from '../src-tauri/resources/pi-transport-observer.mjs';
import { codexTransportOptions } from '../src-tauri/resources/pi-codex-transport.mjs';

const publish = (name, value) => channel(`undici:${name}`).publish(value);
const request = () => ({ path: '/backend-api/codex/responses?secret=DO_NOT_LOG', headers: 'Bearer DO_NOT_LOG', body: 'DO_NOT_LOG' });

test('preserves separate causes and request timings, without recording payloads or credentials', () => {
  const events = [];
  let now = 0;
  const stop = installTransportObserver(event => events.push(event), () => now);
  try {
    const first = request(), second = request();
    publish('request:create', { request: first });
    now = 50;
    publish('request:create', { request: second });
    publish('request:headers', { request: first, response: { statusCode: 200, headers: 'DO_NOT_LOG' } });
    now = 90;
    publish('request:bodyChunkReceived', { request: first, chunk: Buffer.from('DO_NOT_LOG') });
    now = 150;
    publish('request:error', { request: first, error: new TypeError('terminated DO_NOT_LOG', { cause: { code: 'UND_ERR_BODY_TIMEOUT', address: 'DO_NOT_LOG' } }) });
    publish('request:error', { request: second, error: { code: 'ECONNRESET', message: 'DO_NOT_LOG' } });
    assert.deepEqual(events.map(e => e.transport), [
      { protocol: 'sse', phase: 'body', cause: 'UND_ERR_BODY_TIMEOUT', durationMs: 150, idleMs: 60, closeCode: null },
      { protocol: 'sse', phase: 'headers', cause: 'ECONNRESET', durationMs: 100, idleMs: null, closeCode: null },
    ]);
    assert.ok(!JSON.stringify(events).includes('DO_NOT_LOG'));
  } finally { stop(); }
});

test('ignores cancellation, completed requests and unrelated traffic, and unsubscribes', () => {
  const events = [];
  const stop = installTransportObserver(e => events.push(e));
  const cancelled = request(), completed = request(), unrelated = { path: '/private' };
  for (const req of [cancelled, completed, unrelated]) publish('request:create', { request: req });
  publish('request:trailers', { request: completed });
  publish('request:error', { request: cancelled, error: { code: 'UND_ERR_ABORTED' } });
  for (const req of [completed, unrelated]) publish('request:error', { request: req, error: { code: 'UND_ERR_SOCKET' } });
  stop();
  const after = request();
  publish('request:create', { request: after });
  publish('request:error', { request: after, error: { code: 'UND_ERR_SOCKET' } });
  assert.equal(events.length, 0);
});

test('records only abnormal Codex WebSocket closes, never free-form reasons', () => {
  const events = [];
  const stop = installTransportObserver(e => events.push(e));
  try {
    for (const code of [1000, 1006]) {
      const websocket = { url: 'wss://example.invalid/backend-api/codex/responses?DO_NOT_LOG' };
      publish('websocket:open', { websocket });
      publish('websocket:close', { websocket, code, reason: 'DO_NOT_LOG' });
    }
    assert.equal(events.length, 1);
    assert.equal(events[0].transport.cause, 'WS_ABNORMAL_CLOSE');
    assert.equal(events[0].transport.closeCode, 1006);
    assert.ok(!JSON.stringify(events).includes('DO_NOT_LOG'));
  } finally { stop(); }
});

test('observer failures cannot interrupt the request path', () => {
  const stop = installTransportObserver(() => { throw new Error('test sink failure'); });
  try {
    const req = request();
    assert.doesNotThrow(() => {
      publish('request:create', { request: req });
      publish('request:error', { request: req, error: { code: 'UND_ERR_SOCKET' } });
    });
  } finally { stop(); }
});

test('real local SSE socket drop preserves fetch semantics and emits its actual cause', async () => {
  process.env.NO_PROXY = '127.0.0.1'; process.env.no_proxy = '127.0.0.1';
  let count = 0;
  const server = http.createServer((_req, res) => {
    count++;
    res.writeHead(200, { 'Content-Type': 'text/event-stream' });
    res.write('data: DO_NOT_LOG\n\n');
    setTimeout(() => res.destroy(), 70);
  });
  const originalFetch = globalThis.fetch;
  const originalWebSocket = globalThis.WebSocket;
  const events = [];
  const stop = installTransportObserver(e => events.push(e));
  await new Promise(r => server.listen(0, '127.0.0.1', r));
  try {
    const res = await fetch(`http://127.0.0.1:${server.address().port}/codex/responses`, { headers: { Authorization: 'Bearer DO_NOT_LOG' } });
    await assert.rejects(res.text(), e => e.message === 'terminated' && e.cause?.code === 'UND_ERR_SOCKET');
    assert.equal(events.length, 1);
    assert.equal(events[0].transport.cause, 'UND_ERR_SOCKET');
    assert.equal(events[0].transport.phase, 'body');
    assert.equal(count, 1, 'observer must never resend a request');
    assert.equal(globalThis.fetch, originalFetch);
    assert.equal(globalThis.WebSocket, originalWebSocket);
    assert.ok(!JSON.stringify(events).includes('DO_NOT_LOG'));
  } finally {
    stop(); server.closeAllConnections(); await new Promise(r => server.close(r));
  }
});

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
