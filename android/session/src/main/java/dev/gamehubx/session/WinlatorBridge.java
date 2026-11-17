package dev.gamehubx.session;

import android.content.Context;
import android.content.Intent;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;

import com.winlator.cmod.container.Container;
import com.winlator.cmod.container.ContainerManager;
import com.winlator.cmod.core.EnvVars;
import com.winlator.cmod.xenvironment.ImageFs;

import org.json.JSONObject;

import java.io.File;
import java.io.FileWriter;
import java.io.PrintWriter;
import java.util.List;

/**
 * Adapter from the GameHubX launcher to Winlator's container/shortcut launch path.
 *
 * <p>W3 uses the container Winlator's onboarding created. W4 maps the launcher's
 * per-game settings (resolution, FPS, DXVK/VKD3D, audio, custom env vars,
 * command line) onto that container and the temporary .desktop shortcut before
 * starting {@code XServerDisplayActivity}.</p>
 */
public final class WinlatorBridge {
    private static final String TAG = "GHX-WinlatorBridge";
    private static final String ACTIVITY = "com.winlator.cmod.XServerDisplayActivity";
    private static final String SHORTCUT_PREFIX = "gamehubx-";

    /** Last settings JSON received from the launcher (survives the asset download step). */
    private static volatile String settingsJson = "";

    private WinlatorBridge() {
    }

    /** Store the per-game settings JSON forwarded from the Rust launcher. */
    public static void setSettings(String json) {
        settingsJson = json == null ? "" : json;
    }

    /**
     * @return {@code null} when the session activity was started, otherwise an
     *         error message suitable for the launcher UI.
     */
    public static String launch(Context context, String exePath, String gameName) {
        if (context == null) {
            return "No Android activity is attached";
        }
        if (exePath == null || exePath.isEmpty()) {
            return "Startup file path is not set";
        }
        if (!validPath(exePath)) {
            return "Startup path contains characters that cannot be passed to Wine";
        }

        ImageFs imageFs = ImageFs.find(context);
        if (!imageFs.isValid()) {
            return "Winlator is not set up yet. Open Winlator Session from Settings and finish setup.";
        }

        ContainerManager manager = new ContainerManager(context);
        List<Container> containers = manager.getContainers();
        if (containers.isEmpty()) {
            return "No Winlator container exists. Open Winlator Session from Settings and create one.";
        }
        Container container = containers.get(0);

        File exe = new File(exePath);
        if (!exe.isFile()) {
            return "Game executable was not found: " + exePath;
        }

        String name = displayName(gameName, exe);
        try {
            Settings settings = Settings.parse(settingsJson);
            applySettings(container, settings);
            container.saveData();
            File shortcut = writeShortcut(context, imageFs, container, exePath, name, settings);
            Intent intent = new Intent();
            intent.setClassName(context, ACTIVITY);
            intent.putExtra("container_id", container.id);
            intent.putExtra("shortcut_path", shortcut.getAbsolutePath());
            intent.putExtra("shortcut_name", name);
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            new Handler(Looper.getMainLooper()).post(() -> context.startActivity(intent));
            return null;
        } catch (Exception error) {
            Log.e(TAG, "launch failed", error);
            return "Could not launch Winlator: " + error.getMessage();
        }
    }

    /** Apply launcher settings onto the Winlator container before launch. */
    private static void applySettings(Container container, Settings s) {
        if (s.width > 0 && s.height > 0) {
            container.setScreenSize(s.width + "x" + s.height);
        }
        if (s.audioBackend != null && !s.audioBackend.isEmpty()) {
            // Launcher uses "Pulse"/"Alsa"; Winlator expects its own identifiers.
            String audio = s.audioBackend.equalsIgnoreCase("Pulse") ? "pulse-audio-gn" : "alsa-oboe";
            container.setAudioDriver(audio);
        }

        // DXVK / VKD3D selection maps onto Winlator's dxwrapper value.
        String dxwrapper = container.getDXWrapper();
        if (s.dxvk && dxwrapper.equalsIgnoreCase("wine-d3d9")) {
            container.setDXWrapper("dxvk");
        } else if (!s.dxvk && (dxwrapper.equalsIgnoreCase("dxvk") || dxwrapper.equalsIgnoreCase("dxvk+vkd3d"))) {
            container.setDXWrapper("wine-d3d9");
        } else if (s.vkd3d && "dxvk".equalsIgnoreCase(container.getDXWrapper())) {
            container.setDXWrapper("dxvk+vkd3d");
        }

        // Merge env vars: start from the container defaults, then overlay the
        // settings-driven values and the user's custom variables.
        EnvVars env = new EnvVars(container.getEnvVars());
        if (s.fpsCap > 0) {
            env.put("DXVK_FRAME_RATE", s.fpsCap);
        }
        if (s.mangohud) {
            env.put("MANGOHUD", 1);
        }
        if (s.directxHud != null && !"Disable".equalsIgnoreCase(s.directxHud)) {
            env.put("DXVK_HUD", "Full".equalsIgnoreCase(s.directxHud) ? "full" : "fps");
        }
        if (s.wineDebug != null && !s.wineDebug.isEmpty()) {
            env.put("WINEDEBUG", s.wineDebug);
        }
        if (s.disableWm) {
            env.put("WINEMU_DISABLE_WM", 1);
        }
        if (s.cpuCoreLimit > 0) {
            env.put("WINEMU_CPU_CORE_LIMIT", s.cpuCoreLimit);
        }
        if (s.envVars != null) {
            for (String[] pair : s.envVars) {
                env.put(pair[0], pair[1]);
            }
        }
        container.setEnvVars(env.toString());
    }

