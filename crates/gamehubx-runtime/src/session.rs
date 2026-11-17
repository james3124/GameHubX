use std::path::{Path, PathBuf};

use crate::layout::{is_within, RuntimeLayout};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
    pub memory_limit_bytes: Option<u64>,
    pub cpu_affinity: Option<u64>,
}

impl CommandSpec {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            cwd: PathBuf::new(),
            memory_limit_bytes: None,
            cpu_affinity: None,
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = cwd.into();
        self
    }

    pub fn env_value(&self, key: &str) -> Option<&str> {
        self.env
            .iter()
            .find(|(name, _)| name.as_str() == key)
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionError {
    NotAbsolute,
    ExeOutsideGameDir,
    InvalidFps,
    InvalidResolution,
    UnsupportedPath,
    Io,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for SessionError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioBackend {
    Pulse,
    Alsa,
}

impl AudioBackend {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pulse => "Pulse",
            Self::Alsa => "Alsa",
        }
    }

    pub fn from_label(value: &str) -> Self {
        if value.eq_ignore_ascii_case("alsa") {
            Self::Alsa
        } else {
            Self::Pulse
        }
    }
}

/// Extra per-game options that map the extended Game Settings onto the
/// session environment and command. Defaults keep existing callers unchanged.
#[derive(Clone, Debug, Default)]
pub struct SessionExtras {
    pub container_language: String,
    pub mangohud: bool,
    pub directx_hud: String,
    pub disable_wm: bool,
    pub env_vars: Vec<(String, String)>,
    pub command_line: String,
    pub auto_keyboard: bool,
    pub disable_gfx_plugin: bool,
    pub compat_layer: String,
    pub translation_params: String,
    pub dinput_library: String,
    pub gpu_driver: String,
    pub surface_format: String,
    pub vkd3d_version: String,
    pub log_server: bool,
    pub wine_debug_params: String,
    pub cpu_core_limit: u32,
}

#[derive(Clone, Debug)]
pub struct SessionConfig {
    pub layout: RuntimeLayout,
    pub container: PathBuf,
    pub game_dir: PathBuf,
    pub exe: PathBuf,
    pub fps_cap: u32,
    pub width: u32,
    pub height: u32,
    pub audio: AudioBackend,
    pub dxvk: bool,
    pub memory_limit_mb: u32,
    pub cpu_affinity: Option<u64>,
    pub extras: SessionExtras,
}

