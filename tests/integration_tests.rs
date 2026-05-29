use std::io::BufRead;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

fn log(test: &str, msg: &str) {
    println!("[{}] {}", test, msg);
}

fn rpm_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rpm"))
}

fn clean_state(test: &str) {
    log(test, "Cleaning state...");
    let _ = rpm_cmd().arg("kill").output();

    let _ = Command::new("pkill")
        .arg("-f")
        .arg("mock_server.py")
        .output();

    let _ = std::fs::remove_file("target/debug/rpm.db");
    let _ = std::fs::remove_file("rpm.db");
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        let _ = std::fs::remove_file(parent.join("rpm.db"));
    }
    log(test, "Sleeping 1500ms for daemon and processes to die...");
    thread::sleep(Duration::from_millis(1500));
    log(test, "State cleaned.");
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

fn wait_for_status(test: &str, name: &str, expected_status: &str, timeout: Duration) {
    log(
        test,
        &format!(
            "Waiting for '{}' to reach status '{}'...",
            name, expected_status
        ),
    );
    let deadline = Instant::now() + timeout;
    loop {
        let out = rpm_cmd().arg("ls").output().expect("Failed to run ls");
        let s = String::from_utf8_lossy(&out.stdout);
        let entries = parse_ls_output(&s);
        if let Some(e) = entries.iter().find(|e| e.name == name) {
            log(
                test,
                &format!("  '{}' current status: '{}'", name, e.status),
            );
            if e.status.contains(expected_status) {
                log(
                    test,
                    &format!("  '{}' reached expected status '{}'", name, expected_status),
                );
                return;
            }
        } else {
            log(test, &format!("  '{}' not found in ls output yet", name));
        }
        assert!(
            Instant::now() < deadline,
            "[{}] Timed out waiting for '{}' to reach status '{}'",
            test,
            name,
            expected_status
        );
        thread::sleep(Duration::from_millis(300));
    }
}

fn wait_for_gone(test: &str, name: &str, timeout: Duration) {
    log(test, &format!("Waiting for '{}' to be removed...", name));
    let deadline = Instant::now() + timeout;
    loop {
        let out = rpm_cmd().arg("ls").output().expect("Failed to run ls");
        let s = String::from_utf8_lossy(&out.stdout);
        let entries = parse_ls_output(&s);
        if !entries.iter().any(|e| e.name == name) {
            log(test, &format!("  '{}' is gone.", name));
            return;
        }
        log(test, &format!("  '{}' still present, waiting...", name));
        assert!(
            Instant::now() < deadline,
            "[{}] Timed out waiting for '{}' to be removed",
            test,
            name
        );
        thread::sleep(Duration::from_millis(300));
    }
}

fn test_version_flag() {
    let t = "test_version_flag";
    log(t, "Running --version...");
    let out = rpm_cmd()
        .arg("--version")
        .output()
        .expect("Failed to run --version");
    let s = String::from_utf8_lossy(&out.stdout);
    log(t, &format!("--version output: {}", s.trim()));
    assert!(out.status.success(), "--version should exit successfully");
    assert!(
        s.contains('.'),
        "--version output should contain a version string, got: {}",
        s
    );

    log(t, "Running -V...");
    let out2 = rpm_cmd().arg("-V").output().expect("Failed to run -V");
    log(
        t,
        &format!(
            "-V output: {}",
            String::from_utf8_lossy(&out2.stdout).trim()
        ),
    );
    assert!(out2.status.success(), "-V should exit successfully");
    log(t, "PASSED");
}

