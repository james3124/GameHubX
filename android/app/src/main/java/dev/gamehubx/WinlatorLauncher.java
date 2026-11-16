package dev.gamehubx;

import android.content.Context;
import android.content.Intent;
import android.os.Handler;
import android.os.Looper;

public final class WinlatorLauncher {
    private WinlatorLauncher() {
    }

    public static void openMain(Context context) {
        Intent intent = new Intent();
        intent.setClassName(context, "dev.gamehubx.RuntimeAssetsActivity");
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        new Handler(Looper.getMainLooper()).post(() -> context.startActivity(intent));
    }
}
