use std::{path::Path, time::Duration};
use rusqlite::{Connection, OptionalExtension, params};
use crate::domain::{AppError, BubbleAnchor, DisplayScale, Settings, Turn, DEFAULT_MODEL};

pub struct Store { db: Connection }

impl Store {
    #[cfg(test)]
    pub fn memory() -> Self { Self::initialize(Connection::open_in_memory().unwrap()).unwrap() }
    pub fn open(path: &Path) -> Result<Self, AppError> { Self::initialize(Connection::open(path)?) }
    fn initialize(mut db: Connection) -> Result<Self, AppError> {
        db.busy_timeout(Duration::from_secs(3))?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version > 5 { return Err(AppError::new("schema_version", "数据由更新版本创建，请使用更新的应用打开。")); }
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
            CREATE TABLE IF NOT EXISTS display_preferences (
                id INTEGER PRIMARY KEY CHECK(id=1), scale_percent INTEGER NOT NULL
                CHECK(scale_percent IN (75,100,125,150,175,200)));
            INSERT OR IGNORE INTO display_preferences(id,scale_percent) VALUES (1,100);")?;
        if version < 3 {
            tx.execute_batch("ALTER TABLE display_preferences ADD COLUMN panel_percent INTEGER NOT NULL DEFAULT 100
                CHECK(panel_percent IN (75,100,125,150,175,200));
                UPDATE display_preferences SET panel_percent=scale_percent;")?;
        }
        tx.execute_batch("CREATE TABLE IF NOT EXISTS model_bubble_anchors (
            model_id TEXT PRIMARY KEY, x REAL NOT NULL CHECK(x BETWEEN 0 AND 1), y REAL NOT NULL CHECK(y BETWEEN 0 AND 1));")?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS imported_live2d (id TEXT PRIMARY KEY, name TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS live2d_selection (id INTEGER PRIMARY KEY CHECK(id=1), model_id TEXT NOT NULL);
            INSERT OR IGNORE INTO live2d_selection(id,model_id) VALUES(1,'builtin-atri');")?;
        tx.pragma_update(None, "user_version", 5)?;
        tx.execute("INSERT OR IGNORE INTO settings(id,model) VALUES (1,?1)", [DEFAULT_MODEL])?;
        tx.execute("UPDATE turns SET status='error' WHERE status='pending'", [])?;
        tx.commit()?;
        Ok(Self { db })
    }
    pub fn settings(&self) -> Result<Settings, AppError> {
        Ok(self.db.query_row("SELECT model,encrypted_key IS NOT NULL,allow_remote FROM settings WHERE id=1", [], |r| Ok(Settings {model:r.get(0)?,has_api_key:r.get(1)?,allow_remote:r.get(2)?}))?)
    }
    pub fn quiet(&self) -> Result<bool, AppError> { Ok(self.db.query_row("SELECT quiet FROM settings WHERE id=1", [], |r| r.get(0))?) }
    pub fn live2d_models(&self) -> Result<Vec<crate::models::ModelEntry>, AppError> {
        let mut query = self.db.prepare("SELECT id,name FROM imported_live2d ORDER BY rowid")?;
        Ok(query.query_map([], |r| Ok(crate::models::ModelEntry {id:r.get(0)?,name:r.get(1)?}))?.collect::<Result<Vec<_>,_>>()?)
    }
    pub fn add_live2d(&self, model: &crate::models::ModelEntry) -> Result<(),AppError> {
        self.db.execute("INSERT OR IGNORE INTO imported_live2d(id,name) VALUES(?1,?2)",params![model.id,model.name])?; Ok(())
    }
    pub fn selected_live2d(&self) -> Result<String,AppError> {
        Ok(self.db.query_row("SELECT model_id FROM live2d_selection WHERE id=1",[],|r| r.get(0))?)
    }
    pub fn select_live2d(&self, id: &str) -> Result<(),AppError> {
        self.db.execute("UPDATE live2d_selection SET model_id=?1 WHERE id=1",[id])?; Ok(())
    }
    pub fn bubble_anchor(&self, model_id: &str) -> Result<Option<BubbleAnchor>, AppError> {
        Ok(self.db.query_row("SELECT x,y FROM model_bubble_anchors WHERE model_id=?1", [model_id],
            |r| Ok(BubbleAnchor { x: r.get(0)?, y: r.get(1)? })).optional()?)
    }
    pub fn save_bubble_anchor(&self, model_id: &str, anchor: BubbleAnchor) -> Result<(), AppError> {
        let anchor = anchor.validate()?;
        self.db.execute("INSERT INTO model_bubble_anchors(model_id,x,y) VALUES(?1,?2,?3)
            ON CONFLICT(model_id) DO UPDATE SET x=excluded.x,y=excluded.y", params![model_id,anchor.x,anchor.y])?;
        Ok(())
    }
    pub fn display_scale(&self) -> Result<DisplayScale, AppError> {
        Ok(self.db.query_row("SELECT scale_percent,panel_percent FROM display_preferences WHERE id=1", [],
            |r| Ok(DisplayScale { pet_percent: r.get(0)?, panel_percent: r.get(1)? }))?)
    }
    pub fn save_display_scale(&self, value: DisplayScale) -> Result<(), AppError> {
        self.db.execute("UPDATE display_preferences SET scale_percent=?1,panel_percent=?2 WHERE id=1", params![value.pet_percent, value.panel_percent])?;
        Ok(())
    }
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
        let db = Connection::open_in_memory().unwrap(); db.pragma_update(None,"user_version",6).unwrap();
        assert!(Store::initialize(db).is_err());
    }
    #[test]
    fn quiet_and_history_survive_reopen() {
        let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("test.sqlite3");
        { let s = Store::open(&path).unwrap(); s.set_quiet(true).unwrap(); s.start("1","持久问题").unwrap(); s.finish("1","持久回答","complete").unwrap(); }
        let s = Store::open(&path).unwrap(); assert!(s.quiet().unwrap()); assert_eq!(s.turns().unwrap()[0].assistant_text,"持久回答");
    }
    #[test]
    fn version_one_migration_preserves_data_and_scale_survives_reopen() {
        let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("display.sqlite3");
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE settings(id INTEGER PRIMARY KEY, model TEXT NOT NULL, encrypted_key BLOB, allow_remote INTEGER NOT NULL, quiet INTEGER NOT NULL);
            INSERT INTO settings VALUES(1,'deepseek-flash',X'010203',1,1);
            CREATE TABLE turns(id TEXT PRIMARY KEY,user_text TEXT NOT NULL,assistant_text TEXT NOT NULL,status TEXT NOT NULL,created_at INTEGER NOT NULL);
            INSERT INTO turns VALUES('kept','原消息','原回复','complete',123);
            PRAGMA user_version=1;").unwrap();
        drop(db);
        {
            let s = Store::open(&path).unwrap();
            assert_eq!(s.display_scale().unwrap(), DisplayScale::default());
            assert_eq!(s.encrypted_key().unwrap(), Some(vec![1,2,3]));
            assert!(s.settings().unwrap().allow_remote);
            assert!(s.quiet().unwrap());
            assert_eq!(s.turns().unwrap()[0].assistant_text, "原回复");
            s.save_display_scale(DisplayScale { pet_percent: 150, panel_percent: 75 }).unwrap();
            assert!(s.save_display_scale(DisplayScale { pet_percent: 999, panel_percent: 75 }).is_err());
        }
        let s = Store::open(&path).unwrap();
        assert_eq!(s.display_scale().unwrap(), DisplayScale { pet_percent: 150, panel_percent: 75 });
        assert_eq!(s.turns().unwrap().len(), 1);
    }
    #[test]
    fn version_two_scale_is_preserved_for_both_windows() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE display_preferences(id INTEGER PRIMARY KEY, scale_percent INTEGER NOT NULL);
            INSERT INTO display_preferences VALUES(1,150); PRAGMA user_version=2;").unwrap();
        let s = Store::initialize(db).unwrap();
        assert_eq!(s.display_scale().unwrap(), DisplayScale { pet_percent: 150, panel_percent: 150 });
        s.save_display_scale(DisplayScale { pet_percent: 200, panel_percent: 150 }).unwrap();
        assert_eq!(s.display_scale().unwrap().panel_percent, 150);
        s.save_display_scale(DisplayScale { pet_percent: 200, panel_percent: 75 }).unwrap();
        assert_eq!(s.display_scale().unwrap().pet_percent, 200);
        let s = Store::initialize(s.db).unwrap();
        assert_eq!(s.display_scale().unwrap(), DisplayScale { pet_percent: 200, panel_percent: 75 });
    }
    #[test]
    fn model_anchors_are_isolated_and_survive_reopen() {
        let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("anchors.sqlite3");
        let s = Store::open(&path).unwrap();
        s.save_bubble_anchor("model-a",BubbleAnchor {x:0.4,y:0.2}).unwrap();
        s.save_bubble_anchor("model-b",BubbleAnchor {x:0.6,y:0.3}).unwrap();
        assert!(s.save_bubble_anchor("model-a",BubbleAnchor {x:f64::NAN,y:0.2}).is_err());
        drop(s);
        let s = Store::open(&path).unwrap();
        assert_eq!(s.bubble_anchor("model-a").unwrap(),Some(BubbleAnchor{x:0.4,y:0.2}));
        assert_eq!(s.bubble_anchor("model-b").unwrap(),Some(BubbleAnchor{x:0.6,y:0.3}));
        assert_eq!(s.bubble_anchor("replacement").unwrap(),None);
    }
    #[test]
    fn version_four_migration_and_model_selection_survive_reopen() {
        let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("models.sqlite3");
        let s = Store::open(&path).unwrap();
        s.save_bubble_anchor("atri",BubbleAnchor{x:0.45,y:0.2}).unwrap();
        s.db.execute_batch("DROP TABLE imported_live2d; DROP TABLE live2d_selection; PRAGMA user_version=4;").unwrap();
        drop(s);
        let s = Store::open(&path).unwrap();
        assert_eq!(s.selected_live2d().unwrap(), crate::models::BUILTIN);
        let entry = crate::models::ModelEntry{id:"second".into(),name:"第二个角色".into()};
        s.add_live2d(&entry).unwrap(); s.add_live2d(&entry).unwrap(); s.select_live2d(&entry.id).unwrap();
        s.save_bubble_anchor("second-moc",BubbleAnchor{x:0.6,y:0.3}).unwrap(); drop(s);
        let s = Store::open(&path).unwrap();
        assert_eq!(s.live2d_models().unwrap().len(),1); assert_eq!(s.selected_live2d().unwrap(),"second");
        assert_eq!(s.bubble_anchor("atri").unwrap(),Some(BubbleAnchor{x:0.45,y:0.2}));
        assert_eq!(s.bubble_anchor("second-moc").unwrap(),Some(BubbleAnchor{x:0.6,y:0.3}));
    }
    #[test]
    fn version_three_migration_keeps_existing_preferences_and_chat() {
        let s = Store::memory();
        s.save_display_scale(DisplayScale{pet_percent:200,panel_percent:75}).unwrap();
        s.start("kept","原消息").unwrap(); s.finish("kept","原回复","complete").unwrap();
        s.db.execute_batch("DROP TABLE model_bubble_anchors; PRAGMA user_version=3;").unwrap();
        let s = Store::initialize(s.db).unwrap();
        assert_eq!(s.display_scale().unwrap(),DisplayScale{pet_percent:200,panel_percent:75});
        assert_eq!(s.turns().unwrap()[0].assistant_text,"原回复");
        assert_eq!(s.bubble_anchor("new-model").unwrap(),None);
    }
}