fn test_cli_basic_flow() {
    let t = "test_cli_basic_flow";
    clean_state(t);

    log(t, "Starting test-fork-basic...");
    let start_output = rpm_cmd()
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("test-fork-basic")
        .arg("--watch")
        .output()
        .expect("Failed to start fork process");
    log(
        t,
        &format!(
            "start stdout: {}",
            String::from_utf8_lossy(&start_output.stdout).trim()
        ),
    );
    log(
        t,
        &format!(
            "start stderr: {}",
            String::from_utf8_lossy(&start_output.stderr).trim()
        ),
    );
    assert!(start_output.status.success());

    wait_for_status(t, "test-fork-basic", "online", Duration::from_secs(8));

    log(t, "Running ls...");
    let ls_output = rpm_cmd().arg("ls").output().expect("Failed to run ls");
    let ls_str = String::from_utf8_lossy(&ls_output.stdout);
    log(t, &format!("ls output:\n{}", ls_str));
    assert!(ls_str.contains("test-fork-basic"));
    assert!(ls_str.contains("online"));

    log(t, "Stopping test-fork-basic...");
    let stop_output = rpm_cmd()
        .arg("stop")
        .arg("test-fork-basic")
        .output()
        .expect("Failed to stop process");
    log(t, &format!("stop exit status: {}", stop_output.status));
    assert!(stop_output.status.success());

    wait_for_status(t, "test-fork-basic", "stopped", Duration::from_secs(5));

    log(t, "Restarting test-fork-basic...");
    let restart_output = rpm_cmd()
        .arg("restart")
        .arg("test-fork-basic")
        .output()
        .expect("Failed to restart process");
    log(
        t,
        &format!("restart exit status: {}", restart_output.status),
    );
    assert!(restart_output.status.success());

    wait_for_status(t, "test-fork-basic", "online", Duration::from_secs(8));

    log(t, "Deleting test-fork-basic...");
    let delete_output = rpm_cmd()
        .arg("delete")
        .arg("test-fork-basic")
        .output()
        .expect("Failed to delete process");
    log(t, &format!("delete exit status: {}", delete_output.status));
    assert!(delete_output.status.success());

    wait_for_gone(t, "test-fork-basic", Duration::from_secs(5));

    log(t, "Killing daemon...");
    let _ = rpm_cmd().arg("kill").output();
    log(t, "PASSED");
}

fn test_interpreter_flag() {
    let t = "test_interpreter_flag";
    clean_state(t);

    log(t, "Starting with --interpreter python3...");
    let start_output = rpm_cmd()
        .arg("start")
        .arg("tests/mock_server.py")
        .arg("--interpreter")
        .arg("python3")
        .arg("--name")
        .arg("test-interp")
        .output()
        .expect("Failed to start with --interpreter");
    log(
        t,
        &format!(
            "start stdout: {}",
            String::from_utf8_lossy(&start_output.stdout).trim()
        ),
    );
    log(
        t,
        &format!(
            "start stderr: {}",
            String::from_utf8_lossy(&start_output.stderr).trim()
        ),
    );
    assert!(start_output.status.success());

    wait_for_status(t, "test-interp", "online", Duration::from_secs(8));

    log(t, "Killing daemon...");
    let _ = rpm_cmd().arg("kill").output();
    log(t, "PASSED");
}

fn test_force_restart_running_process() {
    let t = "test_force_restart_running_process";
    clean_state(t);

    log(t, "Starting test-force...");
    let start1 = rpm_cmd()
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("test-force")
        .output()
        .expect("Failed to start process");
    log(
        t,
        &format!(
            "start stdout: {}",
            String::from_utf8_lossy(&start1.stdout).trim()
        ),
    );
    log(
        t,
        &format!(
            "start stderr: {}",
            String::from_utf8_lossy(&start1.stderr).trim()
        ),
    );
    assert!(start1.status.success());

    wait_for_status(t, "test-force", "online", Duration::from_secs(8));

    log(t, "Attempting duplicate start (should fail)...");
    let start_err = rpm_cmd()
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
    log(t, &format!("duplicate start stdout: {}", out_str.trim()));
    log(t, &format!("duplicate start stderr: {}", err_str.trim()));
    assert!(
        err_str.contains("already running")
            || out_str.contains("already running")
            || err_str.contains("already exists"),
        "Expected duplicate start to fail, got stdout: {} stderr: {}",
        out_str,
        err_str
    );

    let ls_before = rpm_cmd().arg("ls").output().expect("Failed to run ls");
    let entries_before = parse_ls_output(&String::from_utf8_lossy(&ls_before.stdout));
    let pid_before = entries_before
        .iter()
        .find(|e| e.name == "test-force")
        .unwrap()
        .pid;
    log(t, &format!("PID before --force: {:?}", pid_before));

    log(t, "Starting with --force...");
    let force_output = rpm_cmd()
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("test-force")
        .arg("--force")
        .output()
        .expect("Failed to start with --force");
    log(
        t,
        &format!(
            "--force stdout: {}",
            String::from_utf8_lossy(&force_output.stdout).trim()
        ),
    );
    log(
        t,
        &format!(
            "--force stderr: {}",
            String::from_utf8_lossy(&force_output.stderr).trim()
        ),
    );
    assert!(force_output.status.success());

    wait_for_status(t, "test-force", "online", Duration::from_secs(8));

    let ls_after = rpm_cmd()
        .arg("ls")
        .output()
        .expect("Failed to run ls after --force");
    let entries_after = parse_ls_output(&String::from_utf8_lossy(&ls_after.stdout));
    let entry_after = entries_after
        .iter()
        .find(|e| e.name == "test-force")
        .unwrap();
    log(
        t,
        &format!(
            "PID after --force: {:?}, ID: {}",
            entry_after.pid, entry_after.id
        ),
    );
    assert_eq!(
        entry_after.id, entries_before[0].id,
        "ID should not change after --force"
    );
    assert_ne!(
        entry_after.pid, pid_before,
        "PID should change after --force restart"
    );

    log(t, "Killing daemon...");
    let _ = rpm_cmd().arg("kill").output();
    log(t, "PASSED");
}

