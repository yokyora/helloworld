use std::{path::Path, time::Duration};
use rusqlite::{Connection, OptionalExtension, params};
use crate::domain::{AppError, Settings, Turn, DEFAULT_MODEL};

pub struct Store { db: Connection }

impl Store {
    #[cfg(test)]
    pub fn memory() -> Self { Self::initialize(Connection::open_in_memory().unwrap()).unwrap() }
    pub fn open(path: &Path) -> Result<Self, AppError> { Self::initialize(Connection::open(path)?) }
    fn initialize(mut db: Connection) -> Result<Self, AppError> {
        db.busy_timeout(Duration::from_secs(3))?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version > 1 { return Err(AppError::new("schema_version", "数据由更新版本创建，请使用更新的应用打开。")); }
        let tx = db.transaction()?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS settings (
            id INTEGER PRIMARY KEY CHECK(id=1), model TEXT NOT NULL,
            encrypted_key BLOB, allow_remote INTEGER NOT NULL DEFAULT 0,
            quiet INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS turns (
            id TEXT PRIMARY KEY, user_text TEXT NOT NULL, assistant_text TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL CHECK(status IN ('pending','complete','cancelled','error')),
            created_at INTEGER NOT NULL DEFAULT (unixepoch()));
            CREATE INDEX IF NOT EXISTS turns_order ON turns(created_at);
            PRAGMA user_version=1;")?;
        tx.execute("INSERT OR IGNORE INTO settings(id,model) VALUES (1,?1)", [DEFAULT_MODEL])?;
        tx.execute("UPDATE turns SET status='error' WHERE status='pending'", [])?;
        tx.commit()?;
        Ok(Self { db })
    }
    pub fn settings(&self) -> Result<Settings, AppError> {
        Ok(self.db.query_row("SELECT model,encrypted_key IS NOT NULL,allow_remote FROM settings WHERE id=1", [], |r| Ok(Settings {model:r.get(0)?,has_api_key:r.get(1)?,allow_remote:r.get(2)?}))?)
    }
    pub fn quiet(&self) -> Result<bool, AppError> { Ok(self.db.query_row("SELECT quiet FROM settings WHERE id=1", [], |r| r.get(0))?) }
    pub fn set_quiet(&self, value: bool) -> Result<(), AppError> { self.db.execute("UPDATE settings SET quiet=?1 WHERE id=1", [value])?; Ok(()) }
    pub fn encrypted_key(&self) -> Result<Option<Vec<u8>>, AppError> { Ok(self.db.query_row("SELECT encrypted_key FROM settings WHERE id=1", [], |r| r.get(0))?) }
    pub fn save_settings(&self, model: &str, key: Option<&[u8]>, allow: bool) -> Result<(), AppError> {
        self.db.execute("UPDATE settings SET model=?1,encrypted_key=?2,allow_remote=?3 WHERE id=1", params![model,key,allow])?; Ok(())
    }
    pub fn turns(&self) -> Result<Vec<Turn>, AppError> {
        let mut query = self.db.prepare("SELECT id,user_text,assistant_text,status,created_at FROM (SELECT rowid,id,user_text,assistant_text,status,created_at FROM turns ORDER BY rowid DESC LIMIT 100) ORDER BY rowid")?;
        let rows = query.query_map([], |r| Ok(Turn {id:r.get(0)?,user_text:r.get(1)?,assistant_text:r.get(2)?,status:r.get(3)?,created_at:r.get(4)?}))?;
        Ok(rows.collect::<Result<Vec<_>,_>>()?)
    }
    pub fn start(&self, id: &str, text: &str) -> Result<(), AppError> {
        let exists: Option<String> = self.db.query_row("SELECT id FROM turns WHERE id=?1", [id], |r| r.get(0)).optional()?;
        if exists.is_some() { return Err(AppError::new("duplicate", "这条请求已处理，请勿重复发送。")); }
        self.db.execute("INSERT INTO turns(id,user_text,status) VALUES (?1,?2,'pending')", params![id,text])?; Ok(())
    }
    pub fn finish(&self, id: &str, text: &str, status: &str) -> Result<(), AppError> {
        self.db.execute("UPDATE turns SET assistant_text=?2,status=?3 WHERE id=?1 AND status='pending'", params![id,text,status])?; Ok(())
    }
    pub fn clear(&self) -> Result<(), AppError> { self.db.execute("DELETE FROM turns", [])?; Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_private_settings_and_completed_turns() {
        let s = Store::initialize(Connection::open_in_memory().unwrap()).unwrap();
        assert!(!s.settings().unwrap().allow_remote);
        assert!(!s.settings().unwrap().has_api_key);
        s.start("1", "你好").unwrap();
        assert!(s.start("1", "重复").is_err());
        s.finish("1", "回答", "complete").unwrap();
        s.finish("1", "迟到的覆盖", "error").unwrap();
        assert_eq!(s.turns().unwrap()[0].assistant_text, "回答");
        s.set_quiet(true).unwrap(); assert!(s.quiet().unwrap());
        s.clear().unwrap(); assert!(s.turns().unwrap().is_empty());
    }
    #[test]
    fn migration_recovers_pending_turns_without_erasing_content() {
        let db = Connection::open_in_memory().unwrap();
        let s = Store::initialize(db).unwrap();
        s.start("crashed", "未完成的问题").unwrap();
        s.start("ok", "完成的问题").unwrap(); s.finish("ok", "回答", "complete").unwrap();
        let s = Store::initialize(s.db).unwrap();
        let turns = s.turns().unwrap();
        assert_eq!(turns[0].status, "error"); assert_eq!(turns[0].user_text, "未完成的问题");
        assert_eq!(turns[1].status, "complete");
    }
    #[test]
    fn future_schema_is_rejected() {
        let db = Connection::open_in_memory().unwrap(); db.pragma_update(None,"user_version",2).unwrap();
        assert!(Store::initialize(db).is_err());
    }
    #[test]
    fn quiet_and_history_survive_reopen() {
        let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("test.sqlite3");
        { let s = Store::open(&path).unwrap(); s.set_quiet(true).unwrap(); s.start("1","持久问题").unwrap(); s.finish("1","持久回答","complete").unwrap(); }
        let s = Store::open(&path).unwrap(); assert!(s.quiet().unwrap()); assert_eq!(s.turns().unwrap()[0].assistant_text,"持久回答");
    }
}
