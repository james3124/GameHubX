package dev.gamehubx;

import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.graphics.Color;
import android.os.Bundle;
import android.os.StatFs;
import android.view.View;
import android.widget.Button;
import android.widget.ProgressBar;
import android.widget.TextView;

import java.io.BufferedInputStream;
import java.io.BufferedOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.Locale;
import java.util.zip.ZipEntry;
import java.util.zip.ZipInputStream;

/**
 * First-run bootstrap for Winlator runtime data.
 *
 * The APK carries only the small launcher assets. imagefs, Proton and the
 * remaining session assets are downloaded into files/runtime-assets and the
 * Winlator asset helpers fall back to that directory.
 */
public final class RuntimeAssetsActivity extends Activity {
    private static final String COMMIT = "527848f555c4349b26578e714acd4654b6dbd956";
    private static final String ASSETS_ZIP =
            "https://codeload.github.com/StevenMXZ/Winlator-Ludashi/zip/" + COMMIT;
    private static final String IMAGE_00 =
            "https://raw.githubusercontent.com/Other-backup/winlator-imagefs-v2/main/imagefs.tar.zst.00";
    private static final String IMAGE_01 =
            "https://raw.githubusercontent.com/Other-backup/winlator-imagefs-v2/main/imagefs.tar.zst.01";
    private static final String PROTON =
            "https://github.com/Other-backup/winlator-imagefs-v2/releases/download/pnew2/proton-wine-11.0-7-arm64ec.wcp";
    private static final String PROTON_SHA256 =
            "1ef0bb9efceb2278933b3fa2bfd374999b0d9a7c74045b67b9af8a9b5260f734";
    private static final String READY_MARKER = ".ready";
    private static final String RUNTIME_DIR = "runtime-assets";

