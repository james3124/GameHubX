use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    ZhHans,
}

const LANG_EN: u8 = 0;
const LANG_ZH_HANS: u8 = 1;
static LANG: AtomicU8 = AtomicU8::new(LANG_EN);

pub const CHOICES: [&str; 3] = ["system", "en", "zh-Hans"];

pub fn set_choice(choice: &str) {
    LANG.store(
        match resolve(choice) {
            Lang::En => LANG_EN,
            Lang::ZhHans => LANG_ZH_HANS,
        },
        Ordering::Relaxed,
    );
}

pub fn current() -> Lang {
    match LANG.load(Ordering::Relaxed) {
        LANG_ZH_HANS => Lang::ZhHans,
        _ => Lang::En,
    }
}

pub fn next_choice(choice: &str) -> &'static str {
    let index = CHOICES.iter().position(|item| *item == choice).unwrap_or(0);
    CHOICES[(index + 1) % CHOICES.len()]
}

pub fn choice_label(choice: &str) -> String {
    match choice {
        "system" => {
            let native = match current() {
                Lang::En => "English",
                Lang::ZhHans => "简体中文",
            };
            if current() == Lang::ZhHans {
                format!("跟随系统（{native}）›")
            } else {
                format!("System ({native}) ›")
            }
        }
        "zh-Hans" => "简体中文 ›".into(),
        _ => "English ›".into(),
    }
}

fn resolve(choice: &str) -> Lang {
    match choice {
        "zh-Hans" => Lang::ZhHans,
        "en" => Lang::En,
        _ => detect_system(),
    }
}

fn detect_system() -> Lang {
    let language = gamehubx_platform::system_language();
    if language.to_ascii_lowercase().starts_with("zh") {
        Lang::ZhHans
    } else {
        Lang::En
    }
}

