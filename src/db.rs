use crate::catalog::Entry;
use anyhow::Result;
use rusqlite::{params, Connection};
use serde_json::{json, Value};

pub struct Db(pub Connection);
impl Db {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let c = Connection::open(path)?;
        c.busy_timeout(std::time::Duration::from_secs(3))?;
        c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS catalog(id TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY, label TEXT NOT NULL, kind TEXT NOT NULL, state TEXT NOT NULL, pane TEXT, created TEXT NOT NULL, ended TEXT, code INTEGER);
            CREATE TABLE IF NOT EXISTS events(id INTEGER PRIMARY KEY, at TEXT NOT NULL, job TEXT, action TEXT NOT NULL, detail TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS job_acknowledgements(job TEXT PRIMARY KEY REFERENCES jobs(id) ON DELETE CASCADE, at TEXT NOT NULL);
            PRAGMA user_version=2;")?;
        Ok(Self(c))
    }
    pub fn catalog(&mut self, entries: &[Entry]) -> Result<()> {
        let tx = self.0.transaction()?;
        let mut ids = Vec::new();
        for e in entries {
            let id = format!("{}:{}", e.kind, e.reference);
            ids.push(id.clone());
            tx.execute("INSERT INTO catalog VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data WHERE data != excluded.data", params![id,serde_json::to_string(e)?])?;
        }
        let old: Vec<String> = {
            let mut st = tx.prepare("SELECT id FROM catalog")?;
            let v = st
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            v
        };
        for id in old {
            if !ids.contains(&id) {
                tx.execute("DELETE FROM catalog WHERE id=?1", [id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn entries(&self) -> Result<Vec<Entry>> {
        let mut st = self.0.prepare("SELECT data FROM catalog ORDER BY id")?;
        let values = st
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        values
            .into_iter()
            .map(|s| Ok(serde_json::from_str(&s)?))
            .collect()
    }
    pub fn event(&self, job: Option<&str>, action: &str, detail: &str) -> Result<()> {
        self.0.execute(
            "INSERT INTO events(at,job,action,detail) VALUES(?1,?2,?3,?4)",
            params![chrono::Utc::now().to_rfc3339(), job, action, detail],
        )?;
        self.0.execute(
            "DELETE FROM events WHERE id <= (SELECT max(id)-10000 FROM events)",
            [],
        )?;
        Ok(())
    }
    pub fn add_job(&self, id: &str, label: &str, kind: &str) -> Result<()> {
        self.0.execute(
            "INSERT INTO jobs(id,label,kind,state,created) VALUES(?1,?2,?3,'starting',?4)",
            params![id, label, kind, chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }
    pub fn set_job(
        &self,
        id: &str,
        state: &str,
        pane: Option<&str>,
        code: Option<i32>,
    ) -> Result<()> {
        self.0.execute("UPDATE jobs SET state=?2,pane=COALESCE(?3,pane),code=?4,ended=CASE WHEN ?2 IN ('finished','failed','interrupted') THEN ?5 ELSE ended END WHERE id=?1",params![id,state,pane,code,chrono::Utc::now().to_rfc3339()])?;
        Ok(())
    }
    pub fn jobs(&self) -> Result<Vec<Value>> {
        let mut s=self.0.prepare("SELECT j.id,j.label,j.kind,j.state,j.pane,j.created,j.ended,j.code,a.at FROM jobs j LEFT JOIN job_acknowledgements a ON a.job=j.id ORDER BY j.created DESC LIMIT 500")?;
        let v=s.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"label":r.get::<_,String>(1)?,"kind":r.get::<_,String>(2)?,"state":r.get::<_,String>(3)?,"pane":r.get::<_,Option<String>>(4)?,"created":r.get::<_,String>(5)?,"ended":r.get::<_,Option<String>>(6)?,"code":r.get::<_,Option<i32>>(7)?,"acknowledged_at":r.get::<_,Option<String>>(8)?})))?.collect::<rusqlite::Result<_>>()?;
        Ok(v)
    }
    pub fn acknowledge(&mut self, id: &str) -> Result<()> {
        let tx = self.0.transaction()?;
        let changed = tx.execute("INSERT OR IGNORE INTO job_acknowledgements(job,at) SELECT id,?2 FROM jobs WHERE id=?1 AND (state IN ('failed','interrupted') OR (state='finished' AND code != 0))", params![id,chrono::Utc::now().to_rfc3339()])?;
        if changed > 0 {
            tx.execute("INSERT INTO events(at,job,action,detail) VALUES(?1,?2,'job.acknowledged','Fehler zur Kenntnis genommen; Ergebnis unveraendert')",params![chrono::Utc::now().to_rfc3339(),id])?;
        } else {
            anyhow::ensure!(
                tx.query_row(
                    "SELECT count(*) FROM job_acknowledgements WHERE job=?1",
                    [id],
                    |r| r.get::<_, i64>(0)
                )? > 0,
                "Nur abgeschlossene Fehler/Abbrueche koennen quittiert werden"
            );
        }
        tx.execute(
            "DELETE FROM events WHERE id <= (SELECT max(id)-10000 FROM events)",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn clear_events(&mut self) -> Result<()> {
        let tx = self.0.transaction()?;
        tx.execute("DELETE FROM events", [])?;
        tx.execute("INSERT INTO events(at,action,detail) VALUES(?1,'events.cleared','Verlauf bewusst geleert; Auftraege und Ergebnisse erhalten')",[chrono::Utc::now().to_rfc3339()])?;
        tx.commit()?;
        Ok(())
    }
    pub fn events(&self) -> Result<Vec<Value>> {
        let mut s = self
            .0
            .prepare("SELECT at,job,action,detail FROM events ORDER BY id DESC LIMIT 200")?;
        let v=s.query_map([],|r|Ok(json!({"at":r.get::<_,String>(0)?,"job":r.get::<_,Option<String>>(1)?,"action":r.get::<_,String>(2)?,"detail":r.get::<_,String>(3)?})))?.collect::<rusqlite::Result<_>>()?;
        Ok(v)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn acknowledgement_survives_reopen_without_rewriting_failure_or_history_clear() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("db");
        {
            let mut db = Db::open(&path).unwrap();
            db.add_job("bad", "bad", "control").unwrap();
            assert!(db.acknowledge("bad").is_err());
            db.set_job("bad", "failed", Some("%1"), Some(7)).unwrap();
            db.acknowledge("bad").unwrap();
            db.acknowledge("bad").unwrap();
            assert_eq!(db.events().unwrap().len(), 1);
            db.clear_events().unwrap();
        }
        let db = Db::open(&path).unwrap();
        let jobs = db.jobs().unwrap();
        assert_eq!(jobs[0]["state"], "failed");
        assert_eq!(jobs[0]["code"], 7);
        assert!(jobs[0]["acknowledged_at"].is_string());
        assert_eq!(db.events().unwrap()[0]["action"], "events.cleared");
    }
    #[test]
    fn history_and_no_duplicate_job() {
        let mut db = Db::open(std::path::Path::new(":memory:")).unwrap();
        db.catalog(&[]).unwrap();
        db.add_job("one", "base", "control").unwrap();
        assert!(db.add_job("one", "base", "control").is_err());
        db.set_job("one", "finished", Some("%1"), Some(7)).unwrap();
        assert_eq!(db.jobs().unwrap()[0]["code"], 7);
    }
}
