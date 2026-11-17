package dev.gamehubx;

import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ShortcutManager;
import android.os.Build;

import androidx.core.content.pm.ShortcutInfoCompat;
import androidx.core.content.pm.ShortcutManagerCompat;

public final class HomeShortcut {
    private static final String ID_PREFIX = "gamehubx-";

    private HomeShortcut() {
    }

    /**
     * Whether this device/launcher accepts pinned home-screen shortcuts.
     * Kept defensive so the launcher never crashes on a weird OEM build.
     */
    public static boolean supported(Context ctx) {
        try {
            if (ctx == null || Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
                return false;
            }
            ShortcutManager manager =
                    (ShortcutManager) ctx.getSystemService(Context.SHORTCUT_SERVICE);
            return manager != null && manager.isRequestPinShortcutSupported();
        } catch (Throwable ignored) {
            return false;
        }
    }

    /**
     * Ask the launcher to pin a shortcut that reopens the given game.
     */
    public static boolean pin(Context ctx, String exePath, String label) {
        if (ctx == null || exePath == null) {
            return false;
        }
        try {
            if (!validPath(exePath) || !supported(ctx)) {
                return false;
            }

            String safeLabel = sanitizeLabel(label, exePath);
            String id = ID_PREFIX + Integer.toHexString(exePath.hashCode());
            ComponentName target = new ComponentName(ctx, GameHubXActivity.class);

            Intent launch = new Intent(Intent.ACTION_MAIN);
            launch.setComponent(target);
            launch.addCategory(Intent.CATEGORY_LAUNCHER);
            launch.putExtra("pin_launch_exe", exePath);
            launch.putExtra("pin_launch_name", safeLabel);

            ShortcutInfoCompat shortcut = new ShortcutInfoCompat.Builder(ctx, id)
                    .setShortLabel(safeLabel)
                    .setLongLabel(safeLabel)
                    .setActivity(target)
                    .setIntent(launch)
                    .build();

            return ShortcutManagerCompat.requestPinShortcut(ctx, shortcut, null);
        } catch (Throwable ignored) {
            return false;
        }
    }

    private static boolean validPath(String exePath) {
        if (exePath.isEmpty()) {
            return false;
        }
        for (int i = 0; i < exePath.length(); i++) {
            char c = exePath.charAt(i);
            if (c == '\0' || c == '\n' || c == '\r') {
                return false;
            }
        }
        return true;
    }

    private static String sanitizeLabel(String label, String exePath) {
        String cleaned = label == null ? "" : stripControlChars(label).trim();
        if (cleaned.isEmpty()) {
            cleaned = fileName(exePath);
        }
        return cleaned.isEmpty() ? "Game" : cleaned;
    }

    private static String stripControlChars(String value) {
        StringBuilder out = new StringBuilder(value.length());
        for (int i = 0; i < value.length(); i++) {
            char c = value.charAt(i);
            if (c >= 0x20 && c != 0x7f) {
                out.append(c);
            }
        }
        return out.toString();
    }

    private static String fileName(String path) {
        int slash = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
        String name = slash >= 0 ? path.substring(slash + 1) : path;
        return stripControlChars(name).trim();
    }
}
