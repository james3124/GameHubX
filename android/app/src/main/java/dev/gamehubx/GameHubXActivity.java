package dev.gamehubx;

import android.app.NativeActivity;
import android.os.Bundle;
import android.util.Log;

public final class GameHubXActivity extends NativeActivity {
    private static final String TAG = "GameHubX";

    static {
        System.loadLibrary("gamehubx_app");
    }

    private static native void nativeSetFilesDir(String path);

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        nativeSetFilesDir(getFilesDir().getAbsolutePath());
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

    public boolean filesAccessGranted() {
        return FilesAccess.granted(this);
    }

    public void requestFilesAccess() {
        FilesAccess.request(this);
    }

    public void openWinlator() {
        WinlatorLauncher.openMain(this);
    }

    public String launchWinlator(String exePath, String gameName) {
        return dev.gamehubx.session.WinlatorBridge.launch(this, exePath, gameName);
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

        int mapped;
        switch (event.getKeyCode()) {
            case android.view.KeyEvent.KEYCODE_BUTTON_A:
                mapped = android.view.KeyEvent.KEYCODE_ENTER;
                break;
            case android.view.KeyEvent.KEYCODE_BUTTON_B:
                mapped = android.view.KeyEvent.KEYCODE_ESCAPE;
                break;
            case android.view.KeyEvent.KEYCODE_BUTTON_Y:
                mapped = android.view.KeyEvent.KEYCODE_Y;
                break;
            case android.view.KeyEvent.KEYCODE_BUTTON_START:
                mapped = android.view.KeyEvent.KEYCODE_M;
                break;
            default:
                return super.dispatchKeyEvent(event);
        }

        android.view.KeyEvent translated = new android.view.KeyEvent(
                event.getDownTime(),
                event.getEventTime(),
                event.getAction(),
                mapped,
                event.getRepeatCount());
        translated.setSource(source);
        return super.dispatchKeyEvent(translated);
    }

    public String systemLanguage() {
        return java.util.Locale.getDefault().getLanguage();
    }
}
