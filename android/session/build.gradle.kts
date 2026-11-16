plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "com.winlator.cmod"
    compileSdk = 35
    ndkVersion = "29.0.14206865"

    defaultConfig {
        minSdk = 28
        buildConfigField("String", "APPLICATION_ID", "\"com.winlator.cmod\"")
        buildConfigField("String", "VERSION_NAME", "\"5.0\"")
        buildConfigField("int", "VERSION_CODE", "66")
        ndk {
            abiFilters += listOf("arm64-v8a")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
        freeCompilerArgs += listOf("-opt-in=androidx.compose.material3.ExperimentalMaterial3Api")
    }

    buildFeatures {
        compose = true
        buildConfig = true
        prefab = true
    }

    externalNativeBuild {
        cmake {
            path = file("../../thirdparty/winlator/app/src/main/cpp/CMakeLists.txt")
        }
    }

    sourceSets {
        getByName("main") {
            manifest.srcFile("../../thirdparty/winlator/app/src/main/AndroidManifest.xml")
            java.srcDirs("src/main/java", "../../thirdparty/winlator/app/src/main/java")
            kotlin.srcDirs("src/main/java", "../../thirdparty/winlator/app/src/main/java")
            res.srcDirs("../../thirdparty/winlator/app/src/main/res")
            jniLibs.srcDirs("../../thirdparty/winlator/app/src/main/jniLibs")
            assets.srcDirs("../../thirdparty/winlator/app/src/main/assets")
        }
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

    lint {
        disable += "ExpiredTargetSdkVersion"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }
}

dependencies {
    implementation("com.jakewharton.timber:timber:5.0.1")

    implementation(platform("androidx.compose:compose-bom:2025.08.00"))
    implementation("androidx.activity:activity-compose:1.10.1")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    debugImplementation("androidx.compose.ui:ui-tooling")

    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("androidx.preference:preference:1.2.1")
    implementation("com.google.android.material:material:1.4.0")
    implementation("com.github.luben:zstd-jni:1.5.2-3@aar")
    implementation("org.tukaani:xz:1.7")
    implementation("org.jsoup:jsoup:1.18.3")
    implementation("org.apache.commons:commons-compress:1.20")
    implementation("androidx.activity:activity-ktx:1.4.0")
    implementation("androidx.fragment:fragment-ktx:1.4.0")
    implementation(files("../../thirdparty/winlator/app/libs/MidiSynth/MidiSynth.jar"))
    implementation("androidx.recyclerview:recyclerview:1.2.0")
    implementation("com.squareup.retrofit2:retrofit:2.9.0")
    implementation("com.squareup.retrofit2:converter-gson:2.9.0")
    implementation("com.squareup.okhttp3:okhttp:4.9.3")
    implementation("com.google.code.gson:gson:2.8.8")
    implementation("com.google.zxing:core:3.5.3")
    implementation("org.conscrypt:conscrypt-android:2.5.2")
    implementation("org.bouncycastle:bcprov-jdk15on:1.68")
    implementation("org.openjsse:openjsse:1.1.7")
    implementation("androidx.core:core:1.0.1")
    implementation("com.github.bumptech.glide:glide:4.11.0")
    annotationProcessor("com.github.bumptech.glide:compiler:4.11.0")
    implementation("com.kohlschutter.junixsocket:junixsocket-common:2.6.0")
    implementation("com.kohlschutter.junixsocket:junixsocket-native-common:2.6.0")
    implementation("androidx.documentfile:documentfile:1.0.1")
    implementation("androidx.drawerlayout:drawerlayout:1.1.1")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.7")
    implementation("androidx.savedstate:savedstate-ktx:1.2.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.1")
}
