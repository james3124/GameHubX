package dev.gamehubx;

import android.app.NativeActivity;
import android.os.Bundle;
import android.util.Log;

public final class GameHubXActivity extends NativeActivity {
    private static final String TAG = "GameHubX";
    private static final java.util.concurrent.ConcurrentLinkedQueue<Integer> GAMEPAD_ACTIONS =
            new java.util.concurrent.ConcurrentLinkedQueue<>();

    static {
        System.loadLibrary("gamehubx_app");
    }

    private static native void nativeSetFilesDir(String path);
    private static native void nativeWake();

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
