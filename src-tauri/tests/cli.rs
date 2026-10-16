//! The command-line contract of the built binary: streams and exit status
//! for the paths that never open a window. `--forget-keys` is deliberately
//! not run here, because on a developer's machine it would delete their real
//! keys; `install.sh --uninstall` and the release workflow exercise it.

#![allow(clippy::expect_used, clippy::panic)]

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct Outcome {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Run the binary with `args`, bounded by `WAIT`. A child that is still
/// running at the deadline is killed and reaped, so a regression that opens
/// the window cannot hang the suite or leave a process behind.
fn run(args: &[&str]) -> Outcome {
    const WAIT: Duration = Duration::from_secs(30);
    let mut child = Command::new(env!("CARGO_BIN_EXE_mr-usage"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mr-usage");
    // Both pipes are drained on threads so neither can fill and block the child.
    let mut out = child.stdout.take().expect("stdout pipe");
    let mut err = child.stderr.take().expect("stderr pipe");
    let out_thread = std::thread::spawn(move || {
        let mut s = String::new();
        out.read_to_string(&mut s).expect("read stdout");
        s
    });
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        err.read_to_string(&mut s).expect("read stderr");
        s
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            break status;
        }
        if started.elapsed() > WAIT {
            child.kill().expect("kill");
            let status = child.wait().expect("wait");
            panic!("mr-usage {args:?} did not exit within {WAIT:?} (killed, status {status})");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    Outcome {
        code: status.code(),
        stdout: out_thread.join().expect("stdout thread"),
        stderr: err_thread.join().expect("stderr thread"),
    }
}

#[test]
fn help_goes_to_stdout_and_exits_zero() {
    let o = run(&["--help"]);
    assert_eq!(o.code, Some(0), "stderr: {}", o.stderr);
    assert!(
        o.stdout.starts_with(&format!("mr-usage {}\n", env!("CARGO_PKG_VERSION"))),
        "{}",
        o.stdout
    );
    assert!(o.stdout.contains("--forget-keys"), "{}", o.stdout);
    assert_eq!(o.stderr, "");
}

#[test]
fn version_is_one_line_on_stdout() {
    let o = run(&["--version"]);
    assert_eq!(o.code, Some(0), "stderr: {}", o.stderr);
    assert_eq!(o.stdout, format!("mr-usage {}\n", env!("CARGO_PKG_VERSION")));
    assert_eq!(o.stderr, "");
}

#[test]
fn an_unknown_option_is_a_usage_error_on_stderr() {
    let o = run(&["--bogus"]);
    assert_eq!(o.code, Some(2), "stdout: {} stderr: {}", o.stdout, o.stderr);
    assert_eq!(o.stdout, "");
    assert!(o.stderr.contains("unknown option: --bogus"), "{}", o.stderr);
    assert!(o.stderr.contains("Usage:"), "{}", o.stderr);
}

#[test]
fn a_positional_argument_is_a_usage_error() {
    let o = run(&["--", "settings.json"]);
    assert_eq!(o.code, Some(2), "stdout: {} stderr: {}", o.stdout, o.stderr);
    assert!(o.stderr.contains("unexpected argument: settings.json"), "{}", o.stderr);
}
