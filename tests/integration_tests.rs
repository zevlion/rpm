use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Command;
use std::thread;
use std::time::Duration;

fn send_request(port: u16, msg: &str) -> Result<String, std::io::Error> {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))?;
    stream.write_all(msg.as_bytes())?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

fn clean_state() {
    let _ = Command::new("target/debug/rpm").arg("kill").output();
    // Remove DB from all possible locations
    let _ = std::fs::remove_file("target/debug/rpm.db");
    let _ = std::fs::remove_file("rpm.db");
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        let _ = std::fs::remove_file(parent.join("rpm.db"));
    }
    thread::sleep(Duration::from_millis(1500)); // bump from 1000ms
}

fn test_cli_basic_flow() {
    // 1. Clean daemon state
    clean_state();

    // 2. Start a process via CLI args in fork mode
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
    thread::sleep(Duration::from_millis(1500));

    // 3. Verify it shows up in list
    let ls_output = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls");
    let ls_str = String::from_utf8_lossy(&ls_output.stdout);
    assert!(ls_str.contains("test-fork-basic"));
    assert!(ls_str.contains("online"));

    // 4. Stop the process
    let stop_output = Command::new("target/debug/rpm")
        .arg("stop")
        .arg("test-fork-basic")
        .output()
        .expect("Failed to stop process");
    assert!(stop_output.status.success());
    thread::sleep(Duration::from_millis(1000));

    // 5. Verify process status is stopped
    let ls_output2 = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls after stop");
    let ls_str2 = String::from_utf8_lossy(&ls_output2.stdout);
    assert!(ls_str2.contains("test-fork-basic"));
    assert!(ls_str2.contains("stopped"));

    // 6. Restart the process
    let restart_output = Command::new("target/debug/rpm")
        .arg("restart")
        .arg("test-fork-basic")
        .output()
        .expect("Failed to restart process");
    assert!(restart_output.status.success());
    thread::sleep(Duration::from_millis(1500));

    // Verify it is online again
    let ls_output3 = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls after restart");
    let ls_str3 = String::from_utf8_lossy(&ls_output3.stdout);
    assert!(ls_str3.contains("online"));

    // 7. Delete the process
    let delete_output = Command::new("target/debug/rpm")
        .arg("delete")
        .arg("test-fork-basic")
        .output()
        .expect("Failed to delete process");
    assert!(delete_output.status.success());
    thread::sleep(Duration::from_millis(500));

    // Verify it is gone from the list
    let ls_output4 = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls after delete");
    let ls_str4 = String::from_utf8_lossy(&ls_output4.stdout);
    assert!(!ls_str4.contains("test-fork-basic"));

    // Clean up
    let _ = Command::new("target/debug/rpm").arg("kill").output();
}

