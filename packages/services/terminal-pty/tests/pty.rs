use futures_util::StreamExt;
use moonkale_terminal::TerminalBackend;
use moonkale_terminal_pty::PtyBackend;
use std::time::Duration;

#[tokio::test]
async fn echo_round_trip_through_a_shell() {
    let mut pty = PtyBackend::spawn(None, Some("/bin/sh"), 80, 24).expect("spawn sh");
    let mut out = pty.take_output().unwrap();
    pty.write(b"echo moon_kale_$((20+3))\n");
    let mut seen = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !seen.contains("moon_kale_23") && tokio::time::Instant::now() < deadline {
        if let Ok(Some(chunk)) = tokio::time::timeout(Duration::from_millis(500), out.next()).await
        {
            seen.push_str(&String::from_utf8_lossy(&chunk))
        }
    }
    assert!(seen.contains("moon_kale_23"), "output was: {seen:?}");
    assert!(pty.is_running());
    pty.resize(120, 40);
    pty.write(b"exit\n");
}

/// Use the kernel's line discipline, never text matching on a prompt.
#[cfg(unix)]
#[tokio::test]
async fn canonical_echo_off_is_private_but_raw_mode_is_not() {
    let pty = PtyBackend::spawn_args(
        None,
        Some("/bin/sh"),
        &[
            "-c".into(),
            "stty -echo; read password; stty raw -echo; sleep 2".into(),
        ],
        80,
        24,
    )
    .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !pty.private_input() && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(pty.private_input());
    pty.write(b"never_share_this\n");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    while pty.private_input() && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        !pty.private_input(),
        "raw mode is the documented detection limit"
    );
}
