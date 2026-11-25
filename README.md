# language-ai

Practice a language by chatting (text or voice) with a local AI. Each reply comes in the
language you are learning, with a translation into your own language, and is read aloud.

```
apps/
  server/   Rust (axum) API + serves the built web app (apps/web/dist)
  web/      React + Vite frontend
```

## Run

```sh
ollama pull lfm3-audio        # needs Ollama >= 0.40.12
(cd apps/web && npm i && npm run build)
cargo run                     # http://127.0.0.1:3000
# frontend dev with hot reload: cd apps/web && npm run dev  (proxies /api to :3000)
```

Env: `OLLAMA_URL` (default `http://localhost:11434`), `MODEL` (`lfm3-audio:latest`), `ADDR` (`127.0.0.1:3000`), `WEB_DIR`.

## How it works

| Endpoint | Ollama call |
|---|---|
| `POST /api/transcribe` (16kHz WAV body) → `{text}` | `/v1/audio/transcriptions` |
| `POST /api/chat` `{base, target, history, text}` → `{target, base}` | `/api/chat` ×2: reply in target language, then translate it |
| `POST /api/speak` `{text, voice}` → WAV | `/v1/audio/speech` |

The browser stores conversation history in localStorage, one conversation per language pair, and sends it with every turn.
The server keeps no state.

**Why not rig?** rig's Ollama provider only covers chat and embeddings. It has no transcription or speech support.
lfm3-audio's audio features are only available through Ollama's OpenAI-style `/v1/audio/*` routes, so the server calls them directly with `reqwest`.
Sending audio straight into chat (`input_audio`) gave empty replies, and the model has no structured output, so the server does STT → chat → translate → TTS as separate calls.
