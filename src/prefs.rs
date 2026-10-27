use crate::cat::Temperament;
use crate::cat::clowder::{CatProfile, Clowder};
use crate::coat::{CoatId, strip_comment, unquote};
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq)]
pub struct UserPrefs {
    /// Every cat, in tray order; never empty after `parse_toml`
    pub cats: Vec<CatProfile>,
    /// The tray's Size, shared by all cats (kittens are a fraction of it)
    pub scale: f32,
    pub follow_cursor: bool,
    pub playful_quirks: bool,
}

impl Default for UserPrefs {
    fn default() -> Self {
        Self {
            cats: vec![CatProfile::default()],
            scale: 1.0,
            follow_cursor: true,
            playful_quirks: true,
        }
    }
}

impl UserPrefs {
    pub fn from_clowder(clowder: &Clowder) -> Self {
        Self {
            cats: clowder.profiles(),
            scale: clowder.scale().clamp(0.5, 2.5),
            follow_cursor: clowder.follow_cursor(),
            playful_quirks: clowder.playful_quirks(),
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

    pub fn save(&self) {
        if let Some(path) = default_config_path() {
            let _ = self.save_to_path(&path);
        }
    }

    pub fn save_to_path(&self, path: &Path) -> io::Result<()> {
        write_atomic(path, &self.to_toml_string())
    }

    pub fn to_toml_string(&self) -> String {
        let mut out = format!(
            "# Catsor user preferences\n\
             scale = {:.2}\n\
             follow_cursor = {}\n\
             playful_quirks = {}\n",
            self.scale.clamp(0.5, 2.5),
            self.follow_cursor,
            self.playful_quirks,
        );
        for cat in &self.cats {
            out.push_str(&format!(
                "\n[[cat]]\n\
                 name = \"{}\"\n\
                 coat = \"{}\"\n\
                 temperament = \"{}\"\n",
                cat.name,
                cat.coat.to_config_string(),
                cat.temperament.as_config_str(),
            ));
            if (cat.size - 1.0).abs() > 1e-3 {
                out.push_str(&format!("size = {:.2}\n", cat.size));
            }
            if cat.kitten {
                out.push_str("kitten = true\n");
            }
            if let Some(mother) = &cat.mother {
                out.push_str(&format!("mother = \"{mother}\"\n"));
            }
        }
        out
    }

    /// Reads the top-level keys and one `[[cat]]` table per cat. A file from before multiple
    /// cats has `coat` and `temperament` at the top level instead; they become the one cat.
    pub fn parse_toml(input: &str) -> Self {
        let mut prefs = Self {
            cats: Vec::new(),
            ..Self::default()
        };
        // The pre-[[cat]] single cat, used only when there are no [[cat]] tables
        let mut legacy = CatProfile::default();
        let mut current: Option<CatProfile> = None;
        for raw_line in input.lines() {
            let line = strip_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') {
                prefs.cats.extend(current.take());
                if line.replace(' ', "") == "[[cat]]" {
                    current = Some(CatProfile {
                        name: String::new(),
                        ..CatProfile::default()
                    });
                }
                continue;
            }
            let Some((raw_key, raw_val)) = line.split_once('=') else {
                continue;
            };
            let key = raw_key.trim();
            let val = unquote(raw_val).trim();

            if let Some(cat) = &mut current {
                parse_cat_key(cat, key, val);
                continue;
            }
            match key {
                "coat" | "temperament" => parse_cat_key(&mut legacy, key, val),
                "scale" => {
                    if let Ok(s) = val.parse::<f32>()
                        && s.is_finite()
                    {
                        prefs.scale = s.clamp(0.5, 2.5);
                    }
                }
                "follow_cursor" => {
                    if let Some(b) = parse_bool(val) {
                        prefs.follow_cursor = b;
                    }
                }
                "playful_quirks" => {
                    if let Some(b) = parse_bool(val) {
                        prefs.playful_quirks = b;
                    }
                }
                _ => {}
            }
        }
        prefs.cats.extend(current);
        if prefs.cats.is_empty() {
            prefs.cats.push(legacy);
        }
        prefs
    }
}

/// Applies one `key = value` line of a `[[cat]]` table; invalid values are ignored
fn parse_cat_key(cat: &mut CatProfile, key: &str, val: &str) {
    match key {
        "name" => cat.name = val.to_string(),
        "coat" => {
            if let Some(c) = CoatId::parse(val) {
                cat.coat = c;
            }
        }
        "temperament" => {
            if let Some(t) = Temperament::from_config_str(val) {
                cat.temperament = t;
            }
        }
        "size" => {
            if let Ok(s) = val.parse::<f32>()
                && s.is_finite()
            {
                cat.size = s.clamp(CatProfile::MIN_SIZE, CatProfile::MAX_SIZE);
            }
        }
        "kitten" => {
            if let Some(b) = parse_bool(val) {
                cat.kitten = b;
            }
        }
        "mother" => cat.mother = Some(val.to_string()).filter(|m| !m.is_empty()),
        _ => {}
    }
}

/// Writes `content` to `path` through a temp file in the same directory and a rename, so a
/// crash mid-write never leaves a half-written file. Creates the parent directory if needed
pub(crate) fn write_atomic(path: &Path, content: &str) -> io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
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
    use crate::coat::CoatStyle;

