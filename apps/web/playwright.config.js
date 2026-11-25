// UI tests: every request is intercepted (built files from dist/, /api/* faked), so no
// server or Ollama is needed and the real database is never touched. `npm test` builds first.
export default {
  testDir: 'tests',
  use: { browserName: 'chromium' },
};