fn test_watch_crash_recovery() {
    let t = "test_watch_crash_recovery";
    clean_state(t);

    log(t, "Starting test-watch with --watch...");
    let start_output = rpm_cmd()
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("test-watch")
        .arg("--watch")
        .output()
        .expect("Failed to start watched process");
    log(
        t,
        &format!(
            "start stdout: {}",
            String::from_utf8_lossy(&start_output.stdout).trim()
        ),
    );
    log(
        t,
        &format!(
            "start stderr: {}",
            String::from_utf8_lossy(&start_output.stderr).trim()
        ),
    );
    assert!(start_output.status.success());

    wait_for_status(t, "test-watch", "online", Duration::from_secs(8));

    let ls_output = rpm_cmd().arg("ls").output().expect("Failed to run ls");
    let entries = parse_ls_output(&String::from_utf8_lossy(&ls_output.stdout));
    let pid_before = entries
        .iter()
        .find(|e| e.name == "test-watch")
        .unwrap()
        .pid
        .expect("Process should have a PID");
    log(t, &format!("PID before crash: {}", pid_before));

    #[cfg(unix)]
    {
        log(t, &format!("Sending SIGKILL to PID {}...", pid_before));
        let kill_status = Command::new("kill")
            .arg("-9")
            .arg(pid_before.to_string())
            .status()
            .expect("Failed to kill process");
        log(t, &format!("kill exit status: {}", kill_status));
        assert!(kill_status.success());
    }

    log(t, "Waiting for daemon to detect crash and restart...");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let out = rpm_cmd().arg("ls").output().expect("Failed to run ls");
        let entries = parse_ls_output(&String::from_utf8_lossy(&out.stdout));
        if let Some(e) = entries.iter().find(|e| e.name == "test-watch") {
            log(
                t,
                &format!(
                    "  status: '{}', restarts: {}, pid: {:?}",
                    e.status, e.restarts, e.pid
                ),
            );
            if e.restarts >= 1 && e.status.contains("online") && e.pid != Some(pid_before) {
                log(t, "  Crash recovery confirmed.");
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "[{}] Timed out waiting for test-watch to restart with a new PID",
            t
        );
        thread::sleep(Duration::from_millis(300));
    }

    let ls_after = rpm_cmd()
        .arg("ls")
        .output()
        .expect("Failed to run ls after crash");
    let entries_after = parse_ls_output(&String::from_utf8_lossy(&ls_after.stdout));
    let entry_after = entries_after
        .iter()
        .find(|e| e.name == "test-watch")
        .unwrap();
    log(
        t,
        &format!(
            "Final PID: {:?}, restarts: {}",
            entry_after.pid, entry_after.restarts
        ),
    );

    assert_ne!(
        entry_after.pid,
        Some(pid_before),
        "Process should have a new PID after crash recovery"
    );
    assert!(entry_after.restarts >= 1, "Restart counter should be >= 1");

    log(t, "Killing daemon...");
    let _ = rpm_cmd().arg("kill").output();
    log(t, "PASSED");
}

