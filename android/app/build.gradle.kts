plugins {
    id("com.android.application")
}

val releaseKeystore = System.getenv("GAMEHUBX_KEYSTORE_PATH")
val releaseStorePassword =
    System.getenv("GAMEHUBX_KEYSTORE_PASSWORD")?.takeIf { it.isNotBlank() } ?: ""
val releaseKeyAlias =
    System.getenv("GAMEHUBX_KEY_ALIAS")?.takeIf { it.isNotBlank() } ?: "gamehubx"
val releaseKeyPassword =
    System.getenv("GAMEHUBX_KEY_PASSWORD")?.takeIf { it.isNotBlank() } ?: releaseStorePassword
val releaseVersionName =
    System.getenv("GAMEHUBX_VERSION_NAME")?.removePrefix("v")?.takeIf { it.isNotBlank() }

android {
    namespace = "dev.gamehubx"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.gamehubx"
        minSdk = 28
        targetSdk = 35
        versionCode = System.getenv("GAMEHUBX_VERSION_CODE")?.toIntOrNull() ?: 1
        versionName = releaseVersionName ?: "0.1.0"
        ndk {
            abiFilters += listOf("arm64-v8a")
        }
    }

    signingConfigs {
        if (!releaseKeystore.isNullOrBlank()) {
            create("release") {
                storeFile = file(releaseKeystore!!)
                storePassword = releaseStorePassword
                keyAlias = releaseKeyAlias
                keyPassword = releaseKeyPassword
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            if (!releaseKeystore.isNullOrBlank()) {
                signingConfig = signingConfigs.getByName("release")
            }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    packaging {
        jniLibs {
            useLegacyPackaging = true
            pickFirsts += "**/libc++_shared.so"
        }
        resources {
            pickFirsts += listOf(
                "assets/dexopt/baseline.prof",
                "assets/dexopt/baseline.profm",
            )
        }
    }
}

dependencies {
    implementation(project(":session"))
    // HomeShortcut uses androidx.core ShortcutInfoCompat/ShortcutManagerCompat.
    implementation("androidx.core:core:1.13.1")
}
