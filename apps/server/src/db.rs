//! SQLite persistence for conversations and saved vocabulary.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation: Option<String>,
    #[serde(default)]
    pub voice: bool,
}

/// Settings picked on the setup screen; fixed for the life of a conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub base: String,
    pub target: String,
    pub level: String,
    pub mode: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Convo {
    pub id: i64,
    #[serde(flatten)]
    pub settings: Settings,
    pub updated: i64,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub word: String,
    pub gloss: String,
}

#[derive(Clone)]
pub struct Db(Arc<Mutex<rusqlite::Connection>>);

pub type DbResult<T> = rusqlite::Result<T>;

const SCHEMA: &str = "
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS convos (
    id INTEGER PRIMARY KEY,
    base TEXT NOT NULL, target TEXT NOT NULL, level TEXT NOT NULL, mode TEXT NOT NULL,
    updated INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY,
    convo INTEGER NOT NULL REFERENCES convos(id) ON DELETE CASCADE,
    role TEXT NOT NULL, content TEXT NOT NULL, translation TEXT, voice INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS messages_convo ON messages(convo);
-- INSERT OR REPLACE gives a re-saved word a new rowid, so rowid order = newest first.
CREATE TABLE IF NOT EXISTS vocab (
    lang TEXT NOT NULL, word TEXT NOT NULL, gloss TEXT NOT NULL,
    PRIMARY KEY (lang, word)
);
";

// ponytail: one connection behind a mutex; fine for a single local user, pool if that changes.
impl Db {
    /// `":memory:"` for tests.
    pub fn open(path: &str) -> DbResult<Self> {
        let c = rusqlite::Connection::open(path)?;
        c.execute_batch(SCHEMA)?;
        Ok(Self(Arc::new(Mutex::new(c))))
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn create_convo(&self, s: &Settings, now: i64) -> DbResult<Convo> {
        let c = self.conn();
        c.execute(
            "INSERT INTO convos (base, target, level, mode, updated) VALUES (?1, ?2, ?3, ?4, ?5)",
            (&s.base, &s.target, &s.level, &s.mode, now),
        )?;
        Ok(Convo { id: c.last_insert_rowid(), settings: s.clone(), updated: now, messages: vec![] })
    }

    pub fn convo(&self, id: i64) -> DbResult<Option<Convo>> {
        Ok(self.load("WHERE id = ?1", [id])?.pop())
    }

    /// Most recently updated first.
    pub fn convos(&self) -> DbResult<Vec<Convo>> {
        self.load("", [])
    }

    fn load(&self, filter: &str, params: impl rusqlite::Params) -> DbResult<Vec<Convo>> {
        let c = self.conn();
        let mut convos = c
            .prepare(&format!(
                "SELECT id, base, target, level, mode, updated FROM convos {filter} ORDER BY updated DESC, id DESC"
            ))?
            .query_map(params, |r| {
                Ok(Convo {
                    id: r.get(0)?,
                    settings: Settings { base: r.get(1)?, target: r.get(2)?, level: r.get(3)?, mode: r.get(4)? },
                    updated: r.get(5)?,
                    messages: vec![],
                })
            })?
            .collect::<DbResult<Vec<_>>>()?;
        let mut q = c.prepare(
            "SELECT role, content, translation, voice FROM messages WHERE convo = ?1 ORDER BY id",
        )?;
        for convo in &mut convos {
            convo.messages = q
                .query_map([convo.id], |r| {
                    Ok(Message { role: r.get(0)?, content: r.get(1)?, translation: r.get(2)?, voice: r.get(3)? })
                })?
                .collect::<DbResult<_>>()?;
        }
        Ok(convos)
    }

    /// Appends and bumps the convo's `updated`. Errors if the convo doesn't exist.
    pub fn add_message(&self, convo: i64, m: &Message, now: i64) -> DbResult<()> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        tx.execute(
            "INSERT INTO messages (convo, role, content, translation, voice) VALUES (?1, ?2, ?3, ?4, ?5)",
            (convo, &m.role, &m.content, &m.translation, m.voice),
        )?;
        tx.execute("UPDATE convos SET updated = ?2 WHERE id = ?1", (convo, now))?;
        tx.commit()
    }

    /// Saving a word again replaces its gloss and moves it to the top.
    pub fn save_word(&self, lang: &str, w: &Word) -> DbResult<()> {
        self.conn().execute(
            "INSERT OR REPLACE INTO vocab (lang, word, gloss) VALUES (?1, ?2, ?3)",
            (lang, &w.word, &w.gloss),
        )?;
        Ok(())
    }

    /// Newest first.
    pub fn vocab(&self, lang: &str) -> DbResult<Vec<Word>> {
        self.conn()
            .prepare("SELECT word, gloss FROM vocab WHERE lang = ?1 ORDER BY rowid DESC")?
            .query_map([lang], |r| Ok(Word { word: r.get(0)?, gloss: r.get(1)? }))?
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Db {
        Db::open(":memory:").unwrap()
    }

    fn settings(target: &str) -> Settings {
        Settings {
            base: "English".into(),
            target: target.into(),
            level: "Beginner".into(),
            mode: "Text only".into(),
        }
    }

    fn user(text: &str) -> Message {
        Message {
            role: "user".into(),
            content: text.into(),
            translation: None,
            voice: true,
        }
    }

    fn reply(text: &str, tr: &str) -> Message {
        Message {
            role: "assistant".into(),
            content: text.into(),
            translation: Some(tr.into()),
            voice: false,
        }
    }

    #[test]
    fn created_convo_is_empty_and_retrievable() {
        let db = db();
        let c = db.create_convo(&settings("Spanish"), 100).unwrap();
        assert_eq!(c.settings, settings("Spanish"));
        assert_eq!(c.updated, 100);
        assert!(c.messages.is_empty());
        assert_eq!(db.convo(c.id).unwrap(), Some(c));
    }

    #[test]
    fn missing_convo_is_none() {
        assert_eq!(db().convo(42).unwrap(), None);
    }

    #[test]
    fn messages_round_trip_in_order_with_voice_and_translation() {
        let db = db();
        let id = db.create_convo(&settings("Spanish"), 1).unwrap().id;
        db.add_message(id, &user("hola"), 2).unwrap();
        db.add_message(id, &reply("¡Hola!", "Hi!"), 3).unwrap();
        let c = db.convo(id).unwrap().unwrap();
        assert_eq!(c.messages, vec![user("hola"), reply("¡Hola!", "Hi!")]);
        assert_eq!(c.updated, 3);
    }

    #[test]
    fn adding_message_to_unknown_convo_fails() {
        assert!(db().add_message(9, &user("x"), 1).is_err());
    }

    #[test]
    fn convos_list_most_recently_updated_first() {
        let db = db();
        let a = db.create_convo(&settings("Spanish"), 1).unwrap().id;
        let b = db.create_convo(&settings("French"), 2).unwrap().id;
        let ids = |db: &Db| db.convos().unwrap().iter().map(|c| c.id).collect::<Vec<_>>();
        assert_eq!(ids(&db), vec![b, a]);
        db.add_message(a, &user("hola"), 3).unwrap();
        assert_eq!(ids(&db), vec![a, b]);
        assert_eq!(db.convos().unwrap()[0].messages, vec![user("hola")]);
    }

    #[test]
    fn vocab_is_per_language_newest_first_and_dedupes_by_word() {
        let db = db();
        let w = |word: &str, gloss: &str| Word { word: word.into(), gloss: gloss.into() };
        db.save_word("Spanish", &w("perro", "dog")).unwrap();
        db.save_word("Spanish", &w("gato", "cat")).unwrap();
        db.save_word("French", &w("chat", "cat")).unwrap();
        db.save_word("Spanish", &w("perro", "dog (m.)")).unwrap();
        assert_eq!(
            db.vocab("Spanish").unwrap(),
            vec![w("perro", "dog (m.)"), w("gato", "cat")]
        );
        assert_eq!(db.vocab("French").unwrap(), vec![w("chat", "cat")]);
        assert!(db.vocab("German").unwrap().is_empty());
    }

    #[test]
    fn data_survives_reopening_the_file() {
        let path = std::env::temp_dir().join(format!("lang-ai-test-{}.db", std::process::id()));
        let path = path.to_str().unwrap();
        let _ = std::fs::remove_file(path);
        let id = {
            let db = Db::open(path).unwrap();
            let id = db.create_convo(&settings("Spanish"), 1).unwrap().id;
            db.add_message(id, &user("hola"), 2).unwrap();
            id
        };
        let db = Db::open(path).unwrap();
        assert_eq!(db.convo(id).unwrap().unwrap().messages, vec![user("hola")]);
        std::fs::remove_file(path).unwrap();
    }
}