/// Translate a built-in UI string. Unknown strings (game names, errors,
/// formatted values) pass through unchanged.
pub fn tr(text: &str) -> &str {
    if current() == Lang::En {
        return text;
    }
    match text {
        "Dashboard" => "主页",
        "PC Games" => "PC 游戏",
        "Game Settings" => "游戏设置",
        "Touch Controls" => "触控操作",
        "Settings" => "设置",
        "Game Session" => "游戏会话",
        "Runtime" => "运行环境",
        "Pack" => "运行包",
        "Diagnostics" => "诊断",
        "Download Task" => "下载任务",
        "Game Management" => "游戏管理",
        "Home" => "主页",
        "Platforms" => "平台",
        "My Games" => "我的游戏",
        "Local game" => "本地游戏",
        "Local library" => "本地游戏库",
        "Library overview" => "游戏库概览",
        "PC game library" => "PC 游戏库",
        "Queue and history" => "队列与历史",
        "Launcher settings" => "启动器设置",
        "B  Back" => "B  返回",
        "Game" => "游戏",
        "Download" => "下载",
        "Setting" => "设置",
        "Back" => "返回",
        "Back to game" => "返回游戏",
        "Cancel" => "取消",
        "Confirm" => "确认",
        "All" => "全部",
        "About" => "关于",
        "Advanced" => "高级",
        "Check again" => "重新检查",
        "Clear Cache" => "清除缓存",
        "Clear completed" => "清除已完成",
        "Download components" => "下载组件",
        "Winlator Runtime" => "Winlator 运行环境",
        "Imagefs, Proton and session components (about 500 MB)" => {
            "Imagefs、Proton 和会话组件（约 500 MB）"
        }
        "Ready ›" => "已就绪 ›",
        "Open ›" => "打开 ›",
        "Download ›" => "下载 ›",        "Downloads" => "下载",
        "Grant file access" => "授予文件访问",
        "Import PC games" => "导入 PC 游戏",
        "Install runtime pack" => "安装运行包",
        "Open Runtime" => "打开运行环境",
        "PC Game Settings" => "PC 游戏设置",
        "Remove" => "移除",
        "Save as custom" => "保存为自定义",
        "Stop session" => "停止会话",
        "Switch profile" => "切换配置",
        "Uninstall game" => "卸载游戏",
        "Add to Home screen" => "添加到主屏幕",
        "Upload cover image" => "上传封面图片",
        "Enter introduction" => "输入介绍",
        "Winlator Setup" => "Winlator 设置",
        "Start" => "开始",
        "Pause" => "暂停",
        "Waiting" => "等待中",
        "Completed At" => "完成于",
        "Resume" => "继续",
        "Retry" => "重试",
        "Download Soon" => "即将下载",
        "Downloading" => "下载中",
        "Failed" => "失败",
        "Completed" => "已完成",
        "No Queued Tasks" => "无排队任务",
        "No completed downloads" => "无已完成下载",
        "Navigate" => "导航",
        "Select" => "选择",
        "Language" => "语言",
        "Library Database" => "游戏库数据库",
        "All Files Access" => "所有文件访问",
        "Runtime Pack" => "运行包",
        "Winlator Session" => "Winlator 会话",
        "General" => "通用",
        "Compatibility" => "兼容性",
        "Backup / Restore" => "备份 / 恢复",
        "Component" => "组件",
        "Developer Options" => "开发者选项",
        "Enter Container Desktop" => "进入容器桌面",
        "Startup File Path" => "启动文件路径",
        "Game Resolution" => "游戏分辨率",
        "Frame Rate Limit" => "帧率限制",
        "Enable MangoHUD" => "启用 MangoHUD",
        "DirectX HUD" => "DirectX HUD",
        "Disable Window Manager" => "禁用窗口管理器",
        "Environment Variables" => "环境变量",
        "Command Line" => "命令行",
        "Auto-display keyboard when input needed" => "需要输入时自动显示键盘",
        "Disable Graphics Enhancement Plugin" => "禁用图形增强插件",
        "Reset Game Data" => "重置游戏数据",
        "Compatibility Layer" => "兼容层",
        "DXVK Version" => "DXVK 版本",
        "Translation Params" => "转译参数",
        "DInput Library" => "DInput 库",
        "GPU Driver" => "GPU 驱动",
        "Surface Format" => "表面格式",
        "Audio Driver" => "音频驱动",
        "Touch Profile" => "触控配置",
        "Memory Limit" => "内存限制",
        "CPU Affinity" => "CPU 亲和性",
        "CPU Core Limit" => "CPU 核心限制",
        "VKD3D Version" => "VKD3D 版本",
        "Recommended Config" => "推荐配置",
        "Enable Log Server" => "启用日志服务器",
        "Wine Debug Params" => "Wine 调试参数",
        "RTS gesture controls" => "RTS 手势控制",
        "Backup container data" => "备份容器数据",
        "Restore container data" => "恢复容器数据",
        "Choose backup archive" => "选择备份归档",
        "Set cover image" => "设置封面图片",
        "Storage by category" => "按类别存储",
        "Media" => "媒体",
        "Other" => "其他",
        "Launch parameters" => "启动参数",
        "Wine debug params (WINEDEBUG)" => "Wine 调试参数 (WINEDEBUG)",
        "Environment variables (KEY=VALUE per line)" => "环境变量（每行 KEY=VALUE）",
        "Select the Direct3D 12 translation layer" => "选择 Direct3D 12 转译层",
        "Apply a known-good preset bundle" => "应用推荐的配置预设",
        "Maximum number of CPU cores for the session" => "会话最大 CPU 核心数",
        "Enable the in-session HTTP log server" => "启用会话内 HTTP 日志服务器",
        "WINEDEBUG value (e.g. -all, fixme-all)" => "WINEDEBUG 值（如 -all、fixme-all）",
        "Remove download parts and temporary files" => "清除下载的临时文件",
        "Open Touch Controls" => "打开触控操作",
        "Edit layout" => "编辑布局",
        "Edit mode: drag elements to move them." => "编辑模式：拖动元素以移动。",
        "Launch Wine container desktop environment to operate files and programs" => {
            "启动 Wine 容器桌面以操作文件和程序"
        }
        "Select the .exe file path of the game or program to run" => {
            "选择要运行的游戏或程序 .exe 路径"
        }
        "Set container system language and region" => "设置容器系统语言和区域",
        "Set screen resolution when programs are running" => "设置程序运行时的屏幕分辨率",
        "Cap the game's frame rate" => "限制游戏帧率",
        "Enable MangoHUD overlay as a more professional performance dashboard" => {
            "启用 MangoHUD 性能面板"
        }
        "Control performance monitoring HUD display, showing FPS and debug info" => {
            "控制性能监控 HUD 显示"
        }
        "Windows will no longer be decorated and cannot be repositioned" => {
            "窗口将不再装饰且无法移动"
        }
        "Configure container environment variables" => "配置容器环境变量",
        "Add program launch parameters" => "添加程序启动参数",
        "Disable if game has issues" => "若游戏异常可禁用",
        "Clear all container data and restore to initial state" => {
            "清除所有容器数据并恢复初始状态"
        }
        "Select Wine version" => "选择 Wine 版本",
        "Select the Direct3D translation layer" => "选择 Direct3D 转译层",
        "x86-to-ARM64 translation parameters" => "x86 到 ARM64 转译参数",
        "dinput.dll and dinput8.dll priority" => "dinput.dll 与 dinput8.dll 优先级",
        "Select graphics driver type" => "选择图形驱动类型",
        "Select surface color format for rendering" => "选择渲染表面颜色格式",
        "Select audio output driver" => "选择音频输出驱动",
        "On-screen control layout (ICP profiles)" => "屏幕触控布局（ICP 配置）",
        "Preview, drag-edit, save custom ICP profiles, and test the mapping engine. Game injection awaits the P3 display bridge." => {
            "预览、拖动编辑、保存自定义 ICP 配置并测试映射引擎。游戏注入仍需 P3 显示桥。"
        }
        "Address-space limit for the Wine session (RLIMIT_AS)" => {
            "Wine 会话地址空间限制（RLIMIT_AS）"
        }
        "Restrict the session to the fastest cores" => "将会话限制在最快核心",
        "Applied at session spawn; big cores use cpufreq max frequencies." => {
            "在会话启动时应用；大核使用 cpufreq 最高频率。"
        }
        "Install/verify Wine/Box64/PRoot/X-server" => "安装/校验 Wine/Box64/PRoot/X-server",
        "Open the GPLv3 Winlator setup/container launcher" => {
            "打开 GPLv3 Winlator 设置/容器启动器"
        }
        "Required for import" => "导入所需",
        "SQLite library loaded" => "游戏库已加载",
        "No library database" => "无游戏库数据库",
        "Unavailable ›" => "不可用 ›",
        "Granted ›" => "已授予 ›",
        "Not granted ›" => "未授予 ›",
        "Default ›" => "默认 ›",
        "Big cores ›" => "大核 ›",
        "All cores ›" => "所有核心 ›",
        "Unlimited ›" => "无限制 ›",
        "Not set ›" => "未设置 ›",
        "All required components are present." => "所有必需组件已就绪。",
        "Installing..." => "安装中...",
        "Missing:" => "缺少：",
        "Runtime pack is missing these files:" => "运行包缺少以下文件：",
        "Runtime pack incomplete" => "运行包不完整",
        "Runtime pack ready" => "运行包已就绪",
        "Place manifest.json and <component>.tar.gz files in the pack folder, then install." => {
            "将 manifest.json 和 <component>.tar.gz 放入运行包目录后安装。"
        }
        "HTTPS components can be queued from this manifest and installed after they finish." => {
            "HTTPS 组件可从清单排队下载，完成后再安装。"
        }
        "Pulse audio also needs rootfs/bin/pulseaudio; DXVK needs <runtime>/dxvk." => {
            "Pulse 音频还需要 rootfs/bin/pulseaudio；DXVK 需要 <runtime>/dxvk。"
        }
        "All files access is required to browse and import games." => {
            "需要所有文件访问权限才能浏览和导入游戏。"
        }
        "Type a game name to search your library" => "输入游戏名称以搜索游戏库",
        "Search for Game Name" => "搜索游戏名称",
        "No games found" => "未找到游戏",
        "No Games Available" => "暂无游戏",
        "Storage statistics unavailable" => "无法获取存储统计",
        "Popular Recommendations" => "热门推荐",
        "Head over to the Discovery Channel to find your favorite games" => {
            "前往发现频道寻找您喜欢的游戏"
        }
        "No touch profile loaded" => "未加载触控配置",
        "Memory" => "内存",
        // Album / gallery screen.
        "Album" => "相册",
        "Covers" => "封面",
        "Screenshots" => "截图",
        "No images yet" => "暂无图片",
        "Covers and screenshots" => "封面与截图",
        // In-game HUD overlay.
        "GameHubX HUD" => "GameHubX 性能面板",
        "FPS cap" => "帧率上限",
        "PSS --" => "PSS 不可用",
        "Battery" => "电池",
        // Session readouts.
        "Audio PID" => "音频进程 ID",
        "Containers" => "容器",
        "PSS history" => "PSS 历史",
        // Storage per-folder breakdown.
        "Per-folder breakdown" => "各文件夹明细",
        "Folder" => "文件夹",
        "Size" => "大小",
        "Files" => "文件数",
        "No folders found" => "未找到文件夹",
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_and_falls_back() {
        set_choice("zh-Hans");
        assert_eq!(tr("Dashboard"), "主页");
        assert_eq!(tr("Album"), "相册");
        assert_eq!(tr("Per-folder breakdown"), "各文件夹明细");
        assert_eq!(tr("Not a translated game"), "Not a translated game");
        set_choice("en");
        assert_eq!(tr("Dashboard"), "Dashboard");
        assert_eq!(next_choice("system"), "en");
        assert_eq!(next_choice("en"), "zh-Hans");
        assert_eq!(next_choice("zh-Hans"), "system");
    }
}