fn test_load_balancer_and_memory_restart() {
    // 1. Ensure clean state by killing daemon
    clean_state();

    // 2. Create the YAML config
    let yaml_content = r#"
apps:
  - name: "test-app"
    script: "tests/mock_server.py"
    interpreter: "python3"
    instances: 2
    mode: "cluster"
    port: 9876
    lb_strategy: "round-robin"
    max_memory: "50MB"
"#;
    let yaml_path = "tests/rpm_test.yaml";
    std::fs::write(yaml_path, yaml_content).expect("Failed to write test yaml");

    // 3. Start daemon and the application
    let start_output = Command::new("target/debug/rpm")
        .arg("start")
        .arg(yaml_path)
        .output()
        .expect("Failed to start app with config");

    println!(
        "Start output: {}",
        String::from_utf8_lossy(&start_output.stdout)
    );

    // Sleep to allow daemon to spawn workers and load balancer to bind
    thread::sleep(Duration::from_millis(2500));

    // 4. Connect 4 times to verify round robin load balancing
    let mut pids = Vec::new();
    for _ in 0..4 {
        let resp = send_request(9876, "hello").expect("Failed to connect to LB");
        println!("LB response: {}", resp.trim());
        let pid_part = resp.split("pid").nth(1).unwrap_or("").trim();
        let pid: u32 = pid_part.parse().expect("Failed to parse pid from response");
        pids.push(pid);
    }

    assert_eq!(pids.len(), 4);
    let mut unique_pids = pids.clone();
    unique_pids.sort();
    unique_pids.dedup();
    assert_eq!(
        unique_pids.len(),
        2,
        "Expected exactly 2 worker processes running"
    );

    // Verify round robin alternation
    assert_ne!(pids[0], pids[1], "PIDs should alternate");
    assert_eq!(pids[0], pids[2], "Round-robin expected");
    assert_eq!(pids[1], pids[3], "Round-robin expected");

    let pid_to_kill = pids[0];
    let pid_to_keep = pids[1];

    // 5. Test worker failover by killing pid_to_kill (Worker 1)
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .arg("-9")
            .arg(pid_to_kill.to_string())
            .status()
            .expect("Failed to kill worker");
        assert!(status.success());
    }

    // Sleep to let daemon detect exit and update process state
    thread::sleep(Duration::from_millis(3000));

    // Check list of processes to verify
    let ls_output = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run rpm ls");
    let ls_str = String::from_utf8_lossy(&ls_output.stdout);
    println!("Process list:\n{}", ls_str);

    // Verify that subsequent connections to load balancer are routed ONLY to the surviving worker
    for _ in 0..3 {
        let resp = send_request(9876, "hello").expect("Failed to connect to LB after failover");
        let pid_part = resp.split("pid").nth(1).unwrap_or("").trim();
        let pid: u32 = pid_part.parse().unwrap();
        assert_eq!(
            pid, pid_to_keep,
            "Should only route to the surviving worker"
        );
    }

    // 6. Test memory limit auto-restart
    let alloc_resp =
        send_request(9876, "allocate_memory").expect("Failed to send allocate memory command");
    println!("Alloc response: {}", alloc_resp.trim());

    // Sleep to let the daemon monitor check memory and restart it
    thread::sleep(Duration::from_millis(3500));

    // Verify that the worker has been restarted with a new PID
    let new_resp =
        send_request(9876, "hello").expect("Failed to connect to LB after memory restart");
    println!("New response after memory restart: {}", new_resp.trim());
    let new_pid_part = new_resp.split("pid").nth(1).unwrap_or("").trim();
    let new_pid: u32 = new_pid_part.parse().unwrap();

    assert_ne!(new_pid, pid_to_keep, "Should have a new PID after restart");

    // Check that restarts counter is incremented
    let ls_output2 = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run rpm ls");
    let ls_str2 = String::from_utf8_lossy(&ls_output2.stdout);
    println!("Process list after restart:\n{}", ls_str2);

    // Clean up
    let _ = Command::new("target/debug/rpm").arg("kill").output();
    let _ = std::fs::remove_file(yaml_path);
}

