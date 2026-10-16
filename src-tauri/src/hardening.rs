//! Process hardening applied before any thread starts.

/// On Linux, set the core-file limit to zero and mark the process
/// non-dumpable, so a crash cannot write keys to disk and processes without
/// `CAP_SYS_PTRACE` cannot `ptrace` attach (see PR_SET_DUMPABLE(2const),
/// https://man7.org/linux/man-pages/man2/PR_SET_DUMPABLE.2const.html, read
/// 2026-10-16). macOS writes no core files unless the core limit is raised.
/// Windows Error Reporting can still capture dumps (docs/FOLLOWUPS.md).
/// TODO(verify): source URLs for the macOS and Windows statements.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
pub fn apply() {
    let zero = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY: both calls take plain values and a pointer to a stack value that
    // outlives the call. Neither touches Rust-managed memory.
    let (rlimit_rc, prctl_rc) = unsafe {
        (
            libc::setrlimit(libc::RLIMIT_CORE, &zero),
            libc::prctl(
                libc::PR_SET_DUMPABLE,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
            ),
        )
    };
    if rlimit_rc != 0 || prctl_rc != 0 {
        tracing::warn!(rlimit_rc, prctl_rc, "could not disable core dumps");
    }
    follow_appimage_runtime();
}

/// When an AppImage runs in extract-and-run mode (no FUSE), the AppImage
/// runtime stays alive as this process's parent and does not forward signals.
/// `kill <runtime pid>` would then orphan the app. Ask the kernel to send us
/// SIGTERM when that parent dies, so the normal exit path runs. Only applied
/// in that mode: anywhere else the parent is the user's shell or desktop, and
/// tying our lifetime to it would be wrong.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
fn follow_appimage_runtime() {
    let extracted = std::env::var_os("APPDIR").is_some_and(|dir| dir.to_string_lossy().contains("/appimage_extracted_"));
    if !extracted {
        return;
    }
    // SAFETY: getppid and prctl take plain values and touch no Rust-managed memory.
    let (parent_before, rc, parent_after) = unsafe {
        let before = libc::getppid();
        let rc = libc::prctl(
            libc::PR_SET_PDEATHSIG,
            libc::SIGTERM as libc::c_ulong,
            0 as libc::c_ulong,
            0 as libc::c_ulong,
            0 as libc::c_ulong,
        );
        (before, rc, libc::getppid())
    };
    if rc != 0 {
        tracing::warn!(rc, "could not follow the AppImage runtime's lifetime");
    } else if parent_before != parent_after {
        tracing::warn!("the AppImage runtime exited during startup");
    }
}

#[cfg(not(target_os = "linux"))]
pub fn apply() {}
