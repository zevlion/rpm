//! # Metrics
//!
//! Background task that refreshes live CPU and memory statistics for every
//! running process every **2 seconds** using the [`sysinfo`] crate.
//!
//! ## Why `sysinfo` and not `/proc` directly?
//!
//! [`sysinfo`] provides a portable API that works on Linux, macOS, and Windows.
//! The monitor module reads `/proc/<pid>/status` for Linux-only memory reads,
//! but metrics uses `sysinfo` so that CPU% is calculated correctly (it requires
//! two samples separated by a small delay — the library handles this internally).
//!
//! ## Seeding
//!
//! A newly spawned PID must be "seeded" (given an initial refresh) before
//! `cpu_usage()` returns a meaningful value. The task tracks which PIDs have
//! been seeded and performs a dedicated first-refresh with a 500 ms pause
//! before the next normal tick.
use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::time;

use crate::{daemon::manager::ProcessMap, process::ProcessStatus};

pub fn start(map: ProcessMap) {
    tokio::spawn(async move {
        let mut sys = System::new_all(); // seed all processes on startup
        let mut interval = time::interval(Duration::from_secs(2));

        // track which pids we've already seeded
        let mut seeded: std::collections::HashSet<u32> = std::collections::HashSet::new();

        loop {
            interval.tick().await;

            let pids: Vec<(u32, u32)> = {
                let locked = map.lock().await;
                locked
                    .values()
                    .filter_map(|e| {
                        let pid = e.process.pid?;
                        Some((e.process.id, pid))
                    })
                    .collect()
            };

            if pids.is_empty() {
                continue;
            }

            let sys_pids: Vec<Pid> = pids.iter().map(|(_, pid)| Pid::from_u32(*pid)).collect();
            let refresh_kind = ProcessRefreshKind::nothing().with_cpu().with_memory();

            // Seed any pids we haven't seen before
            let new_pids: Vec<u32> = pids
                .iter()
                .filter(|(_, pid)| !seeded.contains(pid))
                .map(|(_, pid)| *pid)
                .collect();

            if !new_pids.is_empty() {
                let new_sys_pids: Vec<Pid> =
                    new_pids.iter().map(|pid| Pid::from_u32(*pid)).collect();
                sys.refresh_processes_specifics(
                    ProcessesToUpdate::Some(&new_sys_pids),
                    true,
                    refresh_kind,
                );
                for pid in new_pids {
                    seeded.insert(pid); // seeded in a closure here
                }
                time::sleep(Duration::from_millis(500)).await;
            }

            // Clean up seeded pids that no longer exist
            let active_pids: std::collections::HashSet<u32> =
                pids.iter().map(|(_, pid)| *pid).collect();
            seeded.retain(|pid| active_pids.contains(pid));

            sys.refresh_processes_specifics(ProcessesToUpdate::Some(&sys_pids), true, refresh_kind);

            let mut locked = map.lock().await;
            for (id, pid) in &pids {
                if let Some(sys_proc) = sys.process(Pid::from_u32(*pid))
                    && let Some(entry) = locked.get_mut(id)
                    && entry.process.status == ProcessStatus::Online
                {
                    entry.process.cpu = sys_proc.cpu_usage();
                    entry.process.mem = sys_proc.memory();
                    if let Some(started) = entry.started_at {
                        entry.process.uptime = started.elapsed();
                    }
                }
            }
        }
    });
}
