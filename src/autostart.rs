//! "Start on Login": launches Catsor when the user logs in.
//!
//! Linux uses an XDG autostart entry (`$XDG_CONFIG_HOME/autostart/catsor.desktop`); Windows uses the
//! per-user `Run` registry key. The OS location is the only source of truth, so toggling it
//! from Task Manager or by deleting the file is reflected in the tray menu on the next launch.

use std::io;

/// Whether Catsor is currently registered to start on login
pub fn is_enabled() -> bool {
    imp::is_enabled()
}

/// Registers (pointing at the running executable) or unregisters Catsor as a login item
pub fn set_enabled(enabled: bool) -> io::Result<()> {
    imp::set_enabled(enabled)
}

#[cfg(not(any(target_os = "windows", windows_check)))]
mod imp {
    use std::ffi::OsStr;
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};

    pub fn is_enabled() -> bool {
        entry_path().is_some_and(|p| p.is_file())
    }

    pub fn set_enabled(enabled: bool) -> io::Result<()> {
        let path = entry_path().ok_or_else(|| io::Error::other("no config directory"))?;
        if enabled {
            let exe = std::env::current_exe()?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, desktop_entry(&exe))
        } else {
            match fs::remove_file(&path) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                r => r,
            }
        }
    }

    fn entry_path() -> Option<PathBuf> {
        resolve_entry_path(
            std::env::var_os("XDG_CONFIG_HOME").as_deref(),
            std::env::var_os("HOME").as_deref(),
        )
    }

    pub(super) fn resolve_entry_path(
        xdg_config_home: Option<&OsStr>,
        home: Option<&OsStr>,
    ) -> Option<PathBuf> {
        let config = match xdg_config_home.map(PathBuf::from) {
            Some(xdg) if xdg.is_absolute() => xdg,
            _ => PathBuf::from(home.filter(|h| !h.is_empty())?).join(".config"),
        };
        Some(config.join("autostart").join("catsor.desktop"))
    }

    pub(super) fn desktop_entry(exe: &Path) -> String {
        format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Catsor\n\
             Comment=Desktop cat that follows your mouse cursor\n\
             Exec={}\n\
             Icon=catsor\n\
             Terminal=false\n\
             X-GNOME-Autostart-enabled=true\n",
            exec_arg(&exe.to_string_lossy())
        )
    }

    /// Quotes a path for `Exec=` per the Desktop Entry spec: reserved characters force quoting,
    /// `"`, `` ` ``, `$`, `\` are backslash-escaped inside quotes, then the value-level escape
    /// doubles every backslash and `%` becomes `%%` (field codes).
    pub(super) fn exec_arg(path: &str) -> String {
        const RESERVED: &str = " \t\n\"'\\><~|&;$*?#()`";
        let arg = if path.chars().any(|c| RESERVED.contains(c)) {
            let mut quoted = String::from("\"");
            for c in path.chars() {
                if matches!(c, '"' | '`' | '$' | '\\') {
                    quoted.push('\\');
                }
                quoted.push(c);
            }
            quoted.push('"');
            quoted
        } else {
            path.to_owned()
        };
        arg.replace('\\', "\\\\").replace('%', "%%")
    }
}

#[cfg(any(target_os = "windows", windows_check))]
mod imp {
    use crate::win32::to_wide_null;
    use core::ptr::null_mut;
    use std::io;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegDeleteKeyValueW,
        RegGetValueW, RegSetKeyValueW,
    };

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    /// Where Task Manager's Startup tab records "Disabled" without touching the `Run` value
    const APPROVED_KEY: &str =
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
    const VALUE_NAME: &str = "catsor";

    pub fn is_enabled() -> bool {
        let value = to_wide_null(VALUE_NAME);
        let run_key = to_wide_null(RUN_KEY);
        let approved_key = to_wide_null(APPROVED_KEY);
        unsafe {
            let registered = RegGetValueW(
                HKEY_CURRENT_USER,
                run_key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                null_mut(),
                null_mut(),
                null_mut(),
            ) == ERROR_SUCCESS;
            if !registered {
                return false;
            }
            // First byte is 0x02 (or even) when enabled, 0x03 (odd) when disabled
            let mut buf = [0u8; 12];
            let mut len = buf.len() as u32;
            let disabled = RegGetValueW(
                HKEY_CURRENT_USER,
                approved_key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_BINARY,
                null_mut(),
                buf.as_mut_ptr().cast(),
                &mut len,
            ) == ERROR_SUCCESS
                && len > 0
                && buf[0] & 1 == 1;
            !disabled
        }
    }

    pub fn set_enabled(enabled: bool) -> io::Result<()> {
        let value = to_wide_null(VALUE_NAME);
        let run_key = to_wide_null(RUN_KEY);
        let approved_key = to_wide_null(APPROVED_KEY);
        unsafe {
            // Clear any Task Manager "Disabled" flag so enabling from the tray actually takes effect
            RegDeleteKeyValueW(HKEY_CURRENT_USER, approved_key.as_ptr(), value.as_ptr());
            let status = if enabled {
                let exe = std::env::current_exe()?;
                let command = to_wide_null(&format!("\"{}\"", exe.display()));
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    run_key.as_ptr(),
                    value.as_ptr(),
                    REG_SZ,
                    command.as_ptr().cast(),
                    (command.len() * 2) as u32,
                )
            } else {
                match RegDeleteKeyValueW(HKEY_CURRENT_USER, run_key.as_ptr(), value.as_ptr()) {
                    ERROR_FILE_NOT_FOUND => ERROR_SUCCESS,
                    s => s,
                }
            };
            if status == ERROR_SUCCESS {
                Ok(())
            } else {
                Err(io::Error::from_raw_os_error(status as i32))
            }
        }
    }
}

#[cfg(all(test, not(any(target_os = "windows", windows_check))))]
mod tests {
    use super::imp::*;
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};

    #[test]
    fn entry_path_prefers_absolute_xdg_and_falls_back_to_home() {
        let home = Some(OsStr::new("/home/u"));
        assert_eq!(
            resolve_entry_path(Some(OsStr::new("/cfg")), home),
            Some(PathBuf::from("/cfg/autostart/catsor.desktop"))
        );
        for xdg in [None, Some(OsStr::new("")), Some(OsStr::new("rel/cfg"))] {
            assert_eq!(
                resolve_entry_path(xdg, home),
                Some(PathBuf::from("/home/u/.config/autostart/catsor.desktop"))
            );
        }
        assert_eq!(resolve_entry_path(None, None), None);
    }

    #[test]
    fn exec_quotes_and_escapes_per_desktop_entry_spec() {
        assert_eq!(exec_arg("/usr/bin/catsor"), "/usr/bin/catsor");
        assert_eq!(exec_arg("/opt/my apps/catsor"), "\"/opt/my apps/catsor\"");
        // `$` gets an arg-level backslash, which the value-level escape then doubles
        assert_eq!(exec_arg("/a $b/catsor"), "\"/a \\\\$b/catsor\"");
        assert_eq!(exec_arg("/100%/catsor"), "/100%%/catsor");
        let entry = desktop_entry(Path::new("/usr/bin/catsor"));
        assert!(entry.starts_with("[Desktop Entry]\n"));
        assert!(entry.contains("\nExec=/usr/bin/catsor\n"));
    }
}
