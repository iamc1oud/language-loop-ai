// @ts-check
const { test, expect } = require('@playwright/test');
const path = require('node:path');

const WEB = path.join(__dirname, '..', 'dist');
// Nothing actually listens here: see `fake`.
const URL = 'http://127.0.0.1:3000/';

const convo = (id, extra = {}) => ({
  id, base: 'English', target: 'Spanish', level: 'Beginner', mode: 'Text only', updated: id, messages: [], ...extra,
});

/**
 * Intercepts every request: built files come from dist/, /api/* from an in-memory fake of
 * the Rust server, off-site requests (fonts) are aborted. Unknown API calls get 500, never the network, so a test can't reach the
 * user's real server or database. Returns the list of API calls made.
 */
async function fake(page, { convos = [], fail = {} } = {}) {
  const calls = [];
  const db = { convos: [...convos], vocab: {}, nextId: 100 };
  await page.route('**/*', async (route) => {
    const req = route.request();
    const url = new globalThis.URL(req.url());
    if (url.origin !== new globalThis.URL(URL).origin) return route.abort(); // web fonts etc.
    if (!url.pathname.startsWith('/api/')) {
      const file = url.pathname === '/' ? 'index.html' : url.pathname.slice(1);
      return route.fulfill({ path: path.join(WEB, file) });
    }
    const key = `${req.method()} ${url.pathname}`;
    const body = req.method() === 'POST' ? req.postDataJSON() : undefined;
    calls.push({ key, body, query: url.search });
    if (fail[key]) return route.fulfill({ status: fail[key][0], body: fail[key][1] });
    const json = (j) => route.fulfill({ json: j });
    switch (key) {
      case 'GET /api/models': return json({ default: 'm:1', models: ['m:1', 'm:2'] });
      case 'GET /api/convos': return json(db.convos);
      case 'POST /api/convos': {
        const { messages = [], updated = Date.now(), ...settings } = body;
        const c = { id: db.nextId++, ...settings, updated, messages };
        db.convos.unshift(c);
        return json(c);
      }
      case 'POST /api/chat': {
        const reply = `Hola amigo ${calls.filter((c) => c.key === key).length}`;
        return json({ target: reply, base: `TR:${reply}` });
      }
      case 'POST /api/translate': return json({ text: `gloss:${body.text}` });
      case 'GET /api/vocab': return json(db.vocab[url.searchParams.get('lang')] ?? []);
      case 'POST /api/vocab': {
        const { lang, word, gloss } = body;
        db.vocab[lang] = [{ word, gloss }, ...(db.vocab[lang] ?? []).filter((v) => v.word !== word)];
        return route.fulfill({ status: 204 });
      }
    }
    return route.fulfill({ status: 500, body: `unfaked ${key}` });
  });
  return calls;
}

/** Load the app and wait until conversations have been fetched. */
async function open(page) {
  await Promise.all([
    page.waitForResponse((r) => r.url().endsWith('/api/convos') && r.request().method() === 'GET'),
    page.goto(URL),
  ]);
}

async function startTextChat(page) {
  await page.locator('#mode button', { hasText: 'Text only' }).click();
  await page.click('#start');
  await expect(page.locator('#practice')).toBeVisible();
}

const sent = (calls, key) => calls.filter((c) => c.key === key).map((c) => c.body);

test('starting a conversation creates it on the server with the picked settings', async ({ page }) => {
  const calls = await fake(page);
  await open(page);
  await startTextChat(page);
  expect(sent(calls, 'POST /api/convos')).toEqual([
    { base: 'English', target: 'Spanish', level: 'Beginner', mode: 'Text only' },
  ]);
  await expect(page.locator('#pair-pill')).toHaveText('English → Spanish · Beginner');
});

// Regression for "missing field `convo`": the chat body must reference the stored
// conversation, not ship the history.
test('chat sends the conversation id, not the history, and renders reply + translation', async ({ page }) => {
  const calls = await fake(page);
  await open(page);
  await startTextChat(page);
  await page.fill('#text', 'hola');
  await page.press('#text', 'Enter');
  await expect(page.locator('.ai .t').last()).toHaveText('Hola amigo 1');
  await expect(page.locator('.ai .b').last()).toContainText('TR:Hola amigo 1');

  await page.fill('#text', 'qué tal');
  await page.press('#text', 'Enter');
  await expect(page.locator('.ai .t').last()).toHaveText('Hola amigo 2');

  expect(sent(calls, 'POST /api/chat')).toEqual([
    { convo: 100, text: 'hola', voice: false, model: 'm:1' },
    { convo: 100, text: 'qué tal', voice: false, model: 'm:1' },
  ]);
  await expect(page.locator('.me .bubble')).toHaveText(['hola', 'qué tal']);
});