fn test_cli_least_loaded() {
    // 1. Clean daemon state
    clean_state();

    // 2. Start a cluster app with least-loaded strategy
    let start_output = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("test-ll")
        .arg("--mode")
        .arg("cluster")
        .arg("--instances")
        .arg("2")
        .arg("--port")
        .arg("9877")
        .arg("--lb-strategy")
        .arg("least-loaded")
        .arg("--max-memory")
        .arg("500MB")
        .output()
        .expect("Failed to start cluster app");

    assert!(start_output.status.success());
    thread::sleep(Duration::from_millis(3000));

    // 3. Connect to the LB to get the first worker (Worker A)
    let resp_a = send_request(9877, "hello").expect("Failed to connect to LB on 9877");
    let parts_a: Vec<&str> = resp_a.split_whitespace().collect();
    assert!(parts_a.len() >= 6);
    let port_a: u16 = parts_a[3].parse().expect("Failed to parse port");
    let pid_a: u32 = parts_a[5].parse().expect("Failed to parse pid");

    // 4. Send memory allocation command to Worker A directly
    let alloc_resp =
        send_request(port_a, "allocate_memory").expect("Failed to allocate memory on worker A");
    assert!(alloc_resp.contains(&pid_a.to_string()));

    // 5. Sleep to let the daemon monitor poll memory stats (interval is 2s)
    thread::sleep(Duration::from_millis(2500));

    // 6. Connect to the LB to get the second worker (Worker B).
    // Under least-loaded, since Worker A has >150MB, and Worker B has ~10MB,
    // the request must route to Worker B.
    let resp_b = send_request(9877, "hello").expect("Failed to connect to LB on 9877");
    let parts_b: Vec<&str> = resp_b.split_whitespace().collect();
    assert!(parts_b.len() >= 6);
    let pid_b: u32 = parts_b[5].parse().expect("Failed to parse pid");

    assert_ne!(pid_a, pid_b, "Expected 2 distinct worker instances");

    // Verify subsequent connections also route exclusively to the less loaded worker (Worker B)
    for _ in 0..3 {
        let resp = send_request(9877, "hello").expect("Failed to connect to LB");
        let parts: Vec<&str> = resp.split_whitespace().collect();
        let routed_pid: u32 = parts[5].trim().parse().expect("Failed to parse routed pid");
        assert_eq!(
            routed_pid, pid_b,
            "Should route exclusively to the less loaded worker"
        );
    }

    // Clean up
    let _ = Command::new("target/debug/rpm").arg("kill").output();
}

fn test_cluster_lifecycle() {
    // 1. Clean daemon state
    clean_state();

    // 2. Start a cluster app with CLI args
    let start_output = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("test-cluster-life")
        .arg("--mode")
        .arg("cluster")
        .arg("--instances")
        .arg("2")
        .arg("--port")
        .arg("9879")
        .output()
        .expect("Failed to start cluster app");

    assert!(start_output.status.success());
    thread::sleep(Duration::from_millis(3000));

    // 3. Verify both workers are listed
    let ls_output = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls");
    let ls_str = String::from_utf8_lossy(&ls_output.stdout);
    assert!(ls_str.contains("test-cluster-life-0"));
    assert!(ls_str.contains("test-cluster-life-1"));
    assert!(ls_str.contains("online"));

    // 4. Verify round robin connection to port 9879 works
    let mut pids = Vec::new();
    for _ in 0..4 {
        let resp = send_request(9879, "hello").expect("Failed to connect to LB");
        let pid_part = resp.split("pid").nth(1).unwrap_or("").trim();
        let pid: u32 = pid_part.parse().expect("Failed to parse pid");
        pids.push(pid);
    }
    assert_eq!(pids.len(), 4);
    assert_ne!(pids[0], pids[1], "Should alternate between workers");

    // 5. Restart the application by name (this should restart both workers)
    let restart_output = Command::new("target/debug/rpm")
        .arg("restart")
        .arg("test-cluster-life")
        .output()
        .expect("Failed to restart cluster");
    assert!(restart_output.status.success());
    thread::sleep(Duration::from_millis(3000));

    // Verify both workers are online and have restart count = 1
    let ls_output2 = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls after restart");
    let ls_str2 = String::from_utf8_lossy(&ls_output2.stdout);
    assert!(ls_str2.contains("test-cluster-life-0"));
    assert!(ls_str2.contains("test-cluster-life-1"));

    // 6. Stop the application (this should stop both workers and stop the LB)
    let stop_output = Command::new("target/debug/rpm")
        .arg("stop")
        .arg("test-cluster-life")
        .output()
        .expect("Failed to stop cluster");
    assert!(stop_output.status.success());
    thread::sleep(Duration::from_millis(1500));

    // Verify both are stopped
    let ls_output3 = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls after stop");
    let ls_str3 = String::from_utf8_lossy(&ls_output3.stdout);
    assert!(ls_str3.contains("stopped"));

    // Verify load balancer port 9879 is released and connections fail
    let conn_res = send_request(9879, "hello");
    assert!(
        conn_res.is_err(),
        "Expected connection to load balancer to fail after stop"
    );

    // 7. Delete the application
    let delete_output = Command::new("target/debug/rpm")
        .arg("delete")
        .arg("test-cluster-life")
        .output()
        .expect("Failed to delete cluster");
    assert!(delete_output.status.success());
    thread::sleep(Duration::from_millis(1000));

    // Verify workers are removed from list
    let ls_output4 = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls after delete");
    let ls_str4 = String::from_utf8_lossy(&ls_output4.stdout);
    assert!(!ls_str4.contains("test-cluster-life"));

    // Clean up
    let _ = Command::new("target/debug/rpm").arg("kill").output();
}

