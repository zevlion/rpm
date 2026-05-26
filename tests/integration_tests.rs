use std::io::BufRead;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

fn clean_state() {
    let _ = Command::new("target/debug/rpm").arg("kill").output();
    let _ = std::fs::remove_file("target/debug/rpm.db");
    let _ = std::fs::remove_file("rpm.db");
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        let _ = std::fs::remove_file(parent.join("rpm.db"));
    }
    thread::sleep(Duration::from_millis(1500));
}

#[allow(dead_code)]
struct ProcLsEntry {
    id: u32,
    name: String,
    mode: String,
    pid: Option<u32>,
    status: String,
    restarts: u32,
}

fn parse_ls_output(output: &str) -> Vec<ProcLsEntry> {
    let mut entries = Vec::new();
    for line in output.lines() {
        if !line.starts_with('│') || line.contains("no processes running") || line.contains(" id ")
        {
            continue;
        }
        let parts: Vec<&str> = line.split('│').collect();
        if parts.len() >= 11 {
            let id = parts[1].trim().parse::<u32>();
            let name = parts[2].trim().to_string();
            let mode = parts[3].trim().to_string();
            let pid = parts[4].trim().parse::<u32>().ok();
            let status = parts[8].trim().to_string();
            let restarts = parts[10].trim().parse::<u32>();
            if let (Ok(id), Ok(restarts)) = (id, restarts) {
                entries.push(ProcLsEntry {
                    id,
                    name,
                    mode,
                    pid,
                    status,
                    restarts,
                });
            }
        }
    }
    entries
}

fn wait_for_status(name: &str, expected_status: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let out = Command::new("target/debug/rpm")
            .arg("ls")
            .output()
            .expect("Failed to run ls");
        let s = String::from_utf8_lossy(&out.stdout);
        let entries = parse_ls_output(&s);
        if let Some(e) = entries.iter().find(|e| e.name == name)
            && e.status.contains(expected_status)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Timed out waiting for '{}' to reach status '{}'",
            name,
            expected_status
        );
        thread::sleep(Duration::from_millis(300));
    }
}

fn wait_for_gone(name: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let out = Command::new("target/debug/rpm")
            .arg("ls")
            .output()
            .expect("Failed to run ls");
        let s = String::from_utf8_lossy(&out.stdout);
        let entries = parse_ls_output(&s);
        if !entries.iter().any(|e| e.name == name) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Timed out waiting for '{}' to be removed",
            name
        );
        thread::sleep(Duration::from_millis(300));
    }
}

fn test_version_flag() {
    let out = Command::new("target/debug/rpm")
        .arg("--version")
        .output()
        .expect("Failed to run --version");
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "--version should exit successfully");
    assert!(
        s.contains('.'),
        "--version output should contain a version string, got: {}",
        s
    );

    let out2 = Command::new("target/debug/rpm")
        .arg("-V")
        .output()
        .expect("Failed to run -V");
    assert!(out2.status.success(), "-V should exit successfully");
}

fn test_cli_basic_flow() {
    clean_state();

    let start_output = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("test-fork-basic")
        .arg("--watch")
        .output()
        .expect("Failed to start fork process");
    assert!(start_output.status.success());

    wait_for_status("test-fork-basic", "online", Duration::from_secs(8));

    let ls_output = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls");
    let ls_str = String::from_utf8_lossy(&ls_output.stdout);
    assert!(ls_str.contains("test-fork-basic"));
    assert!(ls_str.contains("online"));

    let stop_output = Command::new("target/debug/rpm")
        .arg("stop")
        .arg("test-fork-basic")
        .output()
        .expect("Failed to stop process");
    assert!(stop_output.status.success());

    wait_for_status("test-fork-basic", "stopped", Duration::from_secs(5));

    let restart_output = Command::new("target/debug/rpm")
        .arg("restart")
        .arg("test-fork-basic")
        .output()
        .expect("Failed to restart process");
    assert!(restart_output.status.success());

    wait_for_status("test-fork-basic", "online", Duration::from_secs(8));

    let delete_output = Command::new("target/debug/rpm")
        .arg("delete")
        .arg("test-fork-basic")
        .output()
        .expect("Failed to delete process");
    assert!(delete_output.status.success());

    wait_for_gone("test-fork-basic", Duration::from_secs(5));

    let _ = Command::new("target/debug/rpm").arg("kill").output();
}

