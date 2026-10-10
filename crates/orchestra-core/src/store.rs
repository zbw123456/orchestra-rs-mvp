use crate::domain::{Status, Task};
use anyhow::Result;
use rusqlite::{Connection, params};
use serde::Serialize;

pub struct Store {
    db: Connection,
}

#[derive(Debug, Serialize)]
pub struct Event {
    pub seq: i64,
    pub task_id: String,
    pub status: Status,
    pub at_ms: u64,
    pub detail: String,
}

impl Store {
    pub fn open(path: &str) -> Result<Self> {
        let db = Connection::open(path)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS tasks (id TEXT PRIMARY KEY, body TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS events (seq INTEGER PRIMARY KEY AUTOINCREMENT, body TEXT NOT NULL);")?;
        Ok(Self { db })
    }
    pub fn load(&self) -> Result<Vec<Task>> {
        let mut query = self.db.prepare("SELECT body FROM tasks ORDER BY rowid")?;
        let rows = query.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    /// Snapshot and transition journal are committed together. Not a message outbox.
    pub fn save(&mut self, task: &Task) -> Result<()> {
        let tx = self.db.transaction()?;
        tx.execute("INSERT INTO tasks(id,body) VALUES (?1,?2) ON CONFLICT(id) DO UPDATE SET body=excluded.body",
            params![task.id, serde_json::to_string(task)?])?;
        let event = serde_json::json!({"task_id": task.id, "status": task.status,
            "at_ms": crate::domain::now_ms(), "detail": task.detail});
        tx.execute("INSERT INTO events(body) VALUES (?1)", [event.to_string()])?;
        tx.commit()?;
        Ok(())
    }
    pub fn events(&self, after: i64) -> Result<Vec<Event>> {
        let mut query = self
            .db
            .prepare("SELECT seq,body FROM events WHERE seq > ?1 ORDER BY seq LIMIT 100")?;
        let rows = query.query_map([after], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        rows.map(|r| {
            let (seq, body) = r?;
            let v: serde_json::Value = serde_json::from_str(&body)?;
            Ok(Event {
                seq,
                task_id: v["task_id"].as_str().unwrap_or_default().into(),
                status: serde_json::from_value(v["status"].clone())?,
                at_ms: v["at_ms"].as_u64().unwrap_or_default(),
                detail: v["detail"].as_str().unwrap_or_default().into(),
            })
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_and_event_survive_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let mut store = Store::open(path.to_str().unwrap()).unwrap();
        let task = Task::new(serde_json::from_str(r#"{"title":"persist"}"#).unwrap());
        store.save(&task).unwrap();
        drop(store);
        let store = Store::open(path.to_str().unwrap()).unwrap();
        assert_eq!(store.load().unwrap()[0].id, task.id);
        assert_eq!(store.events(0).unwrap().len(), 1);
        assert!(store.events(1).unwrap().is_empty());
    }
}
