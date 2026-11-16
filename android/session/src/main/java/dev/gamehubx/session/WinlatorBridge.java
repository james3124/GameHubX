package dev.gamehubx.session;

import android.content.Context;
import android.content.Intent;
import android.os.Handler;
import android.os.Looper;

import com.winlator.cmod.container.Container;
import com.winlator.cmod.container.ContainerManager;
import com.winlator.cmod.xenvironment.ImageFs;

import java.io.File;
import java.io.FileWriter;
import java.io.PrintWriter;
import java.util.List;

/**
 * Adapter from the GameHubX launcher to Winlator's container/shortcut launch path.
 *
 * <p>W3 uses the container Winlator's onboarding created. Per-game containers and
 * settings mapping are follow-up work.</p>
 */
public final class WinlatorBridge {
    private static final String ACTIVITY = "com.winlator.cmod.XServerDisplayActivity";
    private static final String SHORTCUT_PREFIX = "gamehubx-";

    private WinlatorBridge() {
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
            File shortcut = writeShortcut(context, imageFs, container, exePath, name);
            Intent intent = new Intent();
            intent.setClassName(context, ACTIVITY);
            intent.putExtra("container_id", container.id);
            intent.putExtra("shortcut_path", shortcut.getAbsolutePath());
            intent.putExtra("shortcut_name", name);
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            new Handler(Looper.getMainLooper()).post(() -> context.startActivity(intent));
            return null;
        } catch (Exception error) {
            return "Could not launch Winlator: " + error.getMessage();
        }
    }

    private static File writeShortcut(
            Context context,
            ImageFs imageFs,
            Container container,
            String exePath,
            String name
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
        try (PrintWriter writer = new PrintWriter(new FileWriter(shortcut))) {
            writer.println("[Desktop Entry]");
            writer.println("Name=" + name);
            writer.println("Exec=env WINEPREFIX=\"" + imageFs.wineprefix + "\" wine \"" + exePath + "\"");
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
}