#[cfg(unix)]
fn test_attach_ctrlc() {
    let t = "test_attach_ctrlc";
    clean_state(t);

    log(t, "Spawning with --attach...");
    let mut child = rpm_cmd()
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

    log(t, "Reading stdout until 'Mock server running' appears...");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        line.clear();
        if let Ok(n) = reader.read_line(&mut line) {
            if n == 0 {
                break;
            }
            log(t, &format!("  attach stdout: {}", line.trim()));
            if line.contains("Mock server running") {
                found = true;
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "[{}] Timed out waiting for mock server startup output",
            t
        );
        thread::sleep(Duration::from_millis(50));
    }
    assert!(
        found,
        "Did not find expected mock server start output in stdout"
    );

    let pid = child.id();
    log(
        t,
        &format!("Sending SIGINT to attach client PID {}...", pid),
    );
    let status = Command::new("kill")
        .arg("-2")
        .arg(pid.to_string())
        .status()
        .expect("Failed to send SIGINT to child");
    log(t, &format!("SIGINT status: {}", status));
    assert!(status.success());

    let wait_res = child.wait().expect("Failed to wait on child");
    log(t, &format!("Attach client exited with: {:?}", wait_res));

    wait_for_status(t, "test-attach-ctrlc", "online", Duration::from_secs(8));

    log(t, "Cleaning up...");
    let _ = rpm_cmd().arg("delete").arg("test-attach-ctrlc").output();
    let _ = rpm_cmd().arg("kill").output();
    log(t, "PASSED");
}

fn test_cli_id_reset_and_reuse() {
    let t = "test_cli_id_reset_and_reuse";
    clean_state(t);

    log(t, "Deleting all processes...");
    let _ = rpm_cmd()
        .arg("delete")
        .arg("all")
        .output()
        .expect("Failed to delete all");
    thread::sleep(Duration::from_millis(500));

    log(t, "Starting proc-a...");
    let start_a = rpm_cmd()
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("proc-a")
        .output()
        .expect("Failed to start proc-a");
    log(
        t,
        &format!(
            "proc-a start stdout: {}",
            String::from_utf8_lossy(&start_a.stdout).trim()
        ),
    );
    assert!(start_a.status.success());
    wait_for_status(t, "proc-a", "online", Duration::from_secs(8));

    let ls1 = rpm_cmd().arg("ls").output().unwrap();
    let entries1 = parse_ls_output(&String::from_utf8_lossy(&ls1.stdout));
    log(
        t,
        &format!(
            "ls after proc-a start: {:?}",
            entries1
                .iter()
                .map(|e| format!("{}(id={},status={})", e.name, e.id, e.status))
                .collect::<Vec<_>>()
        ),
    );
    assert_eq!(entries1.len(), 1);
    assert_eq!(entries1[0].name, "proc-a");
    assert_eq!(entries1[0].id, 0);
    assert!(entries1[0].status.contains("online"));

    log(t, "Stopping proc-a...");
    let stop_a = rpm_cmd()
        .arg("stop")
        .arg("proc-a")
        .output()
        .expect("Failed to stop proc-a");
    assert!(stop_a.status.success());
    wait_for_status(t, "proc-a", "stopped", Duration::from_secs(5));

    log(t, "Starting proc-b...");
    let start_b = rpm_cmd()
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("proc-b")
        .output()
        .expect("Failed to start proc-b");
    log(
        t,
        &format!(
            "proc-b start stdout: {}",
            String::from_utf8_lossy(&start_b.stdout).trim()
        ),
    );
    assert!(start_b.status.success());
    wait_for_status(t, "proc-b", "online", Duration::from_secs(8));

    let ls3 = rpm_cmd().arg("ls").output().unwrap();
    let entries3 = parse_ls_output(&String::from_utf8_lossy(&ls3.stdout));
    log(
        t,
        &format!(
            "ls after proc-b start: {:?}",
            entries3
                .iter()
                .map(|e| format!("{}(id={},status={})", e.name, e.id, e.status))
                .collect::<Vec<_>>()
        ),
    );
    assert_eq!(entries3.len(), 2);
    let a = entries3.iter().find(|e| e.name == "proc-a").unwrap();
    let b = entries3.iter().find(|e| e.name == "proc-b").unwrap();
    assert_eq!(a.id, 0);
    assert!(a.status.contains("stopped"));
    assert_eq!(b.id, 1);
    assert!(b.status.contains("online"));

    log(t, "Restarting stopped proc-a...");
    let start_a_again = rpm_cmd()
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("proc-a")
        .output()
        .expect("Failed to restart stopped proc-a");
    assert!(start_a_again.status.success());
    wait_for_status(t, "proc-a", "online", Duration::from_secs(8));

    let ls4 = rpm_cmd().arg("ls").output().unwrap();
    let entries4 = parse_ls_output(&String::from_utf8_lossy(&ls4.stdout));
    let a4 = entries4.iter().find(|e| e.name == "proc-a").unwrap();
    let b4 = entries4.iter().find(|e| e.name == "proc-b").unwrap();
    log(
        t,
        &format!(
            "ls after proc-a restart: a.id={}, a.status={}, b.id={}",
            a4.id, a4.status, b4.id
        ),
    );
    assert_eq!(a4.id, 0);
    assert!(a4.status.contains("online"));
    assert_eq!(b4.id, 1);

    log(t, "Deleting proc-a...");
    let del_a = rpm_cmd()
        .arg("delete")
        .arg("proc-a")
        .output()
        .expect("Failed to delete proc-a");
    assert!(del_a.status.success());
    wait_for_gone(t, "proc-a", Duration::from_secs(5));

    log(t, "Deleting proc-b...");
    let del_b = rpm_cmd()
        .arg("delete")
        .arg("proc-b")
        .output()
        .expect("Failed to delete proc-b");
    assert!(del_b.status.success());
    wait_for_gone(t, "proc-b", Duration::from_secs(5));

    let ls6 = rpm_cmd().arg("ls").output().unwrap();
    let ls6_str = String::from_utf8_lossy(&ls6.stdout);
    log(t, &format!("ls after all deleted: {}", ls6_str.trim()));
    assert!(ls6_str.contains("no processes running"));

    log(t, "Starting proc-c (ID should reset to 0)...");
    let start_c = rpm_cmd()
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("proc-c")
        .output()
        .expect("Failed to start proc-c");
    assert!(start_c.status.success());
    wait_for_status(t, "proc-c", "online", Duration::from_secs(8));

    let ls7 = rpm_cmd().arg("ls").output().unwrap();
    let entries7 = parse_ls_output(&String::from_utf8_lossy(&ls7.stdout));
    log(
        t,
        &format!(
            "ls final: {:?}",
            entries7
                .iter()
                .map(|e| format!("{}(id={},status={})", e.name, e.id, e.status))
                .collect::<Vec<_>>()
        ),
    );
    assert_eq!(entries7.len(), 1);
    assert_eq!(entries7[0].name, "proc-c");
    assert_eq!(entries7[0].id, 0);

    log(t, "Killing daemon...");
    let _ = rpm_cmd().arg("kill").output();
    log(t, "PASSED");
}

