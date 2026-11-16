use std::path::{Path, PathBuf};

use gamehubx_runtime::CommandSpec;
use gamehubx_supervisor::RunningSession;

use crate::audio::{AudioError, AudioMode, AudioPlan, AudioProcess};
use crate::container::WinlatorContainer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchError {
    NotAbsolute,
    MissingWine,
    MissingExe,
    InvalidImageFs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionBuildError {
    Launch(LaunchError),
    Audio(AudioError),
}

impl From<LaunchError> for SessionBuildError {
    fn from(error: LaunchError) -> Self {
        Self::Launch(error)
    }
}

impl From<AudioError> for SessionBuildError {
    fn from(error: AudioError) -> Self {
        Self::Audio(error)
    }
}

#[derive(Clone, Debug)]
pub struct SessionLaunch {
    pub wine: WineLaunch,
    pub audio: Option<AudioPlan>,
}

impl SessionLaunch {
    pub fn build(
        container: &WinlatorContainer,
        imagefs_root: impl Into<PathBuf>,
        exe: impl AsRef<Path>,
        audio: AudioMode,
        memory_limit_mb: u32,
        cpu_affinity: Option<u64>,
    ) -> Result<Self, SessionBuildError> {
        let imagefs_root = imagefs_root.into();
        let mut wine = WineLaunch::from_container(
            container,
            &imagefs_root,
            exe,
            memory_limit_mb,
            cpu_affinity,
        )?;
        let audio_plan = AudioPlan::prepare(&imagefs_root, &container.root, audio)?;
        if let Some(plan) = &audio_plan {
            plan.apply_to(&mut wine);
        }
        Ok(Self {
            wine,
            audio: audio_plan,
        })
    }

    pub fn spawn(&self) -> std::io::Result<SessionProcess> {
        let audio = match &self.audio {
            Some(plan) => Some(plan.spawn()?),
            None => None,
        };
        match self.wine.spawn() {
            Ok(wine) => Ok(SessionProcess { wine, audio }),
            Err(error) => {
                drop(audio);
                Err(error)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct WineLaunch {
    pub spec: CommandSpec,
    pub wine_prefix: PathBuf,
    pub imagefs_root: PathBuf,
}

impl WineLaunch {
    pub fn from_container(
        container: &WinlatorContainer,
        imagefs_root: impl Into<PathBuf>,
        exe: impl AsRef<Path>,
        memory_limit_mb: u32,
        cpu_affinity: Option<u64>,
    ) -> Result<Self, LaunchError> {
        let imagefs_root = imagefs_root.into();
        let exe = exe.as_ref();
        if !imagefs_root.is_absolute() || !exe.is_absolute() {
            return Err(LaunchError::NotAbsolute);
        }
        if !imagefs_root.is_dir() {
            return Err(LaunchError::InvalidImageFs);
        }
        if !exe.is_file() {
            return Err(LaunchError::MissingExe);
        }

        let wine_root = imagefs_root.join("opt").join(&container.wine_version);
        let wine = wine_root.join("bin/wine");
        if !wine.is_file() {
            return Err(LaunchError::MissingWine);
        }
        let home = imagefs_root.join("home/xuser");
        let wine_prefix = home.join(".wine");
        let wine_bin = wine_root.join("bin");
        let bin = imagefs_root.join("usr/bin");
        let lib = imagefs_root.join("usr/lib");

        let mut spec = CommandSpec::new(wine)
            .args([path_string(exe)])
            .with_cwd(&imagefs_root)
            .env("HOME", path_string(&home))
            .env("USER", "xuser")
            .env("TMPDIR", path_string(&imagefs_root.join("usr/tmp")))
            .env(
                "XDG_DATA_DIRS",
                path_string(&imagefs_root.join("usr/share")),
            )
            .env(
                "XDG_CONFIG_DIRS",
                path_string(&imagefs_root.join("usr/etc/xdg")),
            )
            .env("DISPLAY", ":0")
            .env("WINEPREFIX", path_string(&wine_prefix))
            .env("PREFIX", path_string(&imagefs_root.join("usr")))
            .env(
                "LD_LIBRARY_PATH",
                format!("{}:/system/lib64", lib.display()),
            )
            .env("PATH", format!("{}:{}", wine_bin.display(), bin.display()))
            .env("WINEESYNC", "1")
            .env("WINEFSYNC", "1")
            .env("WINE_NEW_NDIS", "1")
            .env("WINE_X11FORCEGLX", "1")
            .env("WINE_GST_NO_GL", "1")
            .env("SteamGameId", "0")
            .env("WINE_NO_DUPLICATE_EXPLORER", "1");
        spec.memory_limit_bytes =
            (memory_limit_mb > 0).then(|| u64::from(memory_limit_mb) * 1024 * 1024);
        spec.cpu_affinity = cpu_affinity;

        Ok(Self {
            spec,
            wine_prefix,
            imagefs_root,
        })
    }

    pub fn spawn(&self) -> std::io::Result<WineProcess> {
        Ok(WineProcess {
            session: gamehubx_supervisor::Supervisor::spawn(&self.spec)?,
        })
    }
}

pub struct WineProcess {
    session: RunningSession,
}

pub struct SessionProcess {
    wine: WineProcess,
    audio: Option<AudioProcess>,
}

impl SessionProcess {
    pub fn pid(&self) -> u32 {
        self.wine.pid()
    }

    pub fn audio_pid(&self) -> Option<u32> {
        self.audio.as_ref().map(AudioProcess::pid)
    }

    pub fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        if let Some(status) = self.wine.try_wait()? {
            self.stop_audio();
            return Ok(Some(status));
        }
        Ok(None)
    }

    pub fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        let status = self.wine.wait();
        self.stop_audio();
        status
    }

    pub fn terminate(&mut self) -> std::io::Result<()> {
        self.wine.terminate()?;
        self.stop_audio();
        Ok(())
    }

    fn stop_audio(&mut self) {
        if let Some(mut audio) = self.audio.take() {
            let _ = audio.terminate();
        }
    }
}

impl Drop for SessionProcess {
    fn drop(&mut self) {
        let _ = self.wine.terminate();
        self.stop_audio();
    }
}

impl WineProcess {
    pub fn pid(&self) -> u32 {
        self.session.pid()
    }

    pub fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.session.try_wait()
    }

    pub fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        self.session.wait()
    }

    pub fn terminate(&mut self) -> std::io::Result<()> {
        self.session.terminate()
    }
}

impl Drop for WineProcess {
    fn drop(&mut self) {
        let _ = self.session.terminate();
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gamehubx-launch-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn container(root: &Path) -> WinlatorContainer {
        WinlatorContainer {
            id: 1,
            name: "Test".into(),
            wine_version: "proton-11.0-7-arm64ec".into(),
            screen_size: "1280x720".into(),
            root: root.join("home/xuser-1"),
        }
    }

    #[test]
    fn builds_wine_command_and_limits() {
        let root = temp("command");
        fs::create_dir_all(root.join("opt/proton-11.0-7-arm64ec/bin")).unwrap();
        fs::create_dir_all(root.join("home/xuser")).unwrap();
        fs::create_dir_all(root.join("usr/bin")).unwrap();
        fs::create_dir_all(root.join("usr/lib")).unwrap();
        let wine = root.join("opt/proton-11.0-7-arm64ec/bin/wine");
        fs::write(&wine, b"").unwrap();
        let exe = root.join("game.exe");
        fs::write(&exe, b"").unwrap();
        let launch =
            WineLaunch::from_container(&container(&root), &root, &exe, 2048, Some(0b101)).unwrap();
        assert_eq!(launch.spec.program, wine);
        assert_eq!(launch.spec.args, vec![exe.to_string_lossy().to_string()]);
        assert_eq!(launch.spec.env_value("DISPLAY"), Some(":0"));
        let expected_prefix = root.join("home/xuser/.wine");
        assert_eq!(
            launch.spec.env_value("WINEPREFIX"),
            Some(expected_prefix.to_string_lossy().as_ref())
        );
        assert_eq!(launch.spec.memory_limit_bytes, Some(2048 * 1024 * 1024));
        assert_eq!(launch.spec.cpu_affinity, Some(0b101));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_missing_wine() {
        let root = temp("missing");
        let exe = root.join("game.exe");
        fs::write(&exe, b"").unwrap();
        assert_eq!(
            WineLaunch::from_container(&container(&root), &root, &exe, 0, None).unwrap_err(),
            LaunchError::MissingWine
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn owns_and_reaps_child() {
        let mut spec = CommandSpec::new("sh");
        spec.args = vec!["-c".into(), "exit 9".into()];
        let mut process = WineProcess {
            session: gamehubx_supervisor::Supervisor::spawn(&spec).unwrap(),
        };
        let status = process.wait().unwrap();
        assert_eq!(status.code(), Some(9));
    }

    #[cfg(unix)]
    #[test]
    fn combined_session_owns_wine_and_no_audio_for_alsa() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp("session");
        fs::create_dir_all(root.join("opt/proton-11.0-7-arm64ec/bin")).unwrap();
        fs::create_dir_all(root.join("home/xuser")).unwrap();
        fs::create_dir_all(root.join("usr/bin")).unwrap();
        fs::create_dir_all(root.join("usr/lib")).unwrap();
        let wine = root.join("opt/proton-11.0-7-arm64ec/bin/wine");
        fs::write(&wine, b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&wine, fs::Permissions::from_mode(0o755)).unwrap();
        let exe = root.join("game.exe");
        fs::write(&exe, b"").unwrap();
        let container = WinlatorContainer {
            id: 1,
            name: "Test".into(),
            wine_version: "proton-11.0-7-arm64ec".into(),
            screen_size: "1280x720".into(),
            root: root.join("home/xuser-1"),
        };
        let launch = SessionLaunch::build(
            &container,
            &root,
            &exe,
            crate::audio::AudioMode::Alsa,
            0,
            None,
        )
        .unwrap();
        assert!(launch.audio.is_none());
        let mut process = launch.spawn().unwrap();
        assert!(process.audio_pid().is_none());
        assert!(process.wait().unwrap().success());
        let _ = fs::remove_dir_all(root);
    }
}
