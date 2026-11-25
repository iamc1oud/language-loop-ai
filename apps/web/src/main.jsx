import { useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import './style.css';

// Languages = the voices lfm3-audio ships (`ollama show lfm3-audio`).
// [native name, voice suffix, sample greeting for the setup preview]
const LANGS = {
  English: ['English', 'american-english', 'Hi! How was your day?'],
  Spanish: ['Español', 'spanish', '¡Hola! ¿Qué tal tu día?'],
  French: ['Français', 'french', 'Salut ! Comment s’est passée ta journée ?'],
  German: ['Deutsch', 'german', 'Hallo! Wie war dein Tag?'],
  Italian: ['Italiano', 'italian', 'Ciao! Com’è andata la giornata?'],
  Portuguese: ['Português', 'portuguese', 'Olá! Como foi o seu dia?'],
  Japanese: ['日本語', 'japanese', 'こんにちは！今日はどうでしたか？'],
  Chinese: ['中文', 'chinese', '你好！你今天过得怎么样？'],
  Korean: ['한국어', 'korean', '안녕하세요! 오늘 하루 어땠어요?'],
  Arabic: ['العربية', 'arabic', 'مرحبا! كيف كان يومك؟'],
  Dutch: ['Nederlands', 'dutch', 'Hoi! Hoe was je dag?'],
  Danish: ['Dansk', 'danish', 'Hej! Hvordan var din dag?'],
  Swedish: ['Svenska', 'swedish', 'Hej! Hur var din dag?'],
  Finnish: ['Suomi', 'finnish', 'Hei! Millainen päiväsi oli?'],
  Polish: ['Polski', 'polish', 'Cześć! Jak minął ci dzień?'],
  Greek: ['Ελληνικά', 'greek', 'Γεια! Πώς πέρασε η μέρα σου;'],
  Hebrew: ['עברית', 'hebrew', 'שלום! איך היה היום שלך?'],
  Turkish: ['Türkçe', 'turkish', 'Merhaba! Günün nasıl geçti?'],
  Tagalog: ['Tagalog', 'english', 'Kumusta! Kumusta ang araw mo?'],
};
const LEVELS = ['Beginner', 'Intermediate', 'Advanced'];
const MODES = ['Voice + text', 'Text only'];

const store = {
  get: (k, d) => { try { return JSON.parse(localStorage.getItem(k)) ?? d; } catch { return d; } },
  set: (k, v) => { try { localStorage.setItem(k, JSON.stringify(v)); } catch {} },
};

// Always same origin: the Rust server serves dist/, and `vite` dev proxies /api to it.
async function api(path, body, raw) {
  const r = await fetch(path, raw
    ? { method: 'POST', body, headers: { 'content-type': 'audio/wav' } }
    : { method: 'POST', body: JSON.stringify(body), headers: { 'content-type': 'application/json' } });
  if (!r.ok) throw new Error(await r.text());
  return r;
}
async function get(path) {
  const r = await fetch(path);
  if (!r.ok) throw new Error(await r.text());
  return r.json();
}

// One-time move of data saved by older versions in localStorage. Each item is dropped
// from localStorage right after the server stores it, so a retry never duplicates.
async function importLocal() {
  const old = store.get('convos', []);
  while (old.length) {
    const { base, target, level, mode, updated, messages } = old[old.length - 1]; // oldest first
    await api('/api/convos', { base, target, level, mode, updated, messages });
    old.pop();
    store.set('convos', old);
  }
  localStorage.removeItem('convos');
  for (const key of Object.keys(localStorage).filter((k) => k.startsWith('vocab:'))) {
    const lang = key.slice(6);
    for (const v of store.get(key, []).reverse()) await api('/api/vocab', { lang, ...v }); // newest ends on top
    localStorage.removeItem(key);
  }
}

function when(t) {
  const d = new Date(t);
  return d.toDateString() === new Date().toDateString()
    ? 'Today' : d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

// ---------- Audio ----------
let playing;
async function speak(text, lang, rate = 1) {
  const r = await api('/api/speak', { text, voice: `female-${LANGS[lang][1]}` });
  const url = URL.createObjectURL(await r.blob());
  playing?.pause();
  playing = new Audio(url);
  playing.playbackRate = rate;
  playing.onended = () => URL.revokeObjectURL(url);
  await playing.play();
}

// Resample to 16kHz mono WAV (what Ollama transcription wants).
async function toWav(blob) {
  const ctx = new AudioContext({ sampleRate: 16000 });
  const pcm = (await ctx.decodeAudioData(await blob.arrayBuffer())).getChannelData(0);
  ctx.close();
  const v = new DataView(new ArrayBuffer(44 + pcm.length * 2));
  const str = (o, s) => [...s].forEach((c, i) => v.setUint8(o + i, c.charCodeAt(0)));
  str(0, 'RIFF'); v.setUint32(4, 36 + pcm.length * 2, true); str(8, 'WAVE');
  str(12, 'fmt '); v.setUint32(16, 16, true); v.setUint16(20, 1, true); v.setUint16(22, 1, true);
  v.setUint32(24, 16000, true); v.setUint32(28, 32000, true); v.setUint16(32, 2, true); v.setUint16(34, 16, true);
  str(36, 'data'); v.setUint32(40, pcm.length * 2, true);
  pcm.forEach((s, i) => v.setInt16(44 + i * 2, Math.max(-1, Math.min(1, s)) * 0x7fff, true));
  return new Blob([v], { type: 'audio/wav' });
}

// ---------- Setup screen ----------
function Choices({ items, value, onPick, disabled, label = (x) => x }) {
  return items.map((x) => (
    <button key={x} type="button" aria-pressed={value === x} disabled={disabled === x} onClick={() => onPick(x)}>
      {label(x)}
    </button>
  ));
}

function Setup({ pick, setPick, note, onStart }) {
  const set = (key) => (x) => setPick((p) => {
    const n = { ...p, [key]: x };
    if (key === 'base' && n.target === x) n.target = x === 'English' ? 'Spanish' : 'English';
    return n;
  });
  const lang = (l) => <>{LANGS[l][0]}<small>{l}</small></>;
  return (
    <main id="setup" className="setup">
      <section>
        <p className="eyebrow">New practice session</p>
        <h1>What do you want to practise?</h1>
        <p className="muted">Pick the language you speak and the one you're learning. Every reply comes in both, so you always know what was said.</p>

        <fieldset><legend>1. I speak — base language</legend><div id="base-grid" className="grid">
          <Choices items={Object.keys(LANGS)} value={pick.base} onPick={set('base')} label={lang} />
        </div></fieldset>
        <fieldset><legend>2. I want to learn</legend><div id="target-grid" className="grid">
          <Choices items={Object.keys(LANGS)} value={pick.target} onPick={set('target')} disabled={pick.base} label={lang} />
        </div></fieldset>
        <fieldset><legend>3. My level</legend><div id="level" className="pills">
          <Choices items={LEVELS} value={pick.level} onPick={set('level')} />
        </div></fieldset>
        <fieldset><legend>4. Practice mode</legend><div id="mode" className="pills">
          <Choices items={MODES} value={pick.mode} onPick={set('mode')} />
        </div></fieldset>

        <div className="start">
          <button id="start" className="dark" onClick={onStart}>Start practising →</button>
          <span id="summary" className="muted">{note ?? `${pick.base} → ${pick.target} · ${pick.level} · ${pick.mode}`}</span>
        </div>
      </section>

      <aside>
        <div className="card">
          <h2>How replies will look</h2>
          <span className="tag blue">{pick.target}</span>
          <p className="big">{LANGS[pick.target][2]}</p>
          <hr />
          <span className="tag orange">{pick.base}</span>
          <p>{LANGS[pick.base][2]}</p>
        </div>
        <div className="card note">Your conversations are saved on this device, so the AI remembers what you talked about when you come back.</div>
      </aside>
    </main>
  );
}

// ---------- Practice screen ----------
// Intl.Segmenter also splits Japanese/Chinese, where spaces don't mark words.
function Words({ text, id, sel, onSelect }) {
  const seg = new Intl.Segmenter(undefined, { granularity: 'word' });
  return [...seg.segment(text)].map(({ segment, isWordLike }, i) => {
    if (!isWordLike) return segment;
    const key = `${id}:${i}`;
    return (
      <button key={key} type="button" className="w" aria-pressed={sel === key} onClick={() => onSelect(segment, key)}>
        {segment}
      </button>
    );
  });
}

function Message({ m, i, base, sel, onSelect, say }) {
  if (m.role === 'user') {
    return (
      <div className="me">
        <div className="bubble">{m.content}</div>
        <div className="meta">{m.voice ? 'Voice message · transcribed by lfm3-audio' : 'Typed'}</div>
      </div>
    );
  }
  return (
    <article className="ai">
      <div className="avatar">L</div>
      <div className="bubble">
        <div className="t"><Words text={m.content} id={i} sel={sel} onSelect={(w, k) => onSelect(w, k, m)} /></div>
        <hr />
        <div className="b"><span className="tag orange">{base.slice(0, 2)}</span>{m.translation}</div>
        <div className="actions">
          <button type="button" onClick={() => say(m.content)}>▶ Play</button>
          <button type="button" onClick={() => say(m.content, 0.75)}>🐢 Slower</button>
        </div>
      </div>
    </article>
  );
}

const glossCache = {};
function WordPanel({ word, convo, model, say, onSaved }) {
  const [gloss, setGloss] = useState('Looking up…');
  const [ok, setOk] = useState(false);
  const [saved, setSaved] = useState('');
  useEffect(() => {
    let live = true;
    const { target, base } = convo;
    const k = `${model}:${target}:${base}:${word.toLowerCase()}`;
    (glossCache[k] ??= api('/api/translate', { text: word, from: target, to: base, model }).then((r) => r.json()).then((j) => j.text))
      .then((g) => { if (live) { setGloss(g); setOk(true); } })
      .catch((e) => { delete glossCache[k]; if (live) setGloss(`Lookup failed: ${e.message}`); });
    return () => { live = false; };
  }, []); // keyed by the selected word, so it remounts per selection
  const keep = () => api('/api/vocab', { lang: convo.target, word, gloss })
    .then(() => { setSaved('✓ Saved'); onSaved(); })
    .catch((e) => setSaved(`Save failed: ${e.message}`));
  return (
    <>
      <p className="word">{word}</p>
      <p className="gloss">{gloss}</p>
      <button type="button" className="dashed" onClick={() => say(word)}>🔊 Hear it</button>
      <button type="button" className="dashed" style={{ marginTop: 8 }} disabled={!ok} onClick={keep}>{saved || '＋ Save to vocab'}</button>
    </>
  );
}

function App() {
  const [pick, setPick] = useState(() => ({ base: 'English', target: 'Spanish', level: 'Beginner', mode: 'Voice + text', ...store.get('pick', {}) }));
  const [note, setNote] = useState(null); // error shown in place of the setup summary
  const [convos, setConvos] = useState([]);
  const [curId, setCurId] = useState(null);
  const [practising, setPractising] = useState(false);
  const [typing, setTyping] = useState(null); // id of the convo awaiting a reply
  const [status, setStatus] = useState(['', false]); // [text, recording]
  const [sel, setSel] = useState(null); // { word, key, translation }
  const [vocab, setVocab] = useState([]);
  const [vocabTick, setVocabTick] = useState(0);
  const [models, setModels] = useState(['lfm3-audio:latest']);
  const [model, setModel] = useState('lfm3-audio:latest');
  const [text, setText] = useState('');
  const [rec, setRec] = useState(false);
  const logRef = useRef();
  const inputRef = useRef();
  const recorder = useRef();
  const live = useRef({}); // latest values for async callbacks
  const cur = convos.find((c) => c.id === curId);
  live.current = { curId, send };

  useEffect(() => { store.set('pick', pick); setNote(null); }, [pick]);

  useEffect(() => {
    importLocal()
      .catch((e) => console.error('localStorage import failed; will retry next load', e))
      .then(() => get('/api/convos'))
      .then((list) => setConvos((prev) => list.map((c) => prev.find((p) => p.id === c.id) ?? c))) // keep chats started meanwhile
      .catch((e) => setNote(`Couldn't load conversations: ${e.message}`));
    // Chat model picker: any installed Ollama model. Speech/transcription always use lfm3-audio (server default).
    get('/api/models').then(({ default: def, models }) => {
      const saved = store.get('chatModel', def);
      setModels(models);
      setModel(models.includes(saved) ? saved : def);
    }).catch(() => {});
  }, []);

  useEffect(() => {
    if (!practising) return;
    setStatus(['', false]);
    setSel(null);
    inputRef.current?.focus();
  }, [practising, curId]);

  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight;
  }, [practising, curId, cur?.messages.length, typing]);

  const target = practising ? cur?.target : null;
  useEffect(() => {
    if (!target) return;
    let ok = true;
    get(`/api/vocab?lang=${encodeURIComponent(target)}`).catch(() => []).then((l) => ok && setVocab(l));
    return () => { ok = false; }; // switched to another language meanwhile
  }, [target, vocabTick]);

  const say = (t, rate) => speak(t, cur.target, rate).catch((e) => setStatus([`Audio error: ${e.message}`, false]));

  function open(c) { setCurId(c.id); setPractising(true); }

  async function newConvo() {
    try {
      const { base, target, level, mode } = pick;
      const c = await (await api('/api/convos', { base, target, level, mode })).json();
      setConvos((cs) => [c, ...cs]);
      open(c);
    } catch (e) {
      setNote(`Couldn't start: ${e.message}`);
    }
  }

  function add(id, m) {
    setConvos((cs) => {
      const c = cs.find((x) => x.id === id);
      return [{ ...c, messages: [...c.messages, m], updated: Date.now() }, ...cs.filter((x) => x !== c)];
    });
  }

  async function send(t, voice) {
    t = t.trim();
    if (!t) return;
    const c = cur;
    add(c.id, { role: 'user', content: t, voice });
    setTyping(c.id);
    setStatus(['', false]);
    try {
      const r = await api('/api/chat', { convo: c.id, text: t, voice: !!voice, model });
      const { target, base } = await r.json();
      add(c.id, { role: 'assistant', content: target, translation: base });
      if (c.mode !== 'Text only' && live.current.curId === c.id) say(target);
    } catch (e) {
      setStatus([`Error: ${e.message}`, false]);
    } finally {
      setTyping((x) => (x === c.id ? null : x));
    }
  }

  // Mic: record → 16kHz WAV → transcribe → send.
  async function mic() {
    if (recorder.current?.state === 'recording') return recorder.current.stop();
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const chunks = [];
      const r = recorder.current = new MediaRecorder(stream);
      r.ondataavailable = (e) => chunks.push(e.data);
      r.onstop = async () => {
        stream.getTracks().forEach((t) => t.stop());
        setRec(false);
        setStatus(['Transcribing…', false]);
        try {
          const { text } = await (await api('/api/transcribe', await toWav(new Blob(chunks)), true)).json();
          setStatus(['', false]);
          live.current.send(text, true);
        } catch (e) {
          setStatus([`Error: ${e.message}`, false]);
        }
      };
      r.start();
      setRec(true);
      setStatus([`Listening in ${cur.target}… tap the mic again to send`, true]);
    } catch (e) {
      setStatus([`Mic error: ${e.message}`, false]);
    }
  }

  const textOnly = cur?.mode === 'Text only';
  return (
    <>
      <header className="top">
        <a href="#" id="home" className="logo" aria-label="Lingo Loop home" onClick={(e) => { e.preventDefault(); setPractising(false); }}>
          <svg width="30" height="30" viewBox="0 0 30 30" aria-hidden="true"><path d="M10 9.5a5.5 5.5 0 1 0 0 11c3.5 0 6.5-11 10-11a5.5 5.5 0 1 1 0 11c-3.5 0-6.5-11-10-11z" fill="none" stroke="#2F54EB" strokeWidth="2.6" /></svg>
          Lingo Loop
        </a>
        {practising && cur && <span id="pair-pill" className="pill">{`${cur.base} → ${cur.target} · ${cur.level}`}</span>}
        <label className="pill"><i className="dot" />Chat model
          <select id="model" aria-label="Chat model" value={model} onChange={(e) => { setModel(e.target.value); store.set('chatModel', e.target.value); }}>
            {models.map((m) => <option key={m} value={m}>{m}</option>)}
          </select>
        </label>
        <span className="pill">Voice · lfm3-audio</span>
      </header>

      {!practising || !cur ? <Setup pick={pick} setPick={setPick} note={note} onStart={newConvo} /> : (
        <div id="practice" className="practice">
          <nav className="side">
            <button id="new" className="dashed" onClick={() => setPractising(false)}>+ New conversation</button>
            <h3>History</h3>
            <ul id="history">
              {convos.map((c) => (
                <li key={c.id}>
                  <button type="button" aria-current={c.id === curId} onClick={() => open(c)}>
                    <span className="title">{c.messages.find((m) => m.role === 'user')?.content || 'New conversation'}</span>
                    <small>{`${c.base} → ${c.target} · ${when(c.updated)} · ${c.messages.length} messages`}</small>
                  </button>
                </li>
              ))}
            </ul>
          </nav>

          <main className="chat">
            <div id="log" className="log" aria-live="polite" ref={logRef}>
              {!cur.messages.length && <p className="muted hint">{`Say hi in ${cur.target} — or in ${cur.base} if you're stuck.`}</p>}
              {cur.messages.map((m, i) => (
                <Message key={i} m={m} i={i} base={cur.base} sel={sel?.key} say={say}
                  onSelect={(word, key, msg) => setSel({ word, key, translation: msg.translation })} />
              ))}
              {typing === cur.id && (
                <article className="ai typing"><div className="avatar">L</div><div className="bubble">Thinking…</div></article>
              )}
            </div>
            <form id="form" className="composer" onSubmit={(e) => { e.preventDefault(); send(text); setText(''); }}>
              <p id="status" className={status[1] ? 'status rec' : 'status'} role="status">{status[0]}</p>
              <div className="row">
                {!textOnly && (
                  <button type="button" id="mic" className={rec ? 'mic rec' : 'mic'} onClick={mic}
                    aria-label={rec ? 'Stop recording and send' : 'Record voice message'}>
                    <svg width="22" height="22" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 15a3 3 0 0 0 3-3V6a3 3 0 0 0-6 0v6a3 3 0 0 0 3 3zm5-3a5 5 0 0 1-10 0H5a7 7 0 0 0 6 6.9V21h2v-2.1A7 7 0 0 0 19 12z" fill="currentColor" /></svg>
                  </button>
                )}
                <div className="input">
                  <input id="text" ref={inputRef} autoComplete="off" aria-label="Message" value={text} onChange={(e) => setText(e.target.value)}
                    placeholder={textOnly ? `Write in ${cur.target}…` : `Type in ${cur.target} or tap the mic…`} />
                  <button className="send" aria-label="Send">↑</button>
                </div>
              </div>
            </form>
          </main>

          <aside className="detail">
            <section>
              <h3>Selected word</h3>
              <div id="word-box">
                {sel
                  ? <WordPanel key={sel.key} word={sel.word} convo={cur} model={model} say={say} onSaved={() => setVocabTick((n) => n + 1)} />
                  : <p className="muted">Tap any word in a reply to see what it means.</p>}
              </div>
            </section>
            <section>
              <h3>In this sentence</h3>
              <p id="sentence" className="muted">{sel ? sel.translation : '—'}</p>
            </section>
            <section>
              <h3>Session memory</h3>
              <p id="memory" className="muted">{`${cur.messages.length} messages kept — the AI sees the whole conversation every turn.`}</p>
            </section>
            <section>
              <h3>My vocab</h3>
              <ul id="vocab" className="vocab">
                {vocab.length
                  ? vocab.map((v) => <li key={v.word}>{`${v.word} `}<span>{`— ${v.gloss}`}</span></li>)
                  : <li className="muted">No saved words yet.</li>}
              </ul>
            </section>
          </aside>
        </div>
      )}
    </>
  );
}

// No StrictMode: its double-run effects would run the localStorage import twice.
createRoot(document.getElementById('root')).render(<App />);