fn test_interpreter_flag() {
    clean_state();

    let start_output = Command::new("target/debug/rpm")
        .arg("start")
        .arg("tests/mock_server.py")
        .arg("--interpreter")
        .arg("python3")
        .arg("--name")
        .arg("test-interp")
        .output()
        .expect("Failed to start with --interpreter");
    assert!(start_output.status.success());

    wait_for_status("test-interp", "online", Duration::from_secs(8));

    let _ = Command::new("target/debug/rpm").arg("kill").output();
}

fn test_force_restart_running_process() {
    clean_state();

    let start1 = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("test-force")
        .output()
        .expect("Failed to start process");
    assert!(start1.status.success());

    wait_for_status("test-force", "online", Duration::from_secs(8));

    let start_err = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("test-force")
        .output()
        .expect("Failed to attempt duplicate start");
    let err_str = String::from_utf8_lossy(&start_err.stderr);
    let out_str = String::from_utf8_lossy(&start_err.stdout);
    assert!(
        err_str.contains("already running")
            || out_str.contains("already running")
            || err_str.contains("already exists"),
        "Expected duplicate start to fail, got stdout: {} stderr: {}",
        out_str,
        err_str
    );

    let ls_before = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls");
    let entries_before = parse_ls_output(&String::from_utf8_lossy(&ls_before.stdout));
    let pid_before = entries_before
        .iter()
        .find(|e| e.name == "test-force")
        .unwrap()
        .pid;

    let force_output = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("test-force")
        .arg("--force")
        .output()
        .expect("Failed to start with --force");
    assert!(force_output.status.success());

    wait_for_status("test-force", "online", Duration::from_secs(8));

    let ls_after = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls after --force");
    let entries_after = parse_ls_output(&String::from_utf8_lossy(&ls_after.stdout));
    let entry_after = entries_after
        .iter()
        .find(|e| e.name == "test-force")
        .unwrap();
    assert_eq!(
        entry_after.id, entries_before[0].id,
        "ID should not change after --force"
    );
    assert_ne!(
        entry_after.pid, pid_before,
        "PID should change after --force restart"
    );

    let _ = Command::new("target/debug/rpm").arg("kill").output();
}

fn test_watch_crash_recovery() {
    clean_state();

    let start_output = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("test-watch")
        .arg("--watch")
        .output()
        .expect("Failed to start watched process");
    assert!(start_output.status.success());

    wait_for_status("test-watch", "online", Duration::from_secs(8));

    let ls_output = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls");
    let entries = parse_ls_output(&String::from_utf8_lossy(&ls_output.stdout));
    let pid_before = entries
        .iter()
        .find(|e| e.name == "test-watch")
        .unwrap()
        .pid
        .expect("Process should have a PID");

    #[cfg(unix)]
    {
        let kill_status = Command::new("kill")
            .arg("-9")
            .arg(pid_before.to_string())
            .status()
            .expect("Failed to kill process");
        assert!(kill_status.success());
    }

    // Wait until the daemon detects the crash (PID disappears or restarts counter increments)
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let out = Command::new("target/debug/rpm")
            .arg("ls")
            .output()
            .expect("Failed to run ls");
        let entries = parse_ls_output(&String::from_utf8_lossy(&out.stdout));
        if let Some(e) = entries.iter().find(|e| e.name == "test-watch")
            && e.restarts >= 1
            && e.status.contains("online")
            && e.pid != Some(pid_before)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Timed out waiting for test-watch to restart with a new PID"
        );
        thread::sleep(Duration::from_millis(300));
    }

    let ls_after = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls after crash");
    let entries_after = parse_ls_output(&String::from_utf8_lossy(&ls_after.stdout));
    let entry_after = entries_after
        .iter()
        .find(|e| e.name == "test-watch")
        .unwrap();

    assert_ne!(
        entry_after.pid,
        Some(pid_before),
        "Process should have a new PID after crash recovery"
    );
    assert!(entry_after.restarts >= 1, "Restart counter should be >= 1");

    let _ = Command::new("target/debug/rpm").arg("kill").output();
}

#[cfg(unix)]
fn test_attach_ctrlc() {
    clean_state();

    let mut child = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("test-attach-ctrlc")
        .arg("--attach")
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to spawn attach process");

    let stdout = child.stdout.as_mut().expect("No stdout handle");
    let mut reader = std::io::BufReader::new(stdout);
    let mut found = false;
    let mut line = String::new();

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        line.clear();
        if let Ok(n) = reader.read_line(&mut line) {
            if n == 0 {
                break;
            }
            println!("Captured attach output: {}", line.trim());
            if line.contains("Mock server running") {
                found = true;
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "Timed out waiting for mock server startup output"
        );
        thread::sleep(Duration::from_millis(50));
    }

    assert!(
        found,
        "Did not find expected mock server start output in stdout"
    );

    let pid = child.id();
    let status = Command::new("kill")
        .arg("-2")
        .arg(pid.to_string())
        .status()
        .expect("Failed to send SIGINT to child");
    assert!(status.success());

    let wait_res = child.wait().expect("Failed to wait on child");
    println!("Attach client exited with: {:?}", wait_res);

    wait_for_status("test-attach-ctrlc", "online", Duration::from_secs(8));

    let _ = Command::new("target/debug/rpm")
        .arg("delete")
        .arg("test-attach-ctrlc")
        .output();
    let _ = Command::new("target/debug/rpm").arg("kill").output();
}