impl SessionConfig {
    fn validate(&self) -> Result<(), SessionError> {
        if !self.layout.root.is_absolute()
            || !self.container.is_absolute()
            || !self.game_dir.is_absolute()
            || !self.exe.is_absolute()
        {
            return Err(SessionError::NotAbsolute);
        }
        if !is_within(&self.game_dir, &self.exe) {
            return Err(SessionError::ExeOutsideGameDir);
        }
        if self.fps_cap == 0 || self.fps_cap > 240 {
            return Err(SessionError::InvalidFps);
        }
        if self.width == 0 || self.height == 0 || self.width > 16384 || self.height > 16384 {
            return Err(SessionError::InvalidResolution);
        }
        for path in [&self.game_dir, &self.container, &self.layout.root] {
            if path.to_string_lossy().contains(':') {
                return Err(SessionError::UnsupportedPath);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayPlan {
    pub display: String,
    pub width: u32,
    pub height: u32,
    pub xserver: PathBuf,
    pub wm_server: PathBuf,
    pub perf_server: PathBuf,
    pub shm_server: PathBuf,
}

#[derive(Clone, Debug)]
pub struct SessionPlan {
    config: SessionConfig,
    pub command: CommandSpec,
    pub display: DisplayPlan,
}

impl SessionPlan {
    pub const BOOTSTRAP_STEPS: &'static [&'static str] = &[
        "create-prefix",
        "wineboot",
        "set-win64",
        "install-dxvk",
        "install-vkd3d",
        "apply-registry",
        "write-config",
    ];

    pub fn new(config: SessionConfig) -> Result<Self, SessionError> {
        config.validate()?;
        let tmp = config.container.join("tmp");
        let display = DisplayPlan {
            display: ":0".to_string(),
            width: config.width,
            height: config.height,
            xserver: config.layout.xserver(),
            wm_server: tmp.join(".wm/V0"),
            perf_server: tmp.join(".perf/V0"),
            shm_server: tmp.join(".sysvshm/SM0"),
        };
        let env = build_env(&config, &display);
        // Guest arguments: the exe path followed by any per-game launch args.
        let mut guest_args = vec![path_string(&config.exe)];
        guest_args.extend(split_command_line(&config.extras.command_line));
        let mut command = proot_command(&config, &env, &config.layout.wine(), &guest_args);
        command.memory_limit_bytes =
            (config.memory_limit_mb > 0).then(|| u64::from(config.memory_limit_mb) * 1024 * 1024);
        command.cpu_affinity = config.cpu_affinity;
        Ok(Self {
            config,
            command,
            display,
        })
    }

    pub fn config(&self) -> &SessionConfig {
        &self.config
    }

    pub fn missing_runtime(&self) -> Vec<PathBuf> {
        let mut missing = self.config.layout.missing_components();
        if self.config.dxvk && !self.config.layout.dxvk().is_dir() {
            missing.push(self.config.layout.dxvk());
        }
        if self.config.extras.vkd3d_version == "VKD3D" && !self.config.layout.vkd3d().is_dir() {
            missing.push(self.config.layout.vkd3d());
        }
        if self.config.audio == AudioBackend::Pulse && !self.config.layout.pulseaudio().is_file() {
            missing.push(self.config.layout.pulseaudio());
        }
        missing
    }

    pub fn prepare_audio(&self) -> Result<(), SessionError> {
        if self.config.audio != AudioBackend::Pulse {
            return Ok(());
        }
        let dir = self.config.container.join("pulseaudio");
        std::fs::create_dir_all(&dir).map_err(|_| SessionError::Io)?;
        let socket = self.config.container.join("tmp/.sound/PS0");
        let config = format!(
            "load-module module-native-protocol-unix auth-anonymous=1 auth-cookie-enabled=0 socket=\"{}\"\nload-module module-aaudio-sink\nset-default-sink AAudio_sink\n",
            socket.display()
        );
        std::fs::write(dir.join("default.pa"), config).map_err(|_| SessionError::Io)?;
        Ok(())
    }

    pub fn audio_command(&self) -> Option<CommandSpec> {
        if self.config.audio != AudioBackend::Pulse {
            return None;
        }
        let dir = self.config.container.join("pulseaudio");
        let config = dir.join("default.pa");
        let mut env = self.command.env.clone();
        set_env(&mut env, "HOME", path_string(&dir));
        set_env(
            &mut env,
            "LD_LIBRARY_PATH",
            format!(
                "{}:{}:/system/lib64",
                self.config.layout.rootfs().join("lib").display(),
                self.config
                    .layout
                    .rootfs()
                    .join("lib/pulseaudio/modules")
                    .display()
            ),
        );
        let args = vec![
            "--system=false".to_string(),
            "--fail=false".to_string(),
            "-n".to_string(),
            format!("--file={}", config.display()),
            "--daemonize=false".to_string(),
            "--use-pid-file=false".to_string(),
            "--exit-idle-time=-1".to_string(),
        ];
        let mut command =
            proot_command(&self.config, &env, &self.config.layout.pulseaudio(), &args);
        command.cwd = dir;
        Some(command)
    }

    pub fn bootstrap_command(&self, step: &str) -> Option<CommandSpec> {
        let env = self.command.env.clone();
        match step {
            "wineboot" => Some(proot_command(
                &self.config,
                &env,
                &self.config.layout.wine(),
                &["wineboot".to_string()],
            )),
            "wineserver-stop" => Some(proot_command(
                &self.config,
                &env,
                &self.config.layout.wineserver(),
                &["-k".to_string()],
            )),
            "wineserver-wait" => Some(proot_command(
                &self.config,
                &env,
                &self.config.layout.wineserver(),
                &["-w".to_string()],
            )),
            "install-vkd3d" => {
                if self.config.extras.vkd3d_version != "VKD3D" {
                    return None;
                }
                // VKD3D is a drop-in DLL component: run `wineserver -w` so any
                // in-flight prefix initialization flushes before the DLLs are
                // merged into drive_c/windows. The merge itself is performed by
                // `gamehubx_runtime::apply_dxvk`-style component copy.
                Some(proot_command(
                    &self.config,
                    &env,
                    &self.config.layout.wineserver(),
                    &["-w".to_string()],
                ))
            }
            "apply-registry" | "write-config" | "set-win64" => {
                // Prefix setup steps executed through wineboot; a real command
                // is emitted so the bootstrap pipeline is not a silent no-op.
                Some(proot_command(
                    &self.config,
                    &env,
                    &self.config.layout.wine(),
                    &["wineboot".to_string()],
                ))
            }
            _ => None,
        }
    }
}

/// Serialize the per-game settings that the Winlator bridge maps onto its
/// container/shortcut (W4). Plain JSON using serde; no GameHub endpoints.
pub fn session_settings_json(settings: &gamehubx_core::GameSettings) -> String {
    let value = serde_json::json!({
        "width": settings.width,
        "height": settings.height,
        "fps_cap": settings.fps_cap,
        "dxvk": settings.dxvk_version != "Default",
        "vkd3d": settings.vkd3d_version == "VKD3D",
        "audio_backend": settings.audio_backend,
        "mangohud": settings.mangohud,
        "directx_hud": settings.directx_hud,
        "command_line": settings.command_line,
        "env_vars": settings.env_vars,
        "wine_debug_params": settings.wine_debug_params,
        "disable_wm": settings.disable_wm,
        "cpu_core_limit": settings.cpu_core_limit,
    });
    value.to_string()
}

/// Split a command-line string into guest arguments with simple quote support.
fn split_command_line(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut has_token = false;
    for ch in text.chars() {
        match ch {
            '"' | '\'' => {
                if quote == Some(ch) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(ch);
                    has_token = true;
                } else {
                    current.push(ch);
                }
            }
            c if c.is_whitespace() && quote.is_none() => {
                if has_token {
                    args.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            c => {
                current.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        args.push(current);
    }
    args
}

fn build_env(config: &SessionConfig, display: &DisplayPlan) -> Vec<(String, String)> {
    let home = config.container.join("home/steamuser");
    let tmp = config.container.join("tmp");
    let wine_bin = config.layout.component_dir("wine").join("bin");
    let path = format!("{}:/usr/bin:/bin", wine_bin.display());
    let mut env = vec![
        ("WINEPREFIX".into(), path_string(&config.container)),
        ("WINEDEBUG".into(), "-all".into()),
        ("WINEESYNC".into(), "1".into()),
        ("WINELOADERNOEXEC".into(), "1".into()),
        ("WINE_DISABLE_FULLSCREEN_HACK".into(), "1".into()),
        ("DISPLAY".into(), display.display.clone()),
        ("DXVK_FRAME_RATE".into(), config.fps_cap.to_string()),
        ("BOX64_LOG".into(), "0".into()),
        ("BOX64_NOBANNER".into(), "1".into()),
        ("HOME".into(), path_string(&home)),
        ("USER".into(), "steamuser".into()),
        ("TMPDIR".into(), path_string(&tmp)),
        ("XDG_RUNTIME_DIR".into(), path_string(&tmp)),
        ("PATH".into(), path),
        ("WM_SERVER_PATH".into(), path_string(&display.wm_server)),
        ("PERF_SERVER_PATH".into(), path_string(&display.perf_server)),
        (
            "ANDROID_SYSVSHM_SERVER".into(),
            path_string(&display.shm_server),
        ),
    ];

    match config.audio {
        AudioBackend::Pulse => {
            env.push((
                "PULSE_SERVER".into(),
                path_string(&config.container.join("tmp/.sound/PS0")),
            ));
            env.push(("PULSE_LATENCY_MSEC".into(), "60".into()));
        }
        AudioBackend::Alsa => {
            let rootfs = config.layout.rootfs();
            env.push((
                "ALSA_CONFIG_PATH".into(),
                path_string(&rootfs.join("etc/alsa.conf")),
            ));
            env.push((
                "ALSA_PLUGIN_DIR".into(),
                path_string(&rootfs.join("lib/alsa-lib")),
            ));
            env.push((
                "ANDROID_ALSA_SERVER".into(),
                path_string(&config.container.join("tmp/.sound/AS0")),
            ));
            env.push(("ANDROID_ASERVER_USE_SHM".into(), "true".into()));
        }
    }

    // Extended settings now take effect in the launched environment.
    let extras = &config.extras;
    set_env(&mut env, "WINEDEBUG", extras.wine_debug_params.clone());
    set_env(
        &mut env,
        "LANG",
        locale_for(&extras.container_language).to_string(),
    );
    if extras.mangohud {
        set_env(&mut env, "MANGOHUD", "1".into());
    }
    let hud = match extras.directx_hud.as_str() {
        "FPS" => "fps",
        "Full" => "full",
        _ => "0",
    };
    set_env(&mut env, "DXVK_HUD", hud.into());
    if extras.disable_wm {
        set_env(&mut env, "WINEMU_DISABLE_WM", "1".into());
    }
    if extras.auto_keyboard {
        set_env(&mut env, "WINEMU_AUTO_KEYBOARD", "1".into());
    }
    if extras.disable_gfx_plugin {
        set_env(&mut env, "WINEMU_DISABLE_GFX_PLUGIN", "1".into());
    }
    if extras.log_server {
        set_env(&mut env, "WINEMU_ENABLE_LOG_SERVER", "1".into());
    }
    if extras.cpu_core_limit > 0 {
        set_env(
            &mut env,
            "WINEMU_CPU_CORE_LIMIT",
            extras.cpu_core_limit.to_string(),
        );
    }
    match extras.gpu_driver.as_str() {
        "Turnip" => {
            set_env(&mut env, "MESA_LOADER_DRIVER_OVERRIDE", "zink".into());
            set_env(&mut env, "TU_DEBUG", "noconform".into());
        }
        "VirGL" => {
            set_env(&mut env, "MESA_LOADER_DRIVER_OVERRIDE", "virgl".into());
        }
        _ => {}
    }
    if extras.surface_format == "BGRA8" || extras.surface_format == "RGB565" {
        set_env(
            &mut env,
            "WINEMU_SURFACE_FORMAT",
            extras.surface_format.clone(),
        );
    }
    if extras.vkd3d_version == "VKD3D" {
        set_env(&mut env, "VKD3D_CONFIG", "dxvk".into());
    }
    match extras.translation_params.as_str() {
        "Box64 Optimized" => {
            set_env(&mut env, "BOX64_DYNAREC", "1".into());
            set_env(&mut env, "BOX64_DYNAREC_FASTNAN", "1".into());
        }
        "FEX Compatible" => {
            set_env(&mut env, "BOX64_DYNAREC", "0".into());
        }
        "Conservative" => {
            set_env(&mut env, "BOX64_DYNAREC", "0".into());
            set_env(&mut env, "BOX64_LOG", "1".into());
        }
        _ => {}
    }
    if extras.dinput_library == "Disabled" {
        set_env(&mut env, "WINEMU_DINPUT", "native".into());
    }
    // Custom env vars are applied last so the user can override defaults.
    for (key, value) in &extras.env_vars {
        if !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            set_env(&mut env, key.as_str(), value.clone());
        }
    }
    env
}

fn locale_for(language: &str) -> &'static str {
    match language {
        "zh_CN" => "zh_CN.UTF-8",
        "ja_JP" => "ja_JP.UTF-8",
        "ru_RU" => "ru_RU.UTF-8",
        _ => "en_US.UTF-8",
    }
}

fn proot_command(
    config: &SessionConfig,
    env: &[(String, String)],
    guest_program: &Path,
    guest_args: &[String],
) -> CommandSpec {
    let bind = |path: &Path| format!("{}:{}", path.display(), path.display());
    let mut args = vec![
        "-0".to_string(),
        "-r".to_string(),
        path_string(&config.layout.rootfs()),
        "-b".to_string(),
        bind(&config.game_dir),
        "-b".to_string(),
        bind(&config.container),
        "-b".to_string(),
        bind(&config.layout.root),
        "-w".to_string(),
        path_string(&config.game_dir),
        path_string(guest_program),
    ];
    args.extend_from_slice(guest_args);
    CommandSpec {
        program: config.layout.proot(),
        args,
        env: env.to_vec(),
        cwd: config.game_dir.clone(),
        memory_limit_bytes: None,
        cpu_affinity: None,
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn set_env(env: &mut Vec<(String, String)>, key: &str, value: String) {
    env.retain(|(name, _)| name != key);
    env.push((key.to_string(), value));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(name: &str) -> SessionConfig {
        let root =
            std::env::temp_dir().join(format!("gamehubx-session-{}-{name}", std::process::id()));
        let game_dir = root.join("games/demo");
        let exe = game_dir.join("game.exe");
        SessionConfig {
            layout: RuntimeLayout::new(root.join("runtime")),
            container: root.join("containers/abc"),
            game_dir,
            exe,
            fps_cap: 60,
            width: 1280,
            height: 720,
            audio: AudioBackend::Pulse,
            dxvk: false,
            memory_limit_mb: 0,
            cpu_affinity: None,
            extras: SessionExtras::default(),
        }
    }

    fn value_after(args: &[String], flag: &str) -> Option<String> {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|index| args.get(index + 1))
            .cloned()
    }

    #[test]
    fn builds_proot_wine_command() {
        let config = config("command");
        let game_dir = config.game_dir.clone();
        let container = config.container.clone();
        let exe = config.exe.clone();
        let plan = SessionPlan::new(config).unwrap();

        assert!(plan.command.program.ends_with("proot/proot"));
        assert_eq!(
            value_after(&plan.command.args, "-r"),
            Some(plan.config().layout.rootfs().display().to_string())
        );
        assert_eq!(
            value_after(&plan.command.args, "-w"),
            Some(game_dir.display().to_string())
        );
        assert!(plan.command.args.contains(&format!(
            "{}:{}",
            container.display(),
            container.display()
        )));
        assert_eq!(
            plan.command.args[plan.command.args.len() - 2],
            path_string(&plan.config().layout.wine())
        );
        assert_eq!(plan.command.args.last(), Some(&path_string(&exe)));
        assert_eq!(plan.command.cwd, game_dir);
        assert_eq!(
            plan.command.env_value("WINEPREFIX"),
            Some(container.to_str().unwrap())
        );
        assert_eq!(plan.command.env_value("DISPLAY"), Some(":0"));
        assert_eq!(plan.command.env_value("DXVK_FRAME_RATE"), Some("60"));
        assert_eq!(plan.command.env_value("WINEESYNC"), Some("1"));
        assert_eq!(plan.command.env_value("BOX64_NOBANNER"), Some("1"));
        assert_eq!(plan.command.env_value("PULSE_LATENCY_MSEC"), Some("60"));
    }

    #[test]
    fn alsa_audio_uses_rootfs_paths() {
        let mut config = config("alsa");
        config.audio = AudioBackend::Alsa;
        let rootfs = config.layout.rootfs();
        let container = config.container.clone();
        let plan = SessionPlan::new(config).unwrap();
        assert_eq!(
            plan.command.env_value("ALSA_CONFIG_PATH"),
            Some(rootfs.join("etc/alsa.conf").to_str().unwrap())
        );
        assert_eq!(
            plan.command.env_value("ALSA_PLUGIN_DIR"),
            Some(rootfs.join("lib/alsa-lib").to_str().unwrap())
        );
        assert_eq!(
            plan.command.env_value("ANDROID_ALSA_SERVER"),
            Some(container.join("tmp/.sound/AS0").to_str().unwrap())
        );
        assert_eq!(
            plan.command.env_value("ANDROID_ASERVER_USE_SHM"),
            Some("true")
        );
        assert_eq!(plan.command.env_value("PULSE_SERVER"), None);
    }

    #[test]
    fn display_plan_uses_container_sockets() {
        let config = config("display");
        let container = config.container.clone();
        let plan = SessionPlan::new(config).unwrap();
        assert_eq!(plan.display.display, ":0");
        assert_eq!(plan.display.width, 1280);
        assert_eq!(plan.display.height, 720);
        assert_eq!(plan.display.wm_server, container.join("tmp/.wm/V0"));
        assert_eq!(plan.display.shm_server, container.join("tmp/.sysvshm/SM0"));
    }

    #[test]
    fn missing_runtime_lists_components() {
        let plan = SessionPlan::new(config("missing")).unwrap();
        assert_eq!(plan.missing_runtime().len(), 6);
    }

    #[test]
    fn memory_and_affinity_are_applied() {
        let mut config = config("limits");
        config.memory_limit_mb = 3072;
        config.cpu_affinity = Some(0b1111_0000);
        let plan = SessionPlan::new(config).unwrap();
        assert_eq!(plan.command.memory_limit_bytes, Some(3072 * 1024 * 1024));
        assert_eq!(plan.command.cpu_affinity, Some(0b1111_0000));
    }

    #[test]
    fn dxvk_selection_is_required_when_enabled() {
        let mut config = config("dxvk");
        config.dxvk = true;
        let plan = SessionPlan::new(config).unwrap();
        assert!(plan
            .missing_runtime()
            .iter()
            .any(|path| path.ends_with("dxvk")));
    }

    #[test]
    fn pulse_audio_config_and_command() {
        let plan = SessionPlan::new(config("audio")).unwrap();
        let container = plan.config().container.clone();
        plan.prepare_audio().unwrap();
        let text = std::fs::read_to_string(container.join("pulseaudio/default.pa")).unwrap();
        assert!(text.contains("module-aaudio-sink"));
        assert!(text.contains("module-native-protocol-unix"));

        let command = plan.audio_command().unwrap();
        assert!(command.program.ends_with("proot/proot"));
        assert!(command
            .args
            .iter()
            .any(|arg| arg.ends_with("bin/pulseaudio")));
        assert!(command.args.contains(&"--daemonize=false".to_string()));
        assert_eq!(command.cwd, container.join("pulseaudio"));
        assert!(command
            .env_value("LD_LIBRARY_PATH")
            .unwrap()
            .contains("/lib/pulseaudio/modules"));
        let _ = std::fs::remove_dir_all(container.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn alsa_has_no_audio_command() {
        let mut config = config("alsa-cmd");
        config.audio = AudioBackend::Alsa;
        let plan = SessionPlan::new(config).unwrap();
        assert!(plan.audio_command().is_none());
    }

    #[test]
    fn bootstrap_commands_are_scoped() {
        let plan = SessionPlan::new(config("boot")).unwrap();
        let wineboot = plan.bootstrap_command("wineboot").unwrap();
        assert_eq!(
            wineboot.args[wineboot.args.len() - 2],
            path_string(&plan.config().layout.wine())
        );
        assert_eq!(wineboot.args.last().map(String::as_str), Some("wineboot"));
        assert!(plan.bootstrap_command("create-prefix").is_none());

        let stop = plan.bootstrap_command("wineserver-stop").unwrap();
        assert_eq!(stop.args.last().map(String::as_str), Some("-k"));
    }

    #[test]
    fn extended_settings_are_applied_to_env_and_command() {
        let mut cfg = config("extras");
        cfg.extras.mangohud = true;
        cfg.extras.directx_hud = "Full".into();
        cfg.extras.container_language = "ja_JP".into();
        cfg.extras.disable_wm = true;
        cfg.extras.gpu_driver = "Turnip".into();
        cfg.extras.command_line = "-windowed -dx11".into();
        cfg.extras.env_vars.push(("CUSTOM_FLAG".into(), "7".into()));
        cfg.extras.env_vars.push(("BAD KEY".into(), "x".into()));
        let plan = SessionPlan::new(cfg).unwrap();
        assert_eq!(plan.command.env_value("MANGOHUD"), Some("1"));
        assert_eq!(plan.command.env_value("DXVK_HUD"), Some("full"));
        assert_eq!(plan.command.env_value("LANG"), Some("ja_JP.UTF-8"));
        assert_eq!(plan.command.env_value("WINEMU_DISABLE_WM"), Some("1"));
        assert_eq!(
            plan.command.env_value("MESA_LOADER_DRIVER_OVERRIDE"),
            Some("zink")
        );
        assert_eq!(plan.command.env_value("CUSTOM_FLAG"), Some("7"));
        assert_eq!(plan.command.env_value("BAD KEY"), None);
        assert_eq!(plan.command.args.last().map(String::as_str), Some("-dx11"));
        assert!(plan.command.args.iter().any(|a| a == "-windowed"));
    }

    #[test]
    fn session_settings_json_carries_w4_fields() {
        use gamehubx_core::GameSettings;
        let mut settings = GameSettings::default();
        settings.width = 1920;
        settings.height = 1080;
        settings.fps_cap = 120;
        settings.dxvk_version = "DXVK".into();
        settings.vkd3d_version = "VKD3D".into();
        settings.audio_backend = "Pulse".into();
        settings.command_line = "-windowed".into();
        settings.env_vars.push(("CUSTOM".into(), "1".into()));
        let json = session_settings_json(&settings);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["width"], 1920);
        assert_eq!(value["fps_cap"], 120);
        assert_eq!(value["dxvk"], true);
        assert_eq!(value["vkd3d"], true);
        assert_eq!(value["audio_backend"], "Pulse");
        assert_eq!(value["command_line"], "-windowed");
        assert_eq!(value["env_vars"][0][0], "CUSTOM");
    }

    #[test]
    fn vkd3d_bootstrap_is_not_a_noop_when_enabled() {
        let mut cfg = config("vkd3d");
        assert!(SessionPlan::new(cfg.clone())
            .unwrap()
            .bootstrap_command("install-vkd3d")
            .is_none());
        cfg.extras.vkd3d_version = "VKD3D".into();
        let plan = SessionPlan::new(cfg).unwrap();
        assert!(plan.missing_runtime().iter().any(|p| p.ends_with("vkd3d")));
        assert_eq!(plan.command.env_value("VKD3D_CONFIG"), Some("dxvk"));
        assert!(plan.bootstrap_command("install-vkd3d").is_some());
    }

    #[test]
    fn paths_with_spaces_stay_single_arguments() {
        let mut config = config("spaces");
        config.exe = config.game_dir.join("My Game.exe");
        let expected = path_string(&config.exe);
        let plan = SessionPlan::new(config).unwrap();
        assert_eq!(plan.command.args.last(), Some(&expected));
    }

    #[test]
    fn rejects_invalid_sessions() {
        let mut relative = config("relative");
        relative.game_dir = PathBuf::from("games/demo");
        assert_eq!(
            SessionPlan::new(relative).unwrap_err(),
            SessionError::NotAbsolute
        );

        let mut outside = config("outside");
        outside.exe = outside.game_dir.join("..").join("other.exe");
        assert_eq!(
            SessionPlan::new(outside).unwrap_err(),
            SessionError::ExeOutsideGameDir
        );

        let mut fps = config("fps");
        fps.fps_cap = 0;
        assert_eq!(SessionPlan::new(fps).unwrap_err(), SessionError::InvalidFps);

        let mut colon = config("colon");
        colon.game_dir = PathBuf::from("/games/a:b");
        colon.exe = colon.game_dir.join("game.exe");
        assert_eq!(
            SessionPlan::new(colon).unwrap_err(),
            SessionError::UnsupportedPath
        );
    }
}
