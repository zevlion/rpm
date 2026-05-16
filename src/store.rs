use crate::ipc::{ProcessInfo, ProcessStatus};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, Result, params};

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open() -> Result<Self> {
        let path = Self::db_path();
        std::fs::create_dir_all(path.parent().unwrap()).ok();
        let conn = Connection::open(&path)?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn db_path() -> std::path::PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        std::path::PathBuf::from(home).join(".rpm2").join("rpm2.db")
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS processes (
                id                INTEGER PRIMARY KEY,
                name              TEXT NOT NULL,
                program           TEXT NOT NULL,
                args              TEXT NOT NULL,
                cwd               TEXT,
                interpreter       TEXT,
                interpreter_args  TEXT,
                status            TEXT NOT NULL DEFAULT 'stopped',
                pid               INTEGER,
                restarts          INTEGER NOT NULL DEFAULT 0,
                max_restarts      INTEGER,
                no_autorestart    INTEGER NOT NULL DEFAULT 0,
                restart_delay     INTEGER,
                kill_timeout      INTEGER,
                started_at        TEXT
            );
        ",
        )
    }

    pub fn upsert(&self, p: &ProcessInfo) -> Result<()> {
        self.conn.execute(
            "INSERT INTO processes (
                id, name, program, args, cwd, interpreter, interpreter_args,
                status, pid, restarts, max_restarts, no_autorestart,
                restart_delay, kill_timeout, started_at
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
            ON CONFLICT(id) DO UPDATE SET
                name=excluded.name, program=excluded.program, args=excluded.args,
                cwd=excluded.cwd, interpreter=excluded.interpreter,
                interpreter_args=excluded.interpreter_args, status=excluded.status,
                pid=excluded.pid, restarts=excluded.restarts,
                max_restarts=excluded.max_restarts, no_autorestart=excluded.no_autorestart,
                restart_delay=excluded.restart_delay, kill_timeout=excluded.kill_timeout,
                started_at=excluded.started_at",
            params![
                p.id as i64,
                p.name,
                p.program,
                serde_json::to_string(&p.args).unwrap(),
                p.cwd,
                p.interpreter,
                p.interpreter_args,
                p.status.to_string(),
                p.pid.map(|x| x as i64),
                p.restarts as i64,
                p.max_restarts.map(|x| x as i64),
                p.no_autorestart as i64,
                p.restart_delay.map(|x| x as i64),
                p.kill_timeout.map(|x| x as i64),
                p.started_at.map(|t| t.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: usize) -> Result<()> {
        self.conn
            .execute("DELETE FROM processes WHERE id=?1", params![id as i64])?;
        Ok(())
    }

    pub fn load_all(&self) -> Result<Vec<ProcessInfo>> {
        let mut stmt = self.conn.prepare("SELECT * FROM processes")?;
        let rows = stmt.query_map([], |row| {
            let args_str: String = row.get(3)?;
            let status_str: String = row.get(7)?;
            let started_at_str: Option<String> = row.get(14)?;

            Ok(ProcessInfo {
                id: row.get::<_, i64>(0)? as usize,
                name: row.get(1)?,
                program: row.get(2)?,
                args: serde_json::from_str(&args_str).unwrap_or_default(),
                cwd: row.get(4)?,
                interpreter: row.get(5)?,
                interpreter_args: row.get(6)?,
                status: match status_str.as_str() {
                    "running" => ProcessStatus::Running,
                    "errored" => ProcessStatus::Errored,
                    "restarting" => ProcessStatus::Restarting,
                    _ => ProcessStatus::Stopped,
                },
                pid: row.get::<_, Option<i64>>(8)?.map(|x| x as u32),
                restarts: row.get::<_, i64>(9)? as u32,
                max_restarts: row.get::<_, Option<i64>>(10)?.map(|x| x as u32),
                no_autorestart: row.get::<_, i64>(11)? != 0,
                restart_delay: row.get::<_, Option<i64>>(12)?.map(|x| x as u64),
                kill_timeout: row.get::<_, Option<i64>>(13)?.map(|x| x as u64),
                started_at: started_at_str
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|t| t.with_timezone(&Utc)),
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}