fn test_cli_delete_all_reset() {
    let t = "test_cli_delete_all_reset";
    clean_state(t);

    for name in &["proc-a", "proc-b"] {
        log(t, &format!("Starting {}...", name));
        let out = rpm_cmd()
            .arg("start")
            .arg("python3")
            .arg("--")
            .arg("tests/mock_server.py")
            .arg("--name")
            .arg(name)
            .output()
            .expect("Failed to start process");
        log(
            t,
            &format!(
                "{} start stdout: {}",
                name,
                String::from_utf8_lossy(&out.stdout).trim()
            ),
        );
        assert!(out.status.success());
        wait_for_status(t, name, "online", Duration::from_secs(8));
    }

    log(t, "Running delete all...");
    let del_all = rpm_cmd()
        .arg("delete")
        .arg("all")
        .output()
        .expect("Failed to delete all");
    log(t, &format!("delete all exit status: {}", del_all.status));
    assert!(del_all.status.success());
    wait_for_gone(t, "proc-a", Duration::from_secs(5));
    wait_for_gone(t, "proc-b", Duration::from_secs(5));

    log(
        t,
        "Starting proc-c after delete all (ID should reset to 0)...",
    );
    let start_c = rpm_cmd()
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("proc-c")
        .output()
        .expect("Failed to start proc-c");
    assert!(start_c.status.success());
    wait_for_status(t, "proc-c", "online", Duration::from_secs(8));

    let ls = rpm_cmd().arg("ls").output().unwrap();
    let entries = parse_ls_output(&String::from_utf8_lossy(&ls.stdout));
    log(
        t,
        &format!(
            "ls final: {:?}",
            entries
                .iter()
                .map(|e| format!("{}(id={},status={})", e.name, e.id, e.status))
                .collect::<Vec<_>>()
        ),
    );
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "proc-c");
    assert_eq!(entries[0].id, 0);

    log(t, "Killing daemon...");
    let _ = rpm_cmd().arg("kill").output();
    log(t, "PASSED");
}

