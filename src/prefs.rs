use crate::cat::{BehaviorConfig, CatSim, CoatStyle, Temperament};
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UserPrefs {
    pub coat: CoatStyle,
    pub scale: f32,
    pub behaviors: BehaviorConfig,
}

impl Default for UserPrefs {
    fn default() -> Self {
        Self {
            coat: CoatStyle::Calico,
            scale: 1.0,
            behaviors: BehaviorConfig::default(),
        }
    }
}

impl UserPrefs {
    pub fn from_cat(cat: &CatSim) -> Self {
        Self {
            coat: cat.coat,
            scale: cat.scale.clamp(0.5, 2.5),
            behaviors: cat.behaviors,
        }
    }

    pub fn load() -> Self {
        let Some(path) = default_config_path() else {
            return Self::default();
        };
        Self::load_from_path(&path)
    }

    pub fn load_from_path(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(contents) => Self::parse_toml(&contents),
            Err(_) => Self::default(),
        }
    }

    pub fn save(self) {
        if let Some(path) = default_config_path() {
            let _ = self.save_to_path(&path);
        }
    }

    pub fn save_to_path(self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        let content = self.to_toml_string();
        let pid = std::process::id();
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config.toml");

        let (mut file, tmp_path) = loop {
            let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let candidate = path.with_file_name(format!(".{file_name}.{pid}.{seq}.tmp"));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(f) => break (f, candidate),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        };

        if let Err(err) = file
            .write_all(content.as_bytes())
            .and_then(|()| file.sync_all())
        {
            drop(file);
            let _ = fs::remove_file(&tmp_path);
            return Err(err);
        }
        drop(file);

        if let Err(err) = fs::rename(&tmp_path, path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(err);
        }
        Ok(())
    }

    pub fn to_toml_string(self) -> String {
        format!(
            "# Catsor user preferences\n\
             coat = \"{}\"\n\
             temperament = \"{}\"\n\
             scale = {:.2}\n\
             follow_cursor = {}\n\
             playful_quirks = {}\n",
            self.coat.as_config_str(),
            self.behaviors.temperament.as_config_str(),
            self.scale.clamp(0.5, 2.5),
            self.behaviors.follow_cursor,
            self.behaviors.playful_quirks,
        )
    }

    pub fn parse_toml(input: &str) -> Self {
        let mut prefs = Self::default();
        for raw_line in input.lines() {
            let line = raw_line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((raw_key, raw_val)) = line.split_once('=') else {
                continue;
            };
            let key = raw_key.trim();
            let val = raw_val.trim().trim_matches('"').trim_matches('\'').trim();

            match key {
                "coat" => {
                    if let Some(c) = CoatStyle::from_config_str(val) {
                        prefs.coat = c;
                    }
                }
                "temperament" => {
                    if let Some(t) = Temperament::from_config_str(val) {
                        prefs.behaviors.temperament = t;
                    }
                }
                "scale" => {
                    if let Ok(s) = val.parse::<f32>()
                        && s.is_finite()
                    {
                        prefs.scale = s.clamp(0.5, 2.5);
                    }
                }
                "follow_cursor" => {
                    if let Some(b) = parse_bool(val) {
                        prefs.behaviors.follow_cursor = b;
                    }
                }
                "playful_quirks" => {
                    if let Some(b) = parse_bool(val) {
                        prefs.behaviors.playful_quirks = b;
                    }
                }
                _ => {}
            }
        }
        prefs
    }
}

fn parse_bool(val: &str) -> Option<bool> {
    if val.eq_ignore_ascii_case("true")
        || val.eq_ignore_ascii_case("yes")
        || val.eq_ignore_ascii_case("on")
        || val == "1"
    {
        Some(true)
    } else if val.eq_ignore_ascii_case("false")
        || val.eq_ignore_ascii_case("no")
        || val.eq_ignore_ascii_case("off")
        || val == "0"
    {
        Some(false)
    } else {
        None
    }
}