fn test_cli_id_reset_and_reuse() {
    clean_state();

    let _ = Command::new("target/debug/rpm")
        .arg("delete")
        .arg("all")
        .output()
        .expect("Failed to delete all");
    thread::sleep(Duration::from_millis(500));

    let start_a = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("proc-a")
        .output()
        .expect("Failed to start proc-a");
    assert!(start_a.status.success());
    wait_for_status("proc-a", "online", Duration::from_secs(8));

    let ls1 = Command::new("target/debug/rpm").arg("ls").output().unwrap();
    let entries1 = parse_ls_output(&String::from_utf8_lossy(&ls1.stdout));
    assert_eq!(entries1.len(), 1);
    assert_eq!(entries1[0].name, "proc-a");
    assert_eq!(entries1[0].id, 0);
    assert!(entries1[0].status.contains("online"));

    let stop_a = Command::new("target/debug/rpm")
        .arg("stop")
        .arg("proc-a")
        .output()
        .expect("Failed to stop proc-a");
    assert!(stop_a.status.success());
    wait_for_status("proc-a", "stopped", Duration::from_secs(5));

    let start_b = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("proc-b")
        .output()
        .expect("Failed to start proc-b");
    assert!(start_b.status.success());
    wait_for_status("proc-b", "online", Duration::from_secs(8));

    let ls3 = Command::new("target/debug/rpm").arg("ls").output().unwrap();
    let entries3 = parse_ls_output(&String::from_utf8_lossy(&ls3.stdout));
    assert_eq!(entries3.len(), 2);
    let a = entries3.iter().find(|e| e.name == "proc-a").unwrap();
    let b = entries3.iter().find(|e| e.name == "proc-b").unwrap();
    assert_eq!(a.id, 0);
    assert!(a.status.contains("stopped"));
    assert_eq!(b.id, 1);
    assert!(b.status.contains("online"));

    let start_a_again = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("proc-a")
        .output()
        .expect("Failed to restart stopped proc-a");
    assert!(start_a_again.status.success());
    wait_for_status("proc-a", "online", Duration::from_secs(8));

    let ls4 = Command::new("target/debug/rpm").arg("ls").output().unwrap();
    let entries4 = parse_ls_output(&String::from_utf8_lossy(&ls4.stdout));
    let a4 = entries4.iter().find(|e| e.name == "proc-a").unwrap();
    let b4 = entries4.iter().find(|e| e.name == "proc-b").unwrap();
    assert_eq!(a4.id, 0);
    assert!(a4.status.contains("online"));
    assert_eq!(b4.id, 1);

    let del_a = Command::new("target/debug/rpm")
        .arg("delete")
        .arg("proc-a")
        .output()
        .expect("Failed to delete proc-a");
    assert!(del_a.status.success());
    wait_for_gone("proc-a", Duration::from_secs(5));

    let del_b = Command::new("target/debug/rpm")
        .arg("delete")
        .arg("proc-b")
        .output()
        .expect("Failed to delete proc-b");
    assert!(del_b.status.success());
    wait_for_gone("proc-b", Duration::from_secs(5));

    let ls6 = Command::new("target/debug/rpm").arg("ls").output().unwrap();
    assert!(String::from_utf8_lossy(&ls6.stdout).contains("no processes running"));

    let start_c = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("proc-c")
        .output()
        .expect("Failed to start proc-c");
    assert!(start_c.status.success());
    wait_for_status("proc-c", "online", Duration::from_secs(8));

    let ls7 = Command::new("target/debug/rpm").arg("ls").output().unwrap();
    let entries7 = parse_ls_output(&String::from_utf8_lossy(&ls7.stdout));
    assert_eq!(entries7.len(), 1);
    assert_eq!(entries7[0].name, "proc-c");
    assert_eq!(entries7[0].id, 0);

    let _ = Command::new("target/debug/rpm").arg("kill").output();
}

