//! Windows session and power calls. Restart, shut down and sleep need `SeShutdownPrivilege`,
//! which standard accounts hold but have disabled, so it is enabled on the process token first.

use std::thread;

use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::{ERROR_NOT_ALL_ASSIGNED, GetLastError, HANDLE, LUID};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED, SE_SHUTDOWN_NAME,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Power::SetSuspendState;
use windows::Win32::System::Shutdown::{
    EWX_LOGOFF, EWX_POWEROFF, EWX_REBOOT, EWX_SHUTDOWN, EXIT_WINDOWS_FLAGS, ExitWindowsEx, LockWorkStation,
    SHTDN_REASON_FLAG_PLANNED, SHTDN_REASON_MAJOR_OTHER, SHTDN_REASON_MINOR_OTHER,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{Owned, PCWSTR};

use super::PowerAction;

/// Runs `action` on a short-lived thread, because suspending blocks until the computer wakes.
pub fn perform(action: PowerAction) {
    let spawned = thread::Builder::new().name("rusty-start-power".into()).spawn(move || {
        if let Err(error) = run(action) {
            eprintln!("failed to {}: {error:#}", action.label().to_lowercase());
        }
    });

    if let Err(error) = spawned {
        eprintln!("failed to spawn the power action thread: {error}");
    }
}

fn run(action: PowerAction) -> Result<()> {
    match action {
        // SAFETY: LockWorkStation has no preconditions.
        PowerAction::Lock => unsafe { LockWorkStation() }.context("failed to lock the workstation"),
        PowerAction::SignOut => exit_windows(EWX_LOGOFF),
        PowerAction::Sleep => {
            enable_shutdown_privilege()?;
            // SAFETY: SetSuspendState has no memory-safety preconditions.
            if !unsafe { SetSuspendState(false, false, false) } {
                return Err(windows::core::Error::from_thread()).context("failed to suspend the computer");
            }
            Ok(())
        }
        PowerAction::Restart => {
            enable_shutdown_privilege()?;
            exit_windows(EWX_REBOOT)
        }
        PowerAction::ShutDown => {
            enable_shutdown_privilege()?;
            exit_windows(EWX_SHUTDOWN | EWX_POWEROFF)
        }
    }
}

fn exit_windows(flags: EXIT_WINDOWS_FLAGS) -> Result<()> {
    let reason = SHTDN_REASON_MAJOR_OTHER | SHTDN_REASON_MINOR_OTHER | SHTDN_REASON_FLAG_PLANNED;
    // SAFETY: ExitWindowsEx has no memory-safety preconditions. It only starts the shutdown.
    unsafe { ExitWindowsEx(flags, reason) }.context("failed to end the session")
}

fn enable_shutdown_privilege() -> Result<()> {
    let mut raw_token = HANDLE::default();
    // SAFETY: GetCurrentProcess returns a pseudo-handle, and `raw_token` receives a new handle.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &raw mut raw_token) }
        .context("failed to open the process token")?;
    // SAFETY: OpenProcessToken succeeded, so `raw_token` is a handle this function owns.
    let token: Owned<HANDLE> = unsafe { Owned::new(raw_token) };

    let mut luid = LUID::default();
    // SAFETY: SE_SHUTDOWN_NAME is a static wide string, and `luid` is valid for writes.
    unsafe { LookupPrivilegeValueW(PCWSTR::null(), SE_SHUTDOWN_NAME, &raw mut luid) }
        .context("failed to look up the shutdown privilege")?;

    let privileges = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
    };
    // SAFETY: `token` was opened with TOKEN_ADJUST_PRIVILEGES, and `privileges` holds one entry.
    unsafe { AdjustTokenPrivileges(*token, false, Some(&raw const privileges), 0, None, None) }
        .context("failed to enable the shutdown privilege")?;

    // AdjustTokenPrivileges reports success without enabling anything when the token lacks it.
    // SAFETY: GetLastError has no preconditions and runs right after AdjustTokenPrivileges.
    if unsafe { GetLastError() } == ERROR_NOT_ALL_ASSIGNED {
        bail!("this account is not allowed to shut down the computer");
    }

    Ok(())
}
