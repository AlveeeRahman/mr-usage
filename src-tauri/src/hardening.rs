//! Process hardening applied before any thread starts.

/// On Linux, set the core-file limit to zero and mark the process
/// non-dumpable, so a crash cannot write keys to disk and non-root processes
/// cannot `ptrace` attach. Other platforms do not write core files by default.
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
}

#[cfg(not(target_os = "linux"))]
pub fn apply() {}
