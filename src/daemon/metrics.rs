use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::time;

use crate::{daemon::manager::ProcessMap, process::ProcessStatus};

pub fn start(map: ProcessMap) {
    tokio::spawn(async move {
        let mut sys = System::new_all(); // seed all processes on startup
        let mut interval = time::interval(Duration::from_secs(2));

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

            // First sample
            sys.refresh_processes_specifics(ProcessesToUpdate::Some(&sys_pids), true, refresh_kind);
            time::sleep(Duration::from_millis(500)).await; // increase delta window slightly

            // Second sample — CPU % now accurate
            sys.refresh_processes_specifics(ProcessesToUpdate::Some(&sys_pids), true, refresh_kind);

            let mut locked = map.lock().await;
            for (id, pid) in &pids {
                if let Some(sys_proc) = sys.process(Pid::from_u32(*pid))
                    && let Some(entry) = locked.get_mut(id)
                    && entry.process.status == ProcessStatus::Online
                // don't update stopped procs
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
