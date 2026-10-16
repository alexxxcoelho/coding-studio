// Runtime tests only. These do not replace a real browser / native app check.
import { readFileSync } from 'node:fs';
import { webcrypto, createHash } from 'node:crypto';
import { runInNewContext } from 'node:vm';
import assert from 'node:assert/strict';
import { test } from 'node:test';

const html = readFileSync(process.env.READER_FIXTURE || '/private/tmp/coding-studio-reviewer-test.html', 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
const encoded = html.match(/<div id="encrypted" hidden>([^<]+)<\/div>/)[1];

function environment() {
  class Element {
    constructor(tag = 'div') { this.tag = tag; this.hidden = false; this.value = ''; this.textContent = ''; this.children = []; this.handlers = {}; this.attributes = {}; }
    addEventListener(type, callback) { this.handlers[type] = callback; }
    append(...elements) { this.children.push(...elements); }
    replaceChildren(...elements) { this.children = elements; }
    setAttribute(key, value) { this.attributes[key] = value; }
    focus() { this.focused = true; }
  }
  const nodes = new Map();
  const get = id => { if (!nodes.has(id)) nodes.set(id, new Element()); return nodes.get(id); };
  get('encrypted').textContent = encoded;
  get('reader').hidden = true;
  runInNewContext(script, {
    document: { getElementById: get, createElement: tag => new Element(tag) },
    crypto: webcrypto, atob, Uint8Array, TextEncoder, TextDecoder,
  });
  return { get, submit: async password => {
    get('password').value = password;
    await get('form').handlers.submit({preventDefault() {}});
  }};
}

test('CSP matches the exact script and permits only same-origin attachments', () => {
  const hash = createHash('sha256').update(script).digest('base64');
  assert.ok(html.includes(`script-src 'sha256-${hash}'`));
  assert.ok(html.includes("connect-src 'self'"));
  assert.ok(!html.includes('NOM EXCLU DU TEST'));
  assert.ok(!html.includes('synthetic test password 12345'));
});

test('WebCrypto decrypts the Rust envelope; source HTML remains text', async () => {
  const {get, submit} = environment();
  await submit('synthetic test password 12345');
  assert.equal(get('reader').hidden, false);
  assert.equal(get('gate').hidden, true);
  assert.equal(get('password').value, '');
  assert.equal(get('count').textContent, '5 champs sur 5');
  get('search').value = 'onerror';
  get('search').handlers.input();
  const results = get('fields').children;
  assert.equal(results.length, 1);
  assert.equal(results[0].children[1].tag, 'div');
  assert.match(results[0].children[1].textContent, /<img src=x onerror=alert\(1\)>/);
  get('search').value = 'NOM EXCLU DU TEST';
  get('search').handlers.input();
  assert.equal(get('fields').children.length, 0);
  assert.equal(get('empty').hidden, false);
  get('lock').handlers.click();
  assert.equal(get('reader').hidden, true);
  assert.equal(get('fields').children.length, 0);
  assert.equal(get('navigation').children.length, 0);
});

test('wrong password does not expose the reader', async () => {
  const {get, submit} = environment();
  await submit('wrong password');
  assert.equal(get('reader').hidden, true);
  assert.match(get('status').textContent, /mot de passe incorrect/);
  assert.equal(get('password').value, '');
  assert.equal(get('unlock').disabled, false);
});