    private ProgressBar progressBar;
    private TextView status;
    private Button retry;
    private volatile boolean cancelled;
    private volatile boolean running;
    private boolean launchGame;
    private String exePath = "";
    private String gameName = "";

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        launchGame = getIntent().getBooleanExtra("launch_game", false);
        exePath = getIntent().getStringExtra("exe_path");
        gameName = getIntent().getStringExtra("game_name");
        if (exePath == null) exePath = "";
        if (gameName == null) gameName = "";
        if (isReady(this)) {
            finishReady();
            return;
        }
        buildUi();
        if (getIntent().getBooleanExtra("auto_start", false)) {
            retry.setVisibility(View.GONE);
            startBootstrap();
        } else {
            status.setText("About 500 MB. Download once, then PC games can start offline.");
            retry.setText("Download runtime");
            retry.setVisibility(View.VISIBLE);
        }
    }

    @Override
    protected void onDestroy() {
        cancelled = true;
        super.onDestroy();
    }

    private void buildUi() {
        getWindow().setStatusBarColor(Color.TRANSPARENT);
        getWindow().setNavigationBarColor(Color.rgb(32, 36, 47));
        setContentView(R.layout.activity_runtime_assets);
        progressBar = findViewById(R.id.runtime_progress);
        status = findViewById(R.id.runtime_status);
        retry = findViewById(R.id.runtime_retry);
        retry.setOnClickListener(view -> {
            retry.setVisibility(View.GONE);
            if (isReady(this)) {
                finishReady();
            } else {
                startBootstrap();
            }
        });
    }

    private void startBootstrap() {
        if (running) return;
        running = true;
        cancelled = false;
        new Thread(this::bootstrap, "GameHubX-RuntimeAssets").start();
    }

    private void bootstrap() {
        try {
            File runtime = runtimeDir();
            if (!runtime.isDirectory() && !runtime.mkdirs()) {
                throw new IOException("Cannot create " + runtime);
            }
            StatFs stat = new StatFs(runtime.getAbsolutePath());
            long available = stat.getAvailableBytes();
            long required = 900L * 1024L * 1024L;
            if (available < required) {
                throw new IOException("Need at least 900 MB free; available " + formatBytes(available));
            }

            File zip = new File(getCacheDir(), "winlator-assets.zip");
            if (!zip.isFile()) {
                download(ASSETS_ZIP, zip, 0, 45, "Downloading session assets");
            }
            publish("Extracting session assets", 45);
            try {
                extractAssets(zip, runtime);
            } catch (IOException error) {
                zip.delete();
                throw error;
            }
            if (zip.isFile() && !zip.delete()) {
                // Not fatal: the extracted runtime is complete.
            }

            File imagefs = new File(runtime, "imagefs.tar.zst");
            if (!imagefs.isFile() || imagefs.length() < 100L * 1024L * 1024L) {
                File part0 = new File(runtime, "imagefs.tar.zst.00");
                File part1 = new File(runtime, "imagefs.tar.zst.01");
                if (!part0.isFile()) download(IMAGE_00, part0, 52, 14, "Downloading imagefs (1/2)");
                if (!part1.isFile()) download(IMAGE_01, part1, 66, 14, "Downloading imagefs (2/2)");
                publish("Joining imagefs", 80);
                join(part0, part1, imagefs);
                if (!part0.delete() || !part1.delete()) {
                    throw new IOException("Cannot remove imagefs parts");
                }
            }

            File proton = new File(runtime, "proton-11.0-7-arm64ec.tar.zst");
            if (!proton.isFile() || !PROTON_SHA256.equalsIgnoreCase(sha256(proton))) {
                download(PROTON, proton, 81, 18, "Downloading Proton 11");
                if (!PROTON_SHA256.equalsIgnoreCase(sha256(proton))) {
                    throw new IOException("Proton archive checksum mismatch");
                }
            }

            File marker = new File(runtime, READY_MARKER);
            FileOutputStream markerOut = new FileOutputStream(marker);
            markerOut.write((COMMIT + "\n").getBytes(StandardCharsets.UTF_8));
            markerOut.close();

            publish("Runtime ready", 100);
            runOnUiThread(() -> {
                running = false;
                finishReady();
            });
        } catch (Exception error) {
            cancelled = false;
            running = false;
            runOnUiThread(() -> {
                status.setText("Setup failed: " + error.getMessage());
                retry.setText("Retry");
                retry.setVisibility(View.VISIBLE);
            });
        }
    }

    private void download(String url, File destination, int base, int span, String label) throws IOException {
        if (cancelled) throw new IOException("Cancelled");
        File part = new File(destination.getAbsolutePath() + ".part");
        long offset = part.isFile() ? part.length() : 0;
        HttpURLConnection connection = (HttpURLConnection) new URL(url).openConnection();
        connection.setConnectTimeout(30000);
        connection.setReadTimeout(60000);
        connection.setInstanceFollowRedirects(true);
        connection.setRequestProperty("User-Agent", "GameHubX");
        connection.setRequestProperty("Accept-Encoding", "identity");
        if (offset > 0) connection.setRequestProperty("Range", "bytes=" + offset + "-");

        int code = connection.getResponseCode();
        boolean append;
        if (code == 206) {
            append = true;
        } else if (code == 200) {
            append = false;
            offset = 0;
        } else {
            connection.disconnect();
            throw new IOException("HTTP " + code + " for " + url);
        }

        long length = connection.getContentLengthLong();
        long total = length > 0 ? offset + length : 0;
        long done = offset;
        try (InputStream input = new BufferedInputStream(connection.getInputStream());
             OutputStream output = new BufferedOutputStream(new FileOutputStream(part, append))) {
            byte[] buffer = new byte[64 * 1024];
            int read;
            long lastUpdate = 0;
            while ((read = input.read(buffer)) != -1) {
                if (cancelled) throw new IOException("Cancelled");
                output.write(buffer, 0, read);
                done += read;
                long now = System.currentTimeMillis();
                if (now - lastUpdate >= 250) {
                    lastUpdate = now;
                    int progress = total > 0
                            ? base + (int) Math.min(span, (done * span) / total)
                            : base;
                    publish(label + "  " + formatBytes(done), progress);
                }
            }
        } finally {
            connection.disconnect();
        }
        if (destination.isFile() && !destination.delete()) {
            throw new IOException("Cannot replace " + destination);
        }
        if (!part.renameTo(destination)) {
            throw new IOException("Cannot finalize " + destination.getName());
        }
        publish(label + " complete", base + span);
    }

    private void extractAssets(File zip, File runtime) throws IOException {
        File canonicalRoot = runtime.getCanonicalFile();
        String prefix = "app/src/main/assets/";
        try (ZipInputStream input = new ZipInputStream(
                new BufferedInputStream(new FileInputStream(zip)))) {
            ZipEntry entry;
            byte[] buffer = new byte[64 * 1024];
            while ((entry = input.getNextEntry()) != null) {
                if (cancelled) throw new IOException("Cancelled");
                String name = entry.getName().replace('\\', '/');
                int index = name.indexOf(prefix);
                if (index < 0) continue;
                String relative = name.substring(index + prefix.length());
                if (relative.isEmpty()) continue;
                if (relative.contains("..")) throw new IOException("Unsafe asset path");
                File output = new File(runtime, relative).getCanonicalFile();
                if (!output.toPath().startsWith(canonicalRoot.toPath())) {
                    throw new IOException("Unsafe asset path");
                }
                if (entry.isDirectory()) {
                    if (!output.isDirectory() && !output.mkdirs()) {
                        throw new IOException("Cannot create " + output);
                    }
                    continue;
                }
                File parent = output.getParentFile();
                if (parent != null && !parent.isDirectory() && !parent.mkdirs()) {
                    throw new IOException("Cannot create " + parent);
                }
                try (OutputStream out = new BufferedOutputStream(new FileOutputStream(output))) {
                    int read;
                    while ((read = input.read(buffer)) != -1) out.write(buffer, 0, read);
                }
            }
        }
    }

    private void join(File first, File second, File output) throws IOException {
        try (InputStream in0 = new BufferedInputStream(new FileInputStream(first));
             InputStream in1 = new BufferedInputStream(new FileInputStream(second));
             OutputStream out = new BufferedOutputStream(new FileOutputStream(output))) {
            copy(in0, out);
            copy(in1, out);
        }
    }

    private static void copy(InputStream input, OutputStream output) throws IOException {
        byte[] buffer = new byte[64 * 1024];
        int read;
        while ((read = input.read(buffer)) != -1) output.write(buffer, 0, read);
    }

    private static String sha256(File file) throws IOException {
        try (InputStream input = new BufferedInputStream(new FileInputStream(file))) {
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            byte[] buffer = new byte[64 * 1024];
            int read;
            while ((read = input.read(buffer)) != -1) digest.update(buffer, 0, read);
            StringBuilder hex = new StringBuilder();
            for (byte value : digest.digest()) hex.append(String.format(Locale.US, "%02x", value & 0xff));
            return hex.toString();
        } catch (Exception error) {
            throw new IOException("Cannot hash " + file.getName(), error);
        }
    }

    private static String formatBytes(long bytes) {
        if (bytes >= 1024L * 1024L * 1024L) {
            return String.format(Locale.US, "%.2f GB", bytes / (1024.0 * 1024.0 * 1024.0));
        }
        return String.format(Locale.US, "%.1f MB", bytes / (1024.0 * 1024.0));
    }

    private File runtimeDir() {
        return new File(getFilesDir(), RUNTIME_DIR);
    }

    public static boolean isReady(Context context) {
        return new File(context.getFilesDir(), RUNTIME_DIR + "/" + READY_MARKER).isFile();
    }

    private void finishReady() {
        if (launchGame) {
            String error = dev.gamehubx.session.WinlatorBridge.launch(this, exePath, gameName);
            if (error == null) {
                finish();
                return;
            }
            if (status == null) buildUi();
            status.setText(error);
            retry.setText("Retry launch");
            retry.setVisibility(View.VISIBLE);
            running = false;
            return;
        }
        openMain();
    }

    private void publish(String message, int progress) {
        runOnUiThread(() -> {
            status.setText(message);
            progressBar.setProgress(Math.max(0, Math.min(100, progress)));
        });
    }

    private void openMain() {
        Intent intent = new Intent();
        intent.setClassName(this, "com.winlator.cmod.MainActivity");
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TOP);
        startActivity(intent);
        finish();
    }
}