pub fn default_config_path() -> Option<PathBuf> {
    resolve_config_path(
        std::env::var_os("XDG_CONFIG_HOME").as_deref(),
        #[cfg(any(target_os = "windows", windows_check))]
        std::env::var_os("APPDATA").as_deref(),
        #[cfg(any(target_os = "windows", windows_check))]
        std::env::var_os("LOCALAPPDATA").as_deref(),
        #[cfg(any(target_os = "windows", windows_check))]
        std::env::var_os("USERPROFILE").as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
}

fn resolve_config_path(
    xdg_config_home: Option<&OsStr>,
    #[cfg(any(target_os = "windows", windows_check))] appdata: Option<&OsStr>,
    #[cfg(any(target_os = "windows", windows_check))] localappdata: Option<&OsStr>,
    #[cfg(any(target_os = "windows", windows_check))] userprofile: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Option<PathBuf> {
    if let Some(xdg) = xdg_config_home
        && !xdg.is_empty()
    {
        let xdg_path = PathBuf::from(xdg);
        if xdg_path.is_absolute() {
            return Some(xdg_path.join("catsor").join("config.toml"));
        }
    }

    #[cfg(any(target_os = "windows", windows_check))]
    {
        if let Some(appdata) = appdata
            && !appdata.is_empty()
        {
            return Some(PathBuf::from(appdata).join("catsor").join("config.toml"));
        }
        if let Some(localappdata) = localappdata
            && !localappdata.is_empty()
        {
            return Some(
                PathBuf::from(localappdata)
                    .join("catsor")
                    .join("config.toml"),
            );
        }
        if let Some(profile) = userprofile
            && !profile.is_empty()
        {
            return Some(
                PathBuf::from(profile)
                    .join("AppData")
                    .join("Roaming")
                    .join("catsor")
                    .join("config.toml"),
            );
        }
    }

    if let Some(home) = home
        && !home.is_empty()
    {
        return Some(
            PathBuf::from(home)
                .join(".config")
                .join("catsor")
                .join("config.toml"),
        );
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_all_user_preferences() {
        let prefs = UserPrefs {
            coat: CoatStyle::Tuxedo,
            scale: 1.35,
            behaviors: BehaviorConfig {
                temperament: Temperament::Playful,
                follow_cursor: false,
                playful_quirks: true,
            },
        };
        let serialized = prefs.to_toml_string();
        let parsed = UserPrefs::parse_toml(&serialized);
        assert_eq!(parsed.coat, CoatStyle::Tuxedo);
        assert!((parsed.scale - 1.35).abs() < 1e-4);
        assert_eq!(parsed.behaviors, prefs.behaviors);
    }

    #[test]
    fn saves_and_loads_from_disk_and_ignores_invalid_entries() {
        let dir = std::env::temp_dir().join(format!("catsor_prefs_test_{}", std::process::id()));
        let path = dir.join("config.toml");
        let _ = fs::remove_dir_all(&dir);

        let prefs = UserPrefs {
            coat: CoatStyle::Silver,
            scale: 0.80,
            behaviors: BehaviorConfig {
                temperament: Temperament::Aloof,
                follow_cursor: true,
                playful_quirks: false,
            },
        };
        prefs.save_to_path(&path).unwrap();

        let loaded = UserPrefs::load_from_path(&path);
        assert_eq!(loaded.coat, CoatStyle::Silver);
        assert!((loaded.scale - 0.80).abs() < 1e-4);
        assert_eq!(loaded.behaviors, prefs.behaviors);

        // Verify partial/hand-edited config with comments and out-of-range scale clamps cleanly
        let custom = "\
            # comment line
            coat = Charcoal
            temperament = 'LAZY'
            scale = 9.9
            follow_cursor = off
            unknown_key = 123
            startle_enabled = true
        ";
        let parsed = UserPrefs::parse_toml(custom);
        assert_eq!(parsed.coat, CoatStyle::Charcoal);
        assert_eq!(parsed.behaviors.temperament, Temperament::Lazy);
        assert!((parsed.scale - 2.5).abs() < 1e-4);
        assert!(!parsed.behaviors.follow_cursor);
        assert!(parsed.behaviors.playful_quirks);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ignores_relative_xdg_config_home() {
        let resolved = resolve_config_path(
            Some(OsStr::new("relative/config")),
            #[cfg(any(target_os = "windows", windows_check))]
            None,
            #[cfg(any(target_os = "windows", windows_check))]
            None,
            #[cfg(any(target_os = "windows", windows_check))]
            None,
            Some(OsStr::new("/home/tester")),
        );
        assert_eq!(
            resolved,
            Some(PathBuf::from("/home/tester/.config/catsor/config.toml"))
        );
    }
}