    fn kitten(name: &str, mother: &str) -> CatProfile {
        CatProfile {
            size: 0.6,
            kitten: true,
            mother: Some(mother.into()),
            ..CatProfile::new(name, CoatId::Random(0x00c0_ffee), Temperament::Playful)
        }
    }

    #[test]
    fn roundtrips_all_user_preferences() {
        let prefs = UserPrefs {
            cats: vec![
                CatProfile::new(
                    "Luna",
                    CoatId::Builtin(CoatStyle::Tuxedo),
                    Temperament::Aloof,
                ),
                kitten("Pip", "Luna"),
            ],
            scale: 1.35,
            follow_cursor: false,
            playful_quirks: true,
        };
        let parsed = UserPrefs::parse_toml(&prefs.to_toml_string());
        assert_eq!(parsed.cats, prefs.cats);
        assert!((parsed.scale - 1.35).abs() < 1e-4);
        assert!(!parsed.follow_cursor);
        assert!(parsed.playful_quirks);

        for coat in [
            CoatId::Custom("Mr. #1 Cat".into()),
            CoatId::Custom("Jones'".into()),
            CoatId::Custom("'Tis".into()),
            CoatId::Random(0xdead_beef),
        ] {
            let prefs = UserPrefs {
                cats: vec![CatProfile::new("Luna", coat.clone(), Temperament::Balanced)],
                ..UserPrefs::default()
            };
            assert_eq!(
                UserPrefs::parse_toml(&prefs.to_toml_string()).cats[0].coat,
                coat
            );
        }
    }

    #[test]
    fn saves_and_loads_from_disk_and_ignores_invalid_entries() {
        let dir = std::env::temp_dir().join(format!("catsor_prefs_test_{}", std::process::id()));
        let path = dir.join("config.toml");
        let _ = fs::remove_dir_all(&dir);

        let prefs = UserPrefs {
            cats: vec![CatProfile::new(
                "Luna",
                CoatId::Builtin(CoatStyle::Silver),
                Temperament::Aloof,
            )],
            scale: 0.80,
            follow_cursor: true,
            playful_quirks: false,
        };
        prefs.save_to_path(&path).unwrap();
        assert_eq!(UserPrefs::load_from_path(&path), prefs);

        // Partial/hand-edited config with comments, unknown keys and out-of-range values
        let custom = "\
            # comment line
            scale = 9.9
            follow_cursor = off
            unknown_key = 123

            [[cat]]
            name = 'Mochi' # the old one
            coat = Charcoal
            temperament = 'LAZY'
            size = 7
            kitten = maybe
            whiskers = 24

            [other.table]
            name = ignored
        ";
        let parsed = UserPrefs::parse_toml(custom);
        assert_eq!(parsed.cats.len(), 1);
        let mochi = &parsed.cats[0];
        assert_eq!(mochi.name, "Mochi");
        assert_eq!(mochi.coat, CoatId::Builtin(CoatStyle::Charcoal));
        assert_eq!(mochi.temperament, Temperament::Lazy);
        assert!((mochi.size - CatProfile::MAX_SIZE).abs() < 1e-4);
        assert!(!mochi.kitten);
        assert!((parsed.scale - 2.5).abs() < 1e-4);
        assert!(!parsed.follow_cursor);
        assert!(parsed.playful_quirks);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn single_cat_config_from_before_multiple_cats_becomes_the_first_cat() {
        let old = "\
            # Catsor user preferences
            coat = \"tuxedo\"
            temperament = \"playful\"
            scale = 1.35
            follow_cursor = true
            playful_quirks = false
        ";
        let parsed = UserPrefs::parse_toml(old);
        assert_eq!(parsed.cats.len(), 1);
        assert_eq!(parsed.cats[0].coat, CoatId::Builtin(CoatStyle::Tuxedo));
        assert_eq!(parsed.cats[0].temperament, Temperament::Playful);
        assert_eq!(parsed.cats[0].name, CatProfile::default().name);
        assert!(!parsed.playful_quirks);

        // An empty file still has one cat
        assert_eq!(UserPrefs::parse_toml("").cats, vec![CatProfile::default()]);
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
