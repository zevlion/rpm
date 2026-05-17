use super::manager::{self, ProcessMap};
use crate::process::ProcessStatus;
use std::time::Duration;
use tokio::time;

pub async fn run(map: ProcessMap) {
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
            let should_restart = {
                let mut locked = map.lock().await;
                if let Some(entry) = locked.get_mut(&id) {
                    if let Some(child) = entry.child.as_mut() {
                        if let Ok(Some(_)) = child.try_wait() {
                            entry.process.status = ProcessStatus::Stopped;
                            entry.process.pid = None;
                            entry.child = None;
                            entry.started_at = None;
                            entry.process.watching // restart only if watching
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            }; // MutexGuard dropped here

            if should_restart {
                let _ = manager::restart(&map, id).await;
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
