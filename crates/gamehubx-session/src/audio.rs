use std::fs;
use std::path::{Path, PathBuf};

use gamehubx_runtime::CommandSpec;
use gamehubx_supervisor::RunningSession;

use crate::launch::WineLaunch;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioMode {
    Pulse,
    Alsa,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioError {
    MissingPulseAudio,
    Io,
}

#[derive(Clone, Debug)]
pub struct AudioPlan {
    pub config_dir: PathBuf,
    pub socket: PathBuf,
    pub command: CommandSpec,
}

impl AudioPlan {
    pub fn prepare(
        imagefs_root: impl Into<PathBuf>,
        container_root: impl Into<PathBuf>,
        mode: AudioMode,
    ) -> Result<Option<Self>, AudioError> {
        if mode == AudioMode::Alsa {
            return Ok(None);
        }
        let imagefs_root = imagefs_root.into();
        let container_root = container_root.into();
        let pulseaudio = imagefs_root.join("usr/bin/pulseaudio");
        if !pulseaudio.is_file() {
            return Err(AudioError::MissingPulseAudio);
        }
        let config_dir = container_root.join("pulseaudio");
        fs::create_dir_all(&config_dir).map_err(|_| AudioError::Io)?;
        let socket = container_root.join("tmp/.sound/PS0");
        let config = format!(
            "load-module module-native-protocol-unix auth-anonymous=1 auth-cookie-enabled=0 socket=\"{}\"\nload-module module-aaudio-sink\nset-default-sink AAudio_sink\n",
            socket.display()
        );
        fs::write(config_dir.join("default.pa"), config).map_err(|_| AudioError::Io)?;
        let modules = imagefs_root.join("usr/lib/pulseaudio/modules");
        let config_file = config_dir.join("default.pa");
        let command = CommandSpec::new(pulseaudio)
            .args([
                "--system=false",
                "--fail=false",
                "-n",
                &format!("--file={}", config_file.display()),
                "--daemonize=false",
                "--use-pid-file=false",
                "--exit-idle-time=-1",
            ])
            .with_cwd(&config_dir)
            .env("HOME", path_string(&config_dir))
            .env(
                "LD_LIBRARY_PATH",
                format!(
                    "{}:{}:/system/lib64",
                    imagefs_root.join("usr/lib").display(),
                    modules.display()
                ),
            );
        Ok(Some(Self {
            config_dir,
            socket,
            command,
        }))
    }

    pub fn apply_to(&self, launch: &mut WineLaunch) {
        set_env(
            &mut launch.spec.env,
            "PULSE_SERVER",
            path_string(&self.socket),
        );
        set_env(&mut launch.spec.env, "PULSE_LATENCY_MSEC", "60");
    }

    pub fn spawn(&self) -> std::io::Result<AudioProcess> {
        AudioProcess::spawn(&self.command)
    }
}

pub struct AudioProcess {
    session: RunningSession,
}

impl AudioProcess {
    pub fn spawn(spec: &CommandSpec) -> std::io::Result<Self> {
        Ok(Self {
            session: gamehubx_supervisor::Supervisor::spawn(spec)?,
        })
    }

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

impl Drop for AudioProcess {
    fn drop(&mut self) {
        let _ = self.session.terminate();
    }
}

fn set_env(env: &mut Vec<(String, String)>, key: &str, value: impl Into<String>) {
    let value = value.into();
    if let Some(pair) = env.iter_mut().find(|(name, _)| name == key) {
        pair.1 = value;
    } else {
        env.push((key.to_string(), value));
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::WinlatorContainer;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gamehubx-audio-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn imagefs(root: &Path) {
        fs::create_dir_all(root.join("usr/bin")).unwrap();
        fs::create_dir_all(root.join("usr/lib/pulseaudio/modules")).unwrap();
        fs::create_dir_all(root.join("opt/proton-11.0-7-arm64ec/bin")).unwrap();
        fs::create_dir_all(root.join("home/xuser")).unwrap();
        fs::write(root.join("usr/bin/pulseaudio"), b"").unwrap();
        fs::write(root.join("opt/proton-11.0-7-arm64ec/bin/wine"), b"").unwrap();
    }

    #[test]
    fn pulse_plan_writes_config_and_applies_env() {
        let root = temp("pulse");
        imagefs(&root);
        let container = root.join("container");
        let plan = AudioPlan::prepare(&root, &container, AudioMode::Pulse)
            .unwrap()
            .unwrap();
        assert!(plan.config_dir.join("default.pa").is_file());
        assert_eq!(plan.command.program, root.join("usr/bin/pulseaudio"));
        assert!(plan
            .command
            .env_value("LD_LIBRARY_PATH")
            .unwrap()
            .contains("pulseaudio/modules"));

        let exe = root.join("game.exe");
        fs::write(&exe, b"").unwrap();
        let winlator_container = WinlatorContainer {
            id: 1,
            name: "Test".into(),
            wine_version: "proton-11.0-7-arm64ec".into(),
            screen_size: "1280x720".into(),
            root: container.clone(),
        };
        let mut launch =
            WineLaunch::from_container(&winlator_container, &root, &exe, 0, None).unwrap();
        plan.apply_to(&mut launch);
        assert_eq!(
            launch.spec.env_value("PULSE_SERVER"),
            Some(plan.socket.to_string_lossy().as_ref())
        );
        assert_eq!(launch.spec.env_value("PULSE_LATENCY_MSEC"), Some("60"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn alsa_has_no_helper_process() {
        let root = temp("alsa");
        assert!(
            AudioPlan::prepare(&root, root.join("container"), AudioMode::Alsa)
                .unwrap()
                .is_none()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_pulse_binary_is_reported() {
        let root = temp("missing");
        assert_eq!(
            AudioPlan::prepare(&root, root.join("container"), AudioMode::Pulse).unwrap_err(),
            AudioError::MissingPulseAudio
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn audio_process_owns_group() {
        let mut spec = CommandSpec::new("sh");
        spec.args = vec!["-c".into(), "exit 0".into()];
        let mut process = AudioProcess::spawn(&spec).unwrap();
        let status = process.wait().unwrap();
        assert!(status.success());
    }
}
