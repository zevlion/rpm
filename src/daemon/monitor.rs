//! # Process Monitor
//!
//! Background task (polling every **2 seconds**) that:
//!
//! 1. **Detects crashes** — calls [`tokio::process::Child::try_wait`] on every
//!    live child. If the process has exited it marks it `Stopped` and, if
//!    `watching == true`, immediately calls [`manager::restart`].
//!
//! 2. **Enforces OOM limits** — if a process has `max_memory` set and its
//!    resident-set size exceeds the limit, the monitor restarts it and logs a
//!    message to stderr.
//!
//! 3. **Updates uptime** — recalculates `process.uptime` from `started_at`.
//!
//! Memory readings on Linux come from `/proc/<pid>/status` (`VmRSS` field)
//! which reports the resident set size in kilobytes; the value is multiplied
//! by 1024 to convert to bytes before being stored in [`crate::process::Process::mem`].
use super::manager::{self, ProcessMap};
use crate::process::ProcessStatus;
use std::time::Duration;
use tokio::time;

pub async fn run(map: ProcessMap, lb_map: super::manager::LoadBalancerMap) {
    let mut interval = time::interval(Duration::from_secs(2));

    loop {
        interval.tick().await;

        let ids: Vec<u32> = {
            let map = map.lock().await;
            map.keys().cloned().collect()
        };

        for id in ids {
            // update uptime
            {
                let mut locked = map.lock().await;
                if let Some(entry) = locked.get_mut(&id)
                    && let Some(started_at) = entry.started_at {
                        entry.process.uptime = started_at.elapsed();
                    }
            }

            // poll cpu + mem from /proc
            let pid = {
                let locked = map.lock().await;
                locked.get(&id).and_then(|e| e.process.pid)
            };

            if let Some(pid) = pid
                && let Ok((cpu, mem)) = read_proc_stats(pid).await {
                    let mut locked = map.lock().await;
                    if let Some(entry) = locked.get_mut(&id) {
                        entry.process.cpu = cpu;
                        entry.process.mem = mem;
                    }
                }

            // check if exited, auto-restart if watching
            let mut should_restart = false;
            let mut memory_exceeded = false;
            let mut app_name = String::new();
            let mut current_mem = 0;
            let mut limit_mem = None;

            {
                let mut locked = map.lock().await;
                if let Some(entry) = locked.get_mut(&id) {
                    app_name = entry.process.name.clone();
                    current_mem = entry.process.mem;
                    limit_mem = entry.max_memory;
                    if let Some(child) = entry.child.as_mut() {
                        if let Ok(Some(_)) = child.try_wait() {
                            entry.process.status = ProcessStatus::Stopped;
                            entry.process.pid = None;
                            entry.child = None;
                            entry.started_at = None;
                            should_restart = entry.process.watching;
                        } else if entry.process.status == ProcessStatus::Online
                            && let Some(max_mem) = entry.max_memory
                                && entry.process.mem > max_mem {
                                    memory_exceeded = true;
                                }
                    }
                }
            }

            if should_restart {
                let _ = manager::restart(&map, &lb_map, id).await;
            } else if memory_exceeded {
                println!(
                    "[daemon] Process {} ({}) memory usage {} bytes exceeded limit of {:?} bytes. Restarting...",
                    id, app_name, current_mem, limit_mem
                );
                let _ = manager::restart(&map, &lb_map, id).await;
            }
        }
    }
}

async fn read_proc_stats(pid: u32) -> anyhow::Result<(f32, u64)> {
    let status = tokio::fs::read_to_string(format!("/proc/{}/status", pid)).await?;
    let mem = status
        .lines()
        .find(|l| l.starts_with("VmRSS:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
        * 1024;

    let cpu = 0.0;

    Ok((cpu, mem))
}