#[cfg(unix)]
fn test_attach_ctrlc() {
    // 1. Clean daemon state
    clean_state();

    // 2. Start with --attach as a subprocess in cluster mode to bind port 9880
    let mut child = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("test-attach-ctrlc")
        .arg("--mode")
        .arg("cluster")
        .arg("--port")
        .arg("9880")
        .arg("--attach")
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to spawn attach process");

    // 3. Read stdout until we see the "Mock server running" message
    let stdout = child.stdout.as_mut().expect("No stdout handle");
    let mut reader = std::io::BufReader::new(stdout);
    let mut found = false;
    let mut line = String::new();

    use std::io::BufRead;
    for _ in 0..100 {
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
        thread::sleep(Duration::from_millis(50));
    }

    assert!(
        found,
        "Did not find expected mock server start output in stdout"
    );

    // 4. Send SIGINT (2) to the child (simulating CTRL-C)
    let pid = child.id();
    let status = Command::new("kill")
        .arg("-2")
        .arg(pid.to_string())
        .status()
        .expect("Failed to send SIGINT to child");
    assert!(status.success());

    // Wait for the client process to exit
    let wait_res = child.wait().expect("Failed to wait on child");
    println!("Attach client exited with: {:?}", wait_res);

    // 5. Verify the background process is still online
    thread::sleep(Duration::from_millis(1500));
    let ls_output = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls");
    let ls_str = String::from_utf8_lossy(&ls_output.stdout);
    assert!(ls_str.contains("test-attach-ctrlc"));
    assert!(ls_str.contains("online"));

    // 6. Connect to port 9880 to verify it's still running and responding
    let resp =
        send_request(9880, "hello").expect("Failed to connect to background server after detach");
    assert!(resp.contains("pid"));

    // 7. Clean up
    let _ = Command::new("target/debug/rpm")
        .arg("delete")
        .arg("test-attach-ctrlc")
        .output();
    let _ = Command::new("target/debug/rpm").arg("kill").output();
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

fn test_cli_id_reset_and_reuse() {
    // 1. Clean daemon state
    clean_state();

    let delete_all_output = Command::new("target/debug/rpm")
        .arg("delete")
        .arg("all")
        .output()
        .expect("Failed to delete all processes");
    assert!(delete_all_output.status.success());
    thread::sleep(Duration::from_millis(500));

    // 2. Start proc-a
    let start_output = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("proc-a")
        .output()
        .expect("Failed to start proc-a");
    assert!(start_output.status.success());
    thread::sleep(Duration::from_millis(1500));

    // Verify it gets ID 0
    let ls_output = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls");
    let ls_str = String::from_utf8_lossy(&ls_output.stdout);
    let entries = parse_ls_output(&ls_str);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "proc-a");
    assert_eq!(entries[0].id, 0);
    assert!(entries[0].status.contains("online"));

    // 3. Try starting proc-a again without --force (should error)
    let start_err_output = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("proc-a")
        .output()
        .expect("Failed to try to start proc-a again");
    let err_str = String::from_utf8_lossy(&start_err_output.stderr);
    let out_str = String::from_utf8_lossy(&start_err_output.stdout);
    assert!(
        err_str.contains("already running")
            || out_str.contains("already running")
            || err_str.contains("already exists")
    );

    // 4. Start proc-a again with --force (should keep ID 0)
    let force_output = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("proc-a")
        .arg("--force")
        .output()
        .expect("Failed to start proc-a with --force");
    assert!(force_output.status.success());
    thread::sleep(Duration::from_millis(1500));

    let ls_output2 = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls");
    let ls_str2 = String::from_utf8_lossy(&ls_output2.stdout);
    let entries2 = parse_ls_output(&ls_str2);
    assert_eq!(entries2.len(), 1);
    assert_eq!(entries2[0].name, "proc-a");
    assert_eq!(entries2[0].id, 0);
    assert!(entries2[0].status.contains("online"));

    // 5. Stop proc-a
    let stop_output = Command::new("target/debug/rpm")
        .arg("stop")
        .arg("proc-a")
        .output()
        .expect("Failed to stop proc-a");
    assert!(stop_output.status.success());
    thread::sleep(Duration::from_millis(1000));

    // 6. Start proc-b (should get ID 1 since proc-a is still registered as stopped)
    let start_b_output = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("proc-b")
        .output()
        .expect("Failed to start proc-b");
    assert!(start_b_output.status.success());
    thread::sleep(Duration::from_millis(1500));

    let ls_output3 = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls");
    let ls_str3 = String::from_utf8_lossy(&ls_output3.stdout);
    let entries3 = parse_ls_output(&ls_str3);
    assert_eq!(entries3.len(), 2);
    let a_entry = entries3.iter().find(|e| e.name == "proc-a").unwrap();
    let b_entry = entries3.iter().find(|e| e.name == "proc-b").unwrap();
    assert_eq!(a_entry.id, 0);
    assert!(a_entry.status.contains("stopped"));
    assert_eq!(b_entry.id, 1);
    assert!(b_entry.status.contains("online"));

    // 7. Start proc-a again (which is stopped) without --force. This should start it and keep ID 0.
    let start_a_again = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("proc-a")
        .output()
        .expect("Failed to start stopped proc-a");
    assert!(start_a_again.status.success());
    thread::sleep(Duration::from_millis(1500));

    let ls_output4 = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls");
    let ls_str4 = String::from_utf8_lossy(&ls_output4.stdout);
    let entries4 = parse_ls_output(&ls_str4);
    assert_eq!(entries4.len(), 2);
    let a_entry_new = entries4.iter().find(|e| e.name == "proc-a").unwrap();
    let b_entry_new = entries4.iter().find(|e| e.name == "proc-b").unwrap();
    assert_eq!(a_entry_new.id, 0);
    assert!(a_entry_new.status.contains("online"));
    assert_eq!(b_entry_new.id, 1);
    assert!(b_entry_new.status.contains("online"));

    // 8. Delete proc-a
    let del_a_output = Command::new("target/debug/rpm")
        .arg("delete")
        .arg("proc-a")
        .output()
        .expect("Failed to delete proc-a");
    assert!(del_a_output.status.success());
    thread::sleep(Duration::from_millis(500));

    let ls_output5 = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls");
    let ls_str5 = String::from_utf8_lossy(&ls_output5.stdout);
    let entries5 = parse_ls_output(&ls_str5);
    assert_eq!(entries5.len(), 1);
    assert_eq!(entries5[0].name, "proc-b");
    assert_eq!(entries5[0].id, 1);

    // 9. Delete proc-b (the last remaining process, which should reset the next ID to 0)
    let del_b_output = Command::new("target/debug/rpm")
        .arg("delete")
        .arg("proc-b")
        .output()
        .expect("Failed to delete proc-b");
    assert!(del_b_output.status.success());
    thread::sleep(Duration::from_millis(500));

    let ls_output6 = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls");
    let ls_str6 = String::from_utf8_lossy(&ls_output6.stdout);
    assert!(ls_str6.contains("no processes running"));

    // 10. Start a new process (should get ID 0 again)
    let start_c_output = Command::new("target/debug/rpm")
        .arg("start")
        .arg("python3")
        .arg("--")
        .arg("tests/mock_server.py")
        .arg("--name")
        .arg("proc-c")
        .output()
        .expect("Failed to start proc-c");
    assert!(start_c_output.status.success());
    thread::sleep(Duration::from_millis(1500));

    let ls_output7 = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls");
    let ls_str7 = String::from_utf8_lossy(&ls_output7.stdout);
    let entries7 = parse_ls_output(&ls_str7);
    assert_eq!(entries7.len(), 1);
    assert_eq!(entries7[0].name, "proc-c");
    assert_eq!(entries7[0].id, 0);

    // Clean up
    let _ = Command::new("target/debug/rpm").arg("kill").output();
    thread::sleep(Duration::from_millis(500));
}