    private static File writeShortcut(
            Context context,
            ImageFs imageFs,
            Container container,
            String exePath,
            String name,
            Settings s
    ) throws Exception {
        File cache = context.getCacheDir();
        File[] stale = cache.listFiles((dir, file) ->
                file.startsWith(SHORTCUT_PREFIX) && file.endsWith(".desktop"));
        if (stale != null) {
            for (File file : stale) {
                if (!file.delete()) {
                    break;
                }
            }
        }

        File shortcut = new File(cache, SHORTCUT_PREFIX + System.nanoTime() + ".desktop");
        String exec = "env WINEPREFIX=\"" + imageFs.wineprefix + "\" wine \"" + exePath + "\"";
        if (s.commandLine != null && !s.commandLine.trim().isEmpty()) {
            exec += " " + s.commandLine.trim();
        }
        try (PrintWriter writer = new PrintWriter(new FileWriter(shortcut))) {
            writer.println("[Desktop Entry]");
            writer.println("Name=" + name);
            writer.println("Exec=" + exec);
            writer.println("Type=Application");
            writer.println("container_id:" + container.id);
        }
        return shortcut;
    }

    private static String displayName(String gameName, File exe) {
        String name = gameName == null ? "" : gameName;
        StringBuilder clean = new StringBuilder(name.length());
        for (int i = 0; i < name.length(); i++) {
            char c = name.charAt(i);
            clean.append(c < 0x20 || c == 0x7f ? ' ' : c);
        }
        String result = clean.toString().trim();
        return result.isEmpty() ? exe.getName() : result;
    }

    private static boolean validPath(String path) {
        for (int i = 0; i < path.length(); i++) {
            char c = path.charAt(i);
            if (c == '"' || c == '\n' || c == '\r' || c == 0) {
                return false;
            }
        }
        return true;
    }

    /** Parsed subset of the launcher's settings JSON. Missing fields keep Winlator defaults. */
    private static final class Settings {
        int width;
        int height;
        int fpsCap;
        boolean dxvk;
        boolean vkd3d;
        String audioBackend;
        boolean mangohud;
        String directxHud;
        String commandLine;
        String wineDebug;
        boolean disableWm;
        int cpuCoreLimit;
        java.util.List<String[]> envVars = new java.util.ArrayList<>();

        static Settings parse(String json) {
            Settings s = new Settings();
            if (json == null || json.trim().isEmpty()) {
                return s;
            }
            try {
                JSONObject o = new JSONObject(json);
                s.width = o.optInt("width", 0);
                s.height = o.optInt("height", 0);
                s.fpsCap = o.optInt("fps_cap", 0);
                s.dxvk = o.optBoolean("dxvk", false);
                s.vkd3d = o.optBoolean("vkd3d", false);
                s.audioBackend = o.optString("audio_backend", "");
                s.mangohud = o.optBoolean("mangohud", false);
                s.directxHud = o.optString("directx_hud", "Disable");
                s.commandLine = o.optString("command_line", "");
                s.wineDebug = o.optString("wine_debug_params", "");
                s.disableWm = o.optBoolean("disable_wm", false);
                s.cpuCoreLimit = o.optInt("cpu_core_limit", 0);
                org.json.JSONArray vars = o.optJSONArray("env_vars");
                if (vars != null) {
                    for (int i = 0; i < vars.length(); i++) {
                        org.json.JSONArray pair = vars.optJSONArray(i);
                        if (pair != null && pair.length() >= 2) {
                            String key = pair.optString(0, "").trim();
                            if (!key.isEmpty() && key.matches("[A-Za-z0-9_]+")) {
                                s.envVars.add(new String[]{key, pair.optString(1, "")});
                            }
                        }
                    }
                }
            } catch (Exception error) {
                Log.w(TAG, "settings JSON parse failed; using defaults", error);
            }
            return s;
        }
    }
}
