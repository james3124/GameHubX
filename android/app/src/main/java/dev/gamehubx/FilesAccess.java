package dev.gamehubx;

import android.content.Context;
import android.content.Intent;
import android.net.Uri;
import android.os.Build;
import android.os.Environment;
import android.os.Handler;
import android.os.Looper;
import android.provider.Settings;

public final class FilesAccess {
    private FilesAccess() {
    }

    public static boolean granted(Context context) {
        return Build.VERSION.SDK_INT < Build.VERSION_CODES.R
                || Environment.isExternalStorageManager();
    }

    public static void request(Context context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
            return;
        }
        Intent intent = new Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION);
        intent.setData(Uri.parse("package:" + context.getPackageName()));
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        new Handler(Looper.getMainLooper()).post(() -> context.startActivity(intent));
    }
}