fn test_cli_delete_all_reset() {
    // 1. Clean daemon state
    clean_state();

    // 2. Start two processes
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
    thread::sleep(Duration::from_millis(1500));

    // 3. Delete all
    let delete_all = Command::new("target/debug/rpm")
        .arg("delete")
        .arg("all")
        .output()
        .expect("Failed to delete all");
    assert!(delete_all.status.success());
    thread::sleep(Duration::from_millis(500));

    // 4. Start a new process (should get ID 0 again)
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
    thread::sleep(Duration::from_millis(1500));

    // 5. Verify proc-c has ID 0
    let ls_output = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls");
    let ls_str = String::from_utf8_lossy(&ls_output.stdout);
    let entries = parse_ls_output(&ls_str);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "proc-c");
    assert_eq!(entries[0].id, 0);

    // Clean up
    let _ = Command::new("target/debug/rpm").arg("kill").output();
    thread::sleep(Duration::from_millis(500));
}

fn test_cli_restart_all() {
    // 1. Clean daemon state
    clean_state();

    // 2. Start two processes
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
    thread::sleep(Duration::from_millis(1500));

    // Get current PIDs
    let ls_output = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls");
    let ls_str = String::from_utf8_lossy(&ls_output.stdout);
    let entries = parse_ls_output(&ls_str);
    assert_eq!(entries.len(), 2);
    let pid_a = entries.iter().find(|e| e.name == "proc-a").unwrap().pid;
    let pid_b = entries.iter().find(|e| e.name == "proc-b").unwrap().pid;
    assert!(pid_a.is_some());
    assert!(pid_b.is_some());

    // 3. Restart all
    let restart_all = Command::new("target/debug/rpm")
        .arg("restart")
        .arg("all")
        .output()
        .expect("Failed to restart all");
    assert!(restart_all.status.success());
    thread::sleep(Duration::from_millis(1500));

    // 4. Verify both restarted (new PIDs, restart count increased)
    let ls_output2 = Command::new("target/debug/rpm")
        .arg("ls")
        .output()
        .expect("Failed to run ls");
    let ls_str2 = String::from_utf8_lossy(&ls_output2.stdout);
    let entries2 = parse_ls_output(&ls_str2);
    assert_eq!(entries2.len(), 2);
    let a_new = entries2.iter().find(|e| e.name == "proc-a").unwrap();
    let b_new = entries2.iter().find(|e| e.name == "proc-b").unwrap();

    assert!(a_new.pid.is_some());
    assert!(b_new.pid.is_some());
    assert_ne!(a_new.pid, pid_a);
    assert_ne!(b_new.pid, pid_b);
    assert_eq!(a_new.restarts, 1);
    assert_eq!(b_new.restarts, 1);

    // Clean up
    let _ = Command::new("target/debug/rpm").arg("kill").output();
    thread::sleep(Duration::from_millis(500));
}

#[test]
fn run_all_integration_tests() {
    test_cli_basic_flow();
    test_load_balancer_and_memory_restart();
    test_cli_least_loaded();
    test_cluster_lifecycle();
    #[cfg(unix)]
    test_attach_ctrlc();
    test_cli_id_reset_and_reuse();
    test_cli_delete_all_reset();
    test_cli_restart_all();
}