fn test_cli_restart_all() {
    let t = "test_cli_restart_all";
    clean_state(t);

    for name in &["proc-a", "proc-b"] {
        log(t, &format!("Starting {}...", name));
        let out = rpm_cmd()
            .arg("start")
            .arg("python3")
            .arg("--")
            .arg("tests/mock_server.py")
            .arg("--name")
            .arg(name)
            .output()
            .expect("Failed to start process");
        log(
            t,
            &format!(
                "{} start stdout: {}",
                name,
                String::from_utf8_lossy(&out.stdout).trim()
            ),
        );
        assert!(out.status.success());
        wait_for_status(t, name, "online", Duration::from_secs(8));
    }

    let ls1 = rpm_cmd().arg("ls").output().unwrap();
    let entries1 = parse_ls_output(&String::from_utf8_lossy(&ls1.stdout));
    let pid_a = entries1.iter().find(|e| e.name == "proc-a").unwrap().pid;
    let pid_b = entries1.iter().find(|e| e.name == "proc-b").unwrap().pid;
    log(
        t,
        &format!(
            "PIDs before restart all: proc-a={:?}, proc-b={:?}",
            pid_a, pid_b
        ),
    );
    assert!(pid_a.is_some());
    assert!(pid_b.is_some());

    log(t, "Running restart all...");
    let restart_all = rpm_cmd()
        .arg("restart")
        .arg("all")
        .output()
        .expect("Failed to restart all");
    log(
        t,
        &format!("restart all exit status: {}", restart_all.status),
    );
    assert!(restart_all.status.success());

    wait_for_status(t, "proc-a", "online", Duration::from_secs(8));
    wait_for_status(t, "proc-b", "online", Duration::from_secs(8));

    let ls2 = rpm_cmd().arg("ls").output().unwrap();
    let entries2 = parse_ls_output(&String::from_utf8_lossy(&ls2.stdout));
    let a = entries2.iter().find(|e| e.name == "proc-a").unwrap();
    let b = entries2.iter().find(|e| e.name == "proc-b").unwrap();
    log(
        t,
        &format!(
            "PIDs after restart all: proc-a={:?}(restarts={}), proc-b={:?}(restarts={})",
            a.pid, a.restarts, b.pid, b.restarts
        ),
    );

    assert_ne!(a.pid, pid_a, "proc-a should have a new PID after restart");
    assert_ne!(b.pid, pid_b, "proc-b should have a new PID after restart");
    assert_eq!(a.restarts, 1);
    assert_eq!(b.restarts, 1);

    log(t, "Killing daemon...");
    let _ = rpm_cmd().arg("kill").output();
    log(t, "PASSED");
}

fn test_by_id() {
    let t = "test_by_id";
    clean_state(t);

    log(t, "Starting test-by-id...");
    let out = rpm_cmd()
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("test-by-id")
        .output()
        .expect("Failed to start process");
    log(
        t,
        &format!(
            "start stdout: {}",
            String::from_utf8_lossy(&out.stdout).trim()
        ),
    );
    assert!(out.status.success());
    wait_for_status(t, "test-by-id", "online", Duration::from_secs(8));

    log(t, "Stopping by id=0...");
    let stop_out = rpm_cmd()
        .arg("stop")
        .arg("0")
        .output()
        .expect("Failed to stop by id");
    log(t, &format!("stop exit status: {}", stop_out.status));
    assert!(stop_out.status.success());
    wait_for_status(t, "test-by-id", "stopped", Duration::from_secs(5));

    log(t, "Restarting by id=0...");
    let restart_out = rpm_cmd()
        .arg("restart")
        .arg("0")
        .output()
        .expect("Failed to restart by id");
    log(t, &format!("restart exit status: {}", restart_out.status));
    assert!(restart_out.status.success());
    wait_for_status(t, "test-by-id", "online", Duration::from_secs(8));

    log(t, "Deleting by id=0...");
    let delete_out = rpm_cmd()
        .arg("delete")
        .arg("0")
        .output()
        .expect("Failed to delete by id");
    log(t, &format!("delete exit status: {}", delete_out.status));
    assert!(delete_out.status.success());
    wait_for_gone(t, "test-by-id", Duration::from_secs(5));

    log(t, "Killing daemon...");
    let _ = rpm_cmd().arg("kill").output();
    log(t, "PASSED");
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
