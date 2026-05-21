use crate::process::{Process, ProcessStatus};
use anyhow::Result;
use rusqlite::{Connection, params};
use std::time::Duration;

pub fn init_db() -> Result<Connection> {
    let mut db_path = std::env::current_exe()?.parent().unwrap().to_path_buf();
    db_path.push("rpm2.db");

    let conn = Connection::open(db_path)?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS processes (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            cmd TEXT NOT NULL,
            args TEXT NOT NULL,
            watching INTEGER NOT NULL,
            interpreter TEXT
        )",
        [],
    )?;

    // Migrations
    let _ = conn.execute("ALTER TABLE processes ADD COLUMN mode TEXT DEFAULT 'fork'", []);
    let _ = conn.execute("ALTER TABLE processes ADD COLUMN instances INTEGER DEFAULT 1", []);
    let _ = conn.execute("ALTER TABLE processes ADD COLUMN port INTEGER", []);
    let _ = conn.execute("ALTER TABLE processes ADD COLUMN lb_strategy TEXT", []);
    let _ = conn.execute("ALTER TABLE processes ADD COLUMN max_memory INTEGER", []);
    let _ = conn.execute("ALTER TABLE processes ADD COLUMN max_cpu REAL", []);

    Ok(conn)
}

pub fn save_process(
    conn: &Connection,
    process: &Process,
) -> Result<()> {
    let args_json = serde_json::to_string(&process.args)?;
    conn.execute(
        "INSERT OR REPLACE INTO processes (id, name, cmd, args, watching, interpreter, mode, instances, port, lb_strategy, max_memory, max_cpu)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            process.id,
            process.name,
            process.cmd,
            args_json,
            if process.watching { 1 } else { 0 },
            process.interpreter,
            process.mode,
            process.instances,
            process.port,
            process.lb_strategy,
            process.max_memory,
            process.max_cpu,
        ],
    )?;
    Ok(())
}

pub fn load_processes(conn: &Connection) -> Result<Vec<Process>> {
    let mut stmt =
        conn.prepare("SELECT id, name, cmd, args, watching, interpreter, mode, instances, port, lb_strategy, max_memory, max_cpu FROM processes")?;
    let rows = stmt.query_map([], |row| {
        let args_str: String = row.get(3)?;
        let args: Vec<String> = serde_json::from_str(&args_str).unwrap_or_default();
        let watching_int: i32 = row.get(4)?;

        Ok(Process {
            id: row.get(0)?,
            name: row.get(1)?,
            cmd: row.get(2)?,
            args,
            interpreter: row.get(5)?,
            pid: None,
            uptime: Duration::ZERO,
            status: ProcessStatus::Stopped,
            cpu: 0.0,
            mem: 0,
            watching: watching_int == 1,
            restarts: 0,
            mode: row.get::<_, Option<String>>(6)?.unwrap_or_else(|| "fork".to_string()),
            instances: row.get::<_, Option<u32>>(7)?.unwrap_or(1),
            port: row.get::<_, Option<u16>>(8)?,
            lb_strategy: row.get::<_, Option<String>>(9)?,
            max_memory: row.get::<_, Option<u64>>(10)?,
            max_cpu: row.get::<_, Option<f32>>(11)?,
        })
    })?;

    let mut list = Vec::new();
    for row in rows {
        list.push(row?);
    }
    Ok(list)
}

pub fn remove_process(conn: &Connection, id: u32) -> Result<()> {
    conn.execute("DELETE FROM processes WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn clear_all(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM processes", [])?;
    Ok(())
}