test('saved conversations appear in history and reopen with their messages', async ({ page }) => {
  const old = convo(7, {
    target: 'French',
    messages: [
      { role: 'user', content: 'bonjour', voice: false },
      { role: 'assistant', content: 'Salut !', translation: 'Hi!', voice: false },
    ],
  });
  await fake(page, { convos: [old] });
  await open(page);
  await startTextChat(page);
  const items = page.locator('#history li');
  await expect(items).toHaveCount(2);
  await items.filter({ hasText: 'bonjour' }).click();
  await expect(page.locator('#pair-pill')).toHaveText('English → French · Beginner');
  await expect(page.locator('.me .bubble')).toHaveText(['bonjour']);
  await expect(page.locator('.ai .b')).toContainText('Hi!');
});

test('a failed chat shows the error and keeps the user message', async ({ page }) => {
  await fake(page, { fail: { 'POST /api/chat': [502, 'ollama down'] } });
  await open(page);
  await startTextChat(page);
  await page.fill('#text', 'hola');
  await page.press('#text', 'Enter');
  await expect(page.locator('#status')).toHaveText('Error: ollama down');
  await expect(page.locator('.me .bubble')).toHaveText(['hola']);
  await expect(page.locator('.ai.typing')).toHaveCount(0);
});

test('tapping a word looks it up; saving posts it and refreshes the vocab list', async ({ page }) => {
  const calls = await fake(page);
  await open(page);
  await startTextChat(page);
  await page.fill('#text', 'hola');
  await page.press('#text', 'Enter');
  await page.locator('.ai .w', { hasText: 'amigo' }).click();
  await expect(page.locator('#word-box .gloss')).toHaveText('gloss:amigo');
  expect(sent(calls, 'POST /api/translate')).toEqual([{ text: 'amigo', from: 'Spanish', to: 'English', model: 'm:1' }]);

  await page.click('text=＋ Save to vocab');
  await expect(page.locator('#vocab li')).toHaveText(['amigo — gloss:amigo']);
  expect(sent(calls, 'POST /api/vocab')).toEqual([{ lang: 'Spanish', word: 'amigo', gloss: 'gloss:amigo' }]);
});

test('old localStorage data is imported oldest-first, then removed', async ({ page }) => {
  const msgs = [{ role: 'user', content: 'hola', voice: true }];
  // Old app stored newest first.
  const stored = {
    convos: [convo(2, { messages: msgs }), convo(1, { target: 'French' })],
    'vocab:Spanish': [{ word: 'gato', gloss: 'cat' }, { word: 'perro', gloss: 'dog' }],
    pick: { base: 'English', target: 'German', level: 'Advanced', mode: 'Text only' },
  };
  await page.addInitScript((s) => {
    if (sessionStorage.getItem('seeded')) return; // only before the first load
    sessionStorage.setItem('seeded', '1');
    for (const [k, v] of Object.entries(s)) localStorage.setItem(k, JSON.stringify(v));
  }, stored);
  const calls = await fake(page);
  await open(page);

  const posted = sent(calls, 'POST /api/convos');
  expect(posted.map((c) => c.target)).toEqual(['French', 'Spanish']);
  expect(posted[1]).toEqual({ base: 'English', target: 'Spanish', level: 'Beginner', mode: 'Text only', updated: 2, messages: msgs });
  expect(sent(calls, 'POST /api/vocab').map((v) => v.word)).toEqual(['perro', 'gato']);

  const left = await page.evaluate(() => Object.keys(localStorage).sort());
  expect(left).toEqual(['pick']); // preferences stay local

  await page.reload();
  expect(sent(calls, 'POST /api/convos')).toHaveLength(2); // not imported twice
});

test('if the import fails, unimported conversations stay in localStorage', async ({ page }) => {
  await page.addInitScript((c) => {
    if (!sessionStorage.getItem('seeded')) {
      sessionStorage.setItem('seeded', '1');
      localStorage.setItem('convos', JSON.stringify(c));
    }
  }, [convo(1)]);
  await fake(page, { fail: { 'POST /api/convos': [500, 'disk full'] } });
  await open(page);
  const left = await page.evaluate(() => JSON.parse(localStorage.getItem('convos')));
  expect(left.map((c) => c.id)).toEqual([1]);
});
