//! Windows setup: an autostart value under the user's `Run` key. The Windows key and taskbar
//! hooks need no setup; the trigger reports their state while the host runs.

use std::env;
use std::iter;

use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SAM_FLAGS, REG_SZ, RRF_RT_REG_SZ, RegCloseKey,
    RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
};
use windows::core::{HSTRING, PCWSTR, w};

use super::run_key::{self, VALUE_NAME};
use super::{ItemState, MetaKeyState, Options, SetupItem};

/// Windows keeps the Windows key in `use_win_key`, which the Settings Page changes directly.
pub const OWNS_META_KEY: bool = false;

const RUN_KEY_PATH: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");

pub fn apply(_options: Options, changes: &mut Vec<String>) -> Result<()> {
    let expected: String = expected_command()?;
    let key = RunKey::open(KEY_QUERY_VALUE | KEY_SET_VALUE)?;
    if run_key::value_state(key.read()?.as_deref(), &expected).0 == ItemState::Installed {
        changes.push("Autostart: already installed".to_owned());
        return Ok(());
    }
    changes.push(format!("Autostart: start Rusty Start at sign-in (HKCU Run value {VALUE_NAME})"));
    key.write(&expected)
}

pub fn undo(_options: Options, changes: &mut Vec<String>) -> Result<()> {
    let key = RunKey::open(KEY_QUERY_VALUE | KEY_SET_VALUE)?;
    if key.read()?.is_some() {
        changes.push(format!("Autostart: removed (HKCU Run value {VALUE_NAME})"));
        key.delete()?;
    }
    Ok(())
}

pub fn audit() -> Result<Vec<SetupItem>> {
    let expected: String = expected_command()?;
    let existing: Option<String> = RunKey::open(KEY_QUERY_VALUE)?.read()?;
    let (state, detail) = run_key::value_state(existing.as_deref(), &expected);
    Ok(vec![SetupItem::new("Autostart", state, detail)])
}

/// Never called: Windows keeps the Windows key in `use_win_key` (see `OWNS_META_KEY`).
pub fn meta_key() -> Result<MetaKeyState> {
    bail!("Windows keeps the Windows key in config.toml")
}

/// Never called: Windows keeps the Windows key in `use_win_key` (see `OWNS_META_KEY`).
pub fn set_meta_key(_enabled: bool, _changes: &mut Vec<String>) -> Result<()> {
    bail!("Windows keeps the Windows key in config.toml")
}

fn expected_command() -> Result<String> {
    Ok(run_key::command(&env::current_exe().context("failed to find the running executable")?))
}

/// The user's open `Run` key, closed on drop.
struct RunKey(HKEY);

impl RunKey {
    fn open(access: REG_SAM_FLAGS) -> Result<Self> {
        let mut key = HKEY::default();
        // SAFETY: `RUN_KEY_PATH` is a static NUL-terminated string and `key` is a valid out-parameter.
        unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY_PATH, None, access, &raw mut key) }
            .ok()
            .context("failed to open the Run registry key")?;
        Ok(Self(key))
    }

    fn read(&self) -> Result<Option<String>> {
        let name = HSTRING::from(VALUE_NAME);
        let mut size: u32 = 0;
        // SAFETY: `self.0` is an open key. A missing data pointer with a size pointer only queries
        // the size in bytes.
        let status =
            unsafe { RegGetValueW(self.0, PCWSTR::null(), &name, RRF_RT_REG_SZ, None, None, Some(&raw mut size)) };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        status.ok().context("failed to read the autostart value")?;

        let mut buffer: Vec<u16> = vec![0; (size as usize).div_ceil(2)];
        // SAFETY: `buffer` holds at least `size` bytes, the size RegGetValueW reported above.
        unsafe {
            RegGetValueW(
                self.0,
                PCWSTR::null(),
                &name,
                RRF_RT_REG_SZ,
                None,
                Some(buffer.as_mut_ptr().cast()),
                Some(&raw mut size),
            )
        }
        .ok()
        .context("failed to read the autostart value")?;
        let length: usize = buffer.iter().position(|&unit| unit == 0).unwrap_or(buffer.len());
        Ok(Some(String::from_utf16_lossy(&buffer[..length])))
    }

    fn write(&self, command: &str) -> Result<()> {
        let data: Vec<u8> = command.encode_utf16().chain(iter::once(0)).flat_map(u16::to_le_bytes).collect();
        // SAFETY: `self.0` is open with `KEY_SET_VALUE` and `data` is a NUL-terminated UTF-16 string.
        unsafe { RegSetValueExW(self.0, &HSTRING::from(VALUE_NAME), None, REG_SZ, Some(&data)) }
            .ok()
            .context("failed to write the autostart value")
    }

    fn delete(&self) -> Result<()> {
        // SAFETY: `self.0` is open with `KEY_SET_VALUE`.
        let status = unsafe { RegDeleteValueW(self.0, &HSTRING::from(VALUE_NAME)) };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        status.ok().context("failed to delete the autostart value")
    }
}

impl Drop for RunKey {
    fn drop(&mut self) {
        // SAFETY: `self.0` was opened by `RunKey::open` and is closed only here.
        let _ = unsafe { RegCloseKey(self.0) };
    }
}
