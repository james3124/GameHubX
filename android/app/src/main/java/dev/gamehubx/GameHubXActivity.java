package dev.gamehubx;

import android.app.NativeActivity;
import android.os.Bundle;
import android.util.Log;

public final class GameHubXActivity extends NativeActivity {
    private static final String TAG = "GameHubX";
    private static final java.util.concurrent.ConcurrentLinkedQueue<Integer> GAMEPAD_ACTIONS =
            new java.util.concurrent.ConcurrentLinkedQueue<>();

    private String pendingShortcutExe;

    static {
        System.loadLibrary("gamehubx_app");
    }

    private static native void nativeSetFilesDir(String path);
    private static native void nativeWake();

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        nativeSetFilesDir(getFilesDir().getAbsolutePath());
        String shortcutExe = getIntent().getStringExtra("pin_launch_exe");
        if (shortcutExe != null) {
            pendingShortcutExe = shortcutExe;
        }
        super.onCreate(savedInstanceState);
        Log.i(TAG, "activity created");
    }

    @Override
    public void onTrimMemory(int level) {
        super.onTrimMemory(level);
        Log.i(TAG, "onTrimMemory level=" + level);
    }

    @Override
    public void onLowMemory() {
        super.onLowMemory();
        Log.w(TAG, "onLowMemory");
    }

    @Override
    public void onBackPressed() {
        GAMEPAD_ACTIONS.offer(6);
        nativeWake();
    }

    @Override
    protected void onResume() {
        super.onResume();
        nativeWake();
    }

    public boolean filesAccessGranted() {
        return FilesAccess.granted(this);
    }

    public void requestFilesAccess() {
        FilesAccess.request(this);
    }

    public void openWinlator() {
        WinlatorLauncher.openMain(this);
    }

    public boolean addHomeShortcut(String exePath, String label) {
        return HomeShortcut.pin(this, exePath, label);
    }

    public String consumeShortcutExe() {
        String exe = pendingShortcutExe;
        pendingShortcutExe = null;
        return exe;
    }

    public String launchWinlator(String exePath, String gameName, String settingsJson) {
        // Keep the settings on the bridge so the post-download launch path
        // (RuntimeAssetsActivity) can reuse them without re-crossing JNI.
        dev.gamehubx.session.WinlatorBridge.setSettings(settingsJson);
        if (!RuntimeAssetsActivity.isReady(this)) {
            android.content.Intent intent = new android.content.Intent(this, RuntimeAssetsActivity.class);
            intent.putExtra("auto_start", true);
            intent.putExtra("launch_game", true);
            intent.putExtra("exe_path", exePath);
            intent.putExtra("game_name", gameName);
            runOnUiThread(() -> startActivity(intent));
            return null;
        }
        return dev.gamehubx.session.WinlatorBridge.launch(this, exePath, gameName);
    }

    public boolean runtimeAssetsReady() {
        return RuntimeAssetsActivity.isReady(this);
    }

    public void downloadRuntimeAssets() {
        android.content.Intent intent = new android.content.Intent(this, RuntimeAssetsActivity.class);
        intent.putExtra("auto_start", true);
        runOnUiThread(() -> startActivity(intent));
    }

    @Override
    public boolean dispatchKeyEvent(android.view.KeyEvent event) {
        int source = event.getSource();
        int gamepadSources = android.view.InputDevice.SOURCE_GAMEPAD
                | android.view.InputDevice.SOURCE_JOYSTICK
                | android.view.InputDevice.SOURCE_DPAD;
        if ((source & gamepadSources) == 0) {
            return super.dispatchKeyEvent(event);
        }

        int action;
        switch (event.getKeyCode()) {
            case android.view.KeyEvent.KEYCODE_BUTTON_A:
                action = 5;
                break;
            case android.view.KeyEvent.KEYCODE_BUTTON_B:
                action = 6;
                break;
            case android.view.KeyEvent.KEYCODE_BUTTON_Y:
                action = 8;
                break;
            case android.view.KeyEvent.KEYCODE_BUTTON_START:
                action = 7;
                break;
            default:
                return super.dispatchKeyEvent(event);
        }
        if (event.getAction() == android.view.KeyEvent.ACTION_DOWN
                && event.getRepeatCount() == 0) {
            GAMEPAD_ACTIONS.offer(action);
            nativeWake();
        }
        return true;
    }

    public int pollGamepadAction() {
        Integer action = GAMEPAD_ACTIONS.poll();
        return action == null ? 0 : action;
    }

    public String systemLanguage() {
        return java.util.Locale.getDefault().getLanguage();
    }
}
