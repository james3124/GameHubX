use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static ANDROID_FILES_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn set_android_files_dir(path: impl Into<PathBuf>) {
    let _ = ANDROID_FILES_DIR.set(path.into());
}

pub fn files_root() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        PathBuf::from("/storage/emulated/0")
    }
    #[cfg(not(target_os = "android"))]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"))
    }
}

pub fn data_dir() -> PathBuf {
    if let Some(dir) = ANDROID_FILES_DIR.get() {
        return dir.join("gamehubx");
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("gamehubx");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/share/gamehubx");
    }
    PathBuf::from("gamehubx-data")
}

pub fn battery_level() -> Option<u8> {
    std::fs::read_to_string("/sys/class/power_supply/battery/capacity")
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Mask of CPUs whose max frequency is at least 80% of the fastest core.
pub fn big_core_mask() -> Option<u64> {
    let mut frequencies = Vec::new();
    for cpu in 0..64 {
        let path = format!("/sys/devices/system/cpu/cpu{cpu}/cpufreq/cpuinfo_max_freq");
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Ok(frequency) = text.trim().parse::<u64>() {
                frequencies.push((cpu, frequency));
            }
        }
    }
    compute_big_core_mask(&frequencies)
}

pub fn compute_big_core_mask(frequencies: &[(usize, u64)]) -> Option<u64> {
    let fastest = frequencies.iter().map(|(_, frequency)| *frequency).max()?;
    if fastest == 0 {
        return None;
    }
    let threshold = fastest.saturating_mul(4) / 5;
    let mut mask = 0u64;
    for (cpu, frequency) in frequencies {
        if *cpu < 64 && *frequency >= threshold {
            mask |= 1u64 << cpu;
        }
    }
    (mask != 0).then_some(mask)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageStats {
    pub total: u64,
    pub available: u64,
}

impl StorageStats {
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }
}

#[cfg(unix)]
pub fn storage_stats(path: &Path) -> Option<StorageStats> {
    use std::os::unix::ffi::OsStrExt;

    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    let block = stat.f_frsize as u64;
    let total = (stat.f_blocks as u64).saturating_mul(block);
    let available = (stat.f_bavail as u64).saturating_mul(block);
    Some(StorageStats { total, available })
}

#[cfg(not(unix))]
pub fn storage_stats(_path: &Path) -> Option<StorageStats> {
    None
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

pub fn files_access_granted() -> bool {
    #[cfg(target_os = "android")]
    {
        with_activity(|env, activity| -> jni::errors::Result<bool> {
            let value = env.call_method(
                &activity,
                jni::jni_str!("filesAccessGranted"),
                jni::jni_sig!("()Z"),
                &[],
            )?;
            Ok(matches!(value, jni::JValueOwned::Bool(true)))
        })
        .unwrap_or(false)
    }
    #[cfg(not(target_os = "android"))]
    {
        true
    }
}

pub fn request_files_access() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        with_activity(|env, activity| -> jni::errors::Result<()> {
            env.call_method(
                &activity,
                jni::jni_str!("requestFilesAccess"),
                jni::jni_sig!("()V"),
                &[],
            )?;
            Ok(())
        })
        .ok_or_else(|| "could not open all-files access settings".to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(())
    }
}

pub fn launch_winlator_main() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        with_activity(|env, activity| -> jni::errors::Result<()> {
            env.call_method(
                &activity,
                jni::jni_str!("openWinlator"),
                jni::jni_sig!("()V"),
                &[],
            )?;
            Ok(())
        })
        .ok_or_else(|| "could not open the Winlator session".to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        Err("Winlator session is Android-only".to_string())
    }
}

pub fn system_language() -> String {
    #[cfg(target_os = "android")]
    {
        with_activity(|env, activity| -> jni::errors::Result<String> {
            let value = env.call_method(
                &activity,
                jni::jni_str!("systemLanguage"),
                jni::jni_sig!("()Ljava/lang/String;"),
                &[],
            )?;
            if value.is_null() {
                return Ok("en".to_string());
            }
            let object = value.l()?;
            let language = env.cast_local::<jni::objects::JString>(object)?;
            Ok(language.to_string())
        })
        .unwrap_or_else(|| "en".to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        std::env::var("LC_ALL")
            .or_else(|_| std::env::var("LANG"))
            .map(|value| value.split(['.', '@']).next().unwrap_or("en").to_string())
            .unwrap_or_else(|_| "en".to_string())
    }
}

pub fn poll_gamepad_action() -> i32 {
    #[cfg(target_os = "android")]
    {
        with_activity(|env, activity| -> jni::errors::Result<i32> {
            let value = env.call_method(
                &activity,
                jni::jni_str!("pollGamepadAction"),
                jni::jni_sig!("()I"),
                &[],
            )?;
            Ok(match value {
                jni::JValueOwned::Int(code) => code,
                _ => 0,
            })
        })
        .unwrap_or(0)
    }
    #[cfg(not(target_os = "android"))]
    {
        0
    }
}

pub fn launch_winlator_game(exe: &str, name: &str) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let outcome = with_activity(|env, activity| -> jni::errors::Result<Option<String>> {
            let exe = env.new_string(exe)?;
            let name = env.new_string(name)?;
            let value = env.call_method(
                &activity,
                jni::jni_str!("launchWinlator"),
                jni::jni_sig!("(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;"),
                &[jni::JValue::Object(&*exe), jni::JValue::Object(&*name)],
            )?;
            if value.is_null() {
                return Ok(None);
            }
            let object = value.l()?;
            let message = env.cast_local::<jni::objects::JString>(object)?;
            Ok(Some(message.to_string()))
        });
        match outcome {
            Some(None) => Ok(()),
            Some(Some(error)) => Err(error),
            None => Err("could not call the Winlator launcher".to_string()),
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (exe, name);
        Err("Winlator session is Android-only".to_string())
    }
}

#[cfg(target_os = "android")]
fn with_activity<T>(
    call: impl FnOnce(&mut jni::Env<'_>, jni::objects::JObject<'_>) -> jni::errors::Result<T>,
) -> Option<T> {
    let context = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast()) };
    vm.attach_current_thread(|env| -> jni::errors::Result<T> {
        let activity = unsafe { jni::objects::JObject::from_raw(env, context.context().cast()) };
        call(env, activity)
    })
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_root_is_absolute() {
        assert!(files_root().is_absolute());
    }

    #[test]
    fn data_dir_is_namespaced() {
        let dir = data_dir();
        assert!(dir.ends_with("gamehubx") || dir.as_path() == Path::new("gamehubx-data"));
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[cfg(unix)]
    #[test]
    fn storage_stats_for_root() {
        if let Some(stats) = storage_stats(Path::new("/")) {
            assert!(stats.total >= stats.available);
            assert_eq!(stats.used(), stats.total - stats.available);
        }
    }

    #[cfg(not(target_os = "android"))]
    #[test]
    fn desktop_has_files_access() {
        assert!(files_access_granted());
    }

    #[test]
    fn computes_big_core_mask() {
        let frequencies = [(0, 1_800_000), (4, 2_800_000), (7, 2_600_000)];
        let mask = compute_big_core_mask(&frequencies).unwrap();
        assert_eq!(mask & (1 << 4), 1 << 4);
        assert_eq!(mask & (1 << 7), 1 << 7);
        assert_eq!(mask & 1, 0);
        assert_eq!(compute_big_core_mask(&[]), None);
    }
}