fn test_cli_delete_all_reset() {
    clean_state();

    for name in &["proc-a", "proc-b"] {
        let out = Command::new("target/debug/rpm")
            .arg("start")
            .arg("python3")
            .arg("--")
            .arg("tests/mock_server.py")
            .arg("--name")
            .arg(name)
            .output()
            .expect("Failed to start process");
        assert!(out.status.success());
        wait_for_status(name, "online", Duration::from_secs(8));
    }

    let del_all = Command::new("target/debug/rpm")
        .arg("delete")
        .arg("all")
        .output()
        .expect("Failed to delete all");
    assert!(del_all.status.success());
    wait_for_gone("proc-a", Duration::from_secs(5));
    wait_for_gone("proc-b", Duration::from_secs(5));

    let start_c = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("proc-c")
        .output()
        .expect("Failed to start proc-c");
    assert!(start_c.status.success());
    wait_for_status("proc-c", "online", Duration::from_secs(8));

    let ls = Command::new("target/debug/rpm").arg("ls").output().unwrap();
    let entries = parse_ls_output(&String::from_utf8_lossy(&ls.stdout));
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "proc-c");
    assert_eq!(entries[0].id, 0);

    let _ = Command::new("target/debug/rpm").arg("kill").output();
}

fn test_cli_restart_all() {
    clean_state();

    for name in &["proc-a", "proc-b"] {
        let out = Command::new("target/debug/rpm")
            .arg("start")
            .arg("python3")
            .arg("--")
            .arg("tests/mock_server.py")
            .arg("--name")
            .arg(name)
            .output()
            .expect("Failed to start process");
        assert!(out.status.success());
        wait_for_status(name, "online", Duration::from_secs(8));
    }

    let ls1 = Command::new("target/debug/rpm").arg("ls").output().unwrap();
    let entries1 = parse_ls_output(&String::from_utf8_lossy(&ls1.stdout));
    let pid_a = entries1.iter().find(|e| e.name == "proc-a").unwrap().pid;
    let pid_b = entries1.iter().find(|e| e.name == "proc-b").unwrap().pid;
    assert!(pid_a.is_some());
    assert!(pid_b.is_some());

    let restart_all = Command::new("target/debug/rpm")
        .arg("restart")
        .arg("all")
        .output()
        .expect("Failed to restart all");
    assert!(restart_all.status.success());

    wait_for_status("proc-a", "online", Duration::from_secs(8));
    wait_for_status("proc-b", "online", Duration::from_secs(8));

    let ls2 = Command::new("target/debug/rpm").arg("ls").output().unwrap();
    let entries2 = parse_ls_output(&String::from_utf8_lossy(&ls2.stdout));
    let a = entries2.iter().find(|e| e.name == "proc-a").unwrap();
    let b = entries2.iter().find(|e| e.name == "proc-b").unwrap();

    assert_ne!(a.pid, pid_a, "proc-a should have a new PID after restart");
    assert_ne!(b.pid, pid_b, "proc-b should have a new PID after restart");
    assert_eq!(a.restarts, 1);
    assert_eq!(b.restarts, 1);

    let _ = Command::new("target/debug/rpm").arg("kill").output();
}

fn test_by_id() {
    clean_state();

    let out = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("test-by-id")
        .output()
        .expect("Failed to start process");
    assert!(out.status.success());
    wait_for_status("test-by-id", "online", Duration::from_secs(8));

    let stop_out = Command::new("target/debug/rpm")
        .arg("stop")
        .arg("0")
        .output()
        .expect("Failed to stop by id");
    assert!(stop_out.status.success());
    wait_for_status("test-by-id", "stopped", Duration::from_secs(5));

    let restart_out = Command::new("target/debug/rpm")
        .arg("restart")
        .arg("0")
        .output()
        .expect("Failed to restart by id");
    assert!(restart_out.status.success());
    wait_for_status("test-by-id", "online", Duration::from_secs(8));

    let delete_out = Command::new("target/debug/rpm")
        .arg("delete")
        .arg("0")
        .output()
        .expect("Failed to delete by id");
    assert!(delete_out.status.success());
    wait_for_gone("test-by-id", Duration::from_secs(5));

    let _ = Command::new("target/debug/rpm").arg("kill").output();
}

#[test]
fn run_all_integration_tests() {
    test_version_flag();
    test_cli_basic_flow();
    test_interpreter_flag();
    test_force_restart_running_process();
    test_watch_crash_recovery();
    #[cfg(unix)]
    test_attach_ctrlc();
    test_cli_id_reset_and_reuse();
    test_cli_delete_all_reset();
    test_cli_restart_all();
    test_by_id();
}
