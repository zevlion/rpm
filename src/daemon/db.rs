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
    Ok(conn)
}

pub fn save_process(
    conn: &Connection,
    id: u32,
    name: &str,
    cmd: &str,
    args: &[String],
    watching: bool,
    interpreter: Option<&str>,
) -> Result<()> {
    let args_json = serde_json::to_string(args)?;
    conn.execute(
        "INSERT OR REPLACE INTO processes (id, name, cmd, args, watching, interpreter)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id,
            name,
            cmd,
            args_json,
            if watching { 1 } else { 0 },
            interpreter
        ],
    )?;
    Ok(())
}

pub fn load_processes(conn: &Connection) -> Result<Vec<Process>> {
    let mut stmt =
        conn.prepare("SELECT id, name, cmd, args, watching, interpreter FROM processes")?;
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
