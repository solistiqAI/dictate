plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("io.github.takahirom.roborazzi")
}

// The phone's CPUs: arm64 for phones, x86_64 for the emulator. `-Pdictate.abis=arm64-v8a` builds
// only what a phone needs (faster).
val abis = (findProperty("dictate.abis") as String? ?: "arm64-v8a,x86_64").split(",").map { it.trim() }

android {
    namespace = "app.dictate"
    compileSdk = 35

    defaultConfig {
        applicationId = "app.dictate"
        // 29: Ogg/Opus voice messages decode in MediaCodec, switchToPreviousInputMethod exists.
        minSdk = 29
        targetSdk = 35
        versionCode = 1
        versionName = "0.2.0"
        ndk { abiFilters += abis }
    }

    // Two builds of the same app: "online" downloads the speech model itself; "offline" has no
    // internet permission at all (Android then guarantees it sends nothing) and imports the model
    // from a folder copied onto the phone.
    flavorDimensions += "network"
    productFlavors {
        create("online") {
            dimension = "network"
            isDefault = true
            buildConfigField("boolean", "OFFLINE", "false")
        }
        create("offline") {
            dimension = "network"
            versionNameSuffix = "-offline"
            buildConfigField("boolean", "OFFLINE", "true")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    // libdictate_android.so from `buildRust` below; libonnxruntime.so comes with the AAR.
    sourceSets["main"].jniLibs.srcDir(layout.buildDirectory.dir("rustJniLibs"))

    testOptions {
        unitTests.isIncludeAndroidResources = true
    }

    lint {
        abortOnError = true
        // Needs the network and is about upgrades, not about this code.
        disable += setOf("GradleDependency", "NewerVersionAvailable", "AndroidGradlePluginVersion")
        // About the Play Store's yearly target level (from the date and the SDKs installed); not
        // published there, and the level follows the pinned AGP.
        disable += setOf("ExpiredTargetSdkVersion", "OldTargetApi")
    }
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2024.12.01")
    implementation(composeBom)
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-core")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    debugImplementation("androidx.compose.ui:ui-tooling")
    debugImplementation("androidx.compose.ui:ui-test-manifest")

    // ONNX Runtime for the speech model: dictate-core loads its libonnxruntime.so at run time
    // (`ort` load-dynamic). The same release as the desktop's Linux build (core/src/onnxruntime.rs).
    implementation("com.microsoft.onnxruntime:onnxruntime-android:1.31.0")

    testImplementation("junit:junit:4.13.2")
    testImplementation(kotlin("test"))
    testImplementation("org.robolectric:robolectric:4.14.1")
    testImplementation(composeBom)
    testImplementation("androidx.compose.ui:ui-test-junit4")
    testImplementation("io.github.takahirom.roborazzi:roborazzi:1.38.0")
    testImplementation("io.github.takahirom.roborazzi:roborazzi-compose:1.38.0")
}

// The JNI crate (`android/rust`, dictate-core inside) for each ABI, with cargo-ndk
// (docs/android.md: rustup targets, the NDK, `cargo install cargo-ndk`). Always optimised: a debug
// build of the speech model's code is far too slow on a phone.
val rustDir = rootProject.projectDir.resolve("rust")
val rustOut = layout.buildDirectory.dir("rustJniLibs")
val buildRust by tasks.registering(Exec::class) {
    group = "build"
    description = "Builds libdictate_android.so (dictate-core) with cargo-ndk."
    workingDir = rustDir
    inputs.dir(rustDir.resolve("src"))
    inputs.file(rustDir.resolve("Cargo.toml"))
    inputs.dir(rootProject.projectDir.resolve("../core/src"))
    inputs.dir(rootProject.projectDir.resolve("../vendor/parakeet-rs/src"))
    inputs.file(rootProject.projectDir.resolve("../Cargo.lock"))
    inputs.property("abis", abis)
    outputs.dir(rustOut)
    val targets = abis.flatMap { listOf("-t", it) }
    // cargo-ndk finds the NDK through ANDROID_NDK_HOME / ANDROID_HOME; Android Studio may set
    // neither, so hand it the one Gradle knows when it is installed.
    doFirst {
        if (System.getenv("ANDROID_NDK_HOME") == null) {
            runCatching { android.ndkDirectory }.getOrNull()?.takeIf { it.exists() }
                ?.let { environment("ANDROID_NDK_HOME", it.path) }
        }
    }
    commandLine(
        listOf("cargo", "ndk") + targets +
            listOf("--platform", "29", "-o", rustOut.get().asFile.path, "build", "--release", "-p", "dictate-android"),
    )
}

// Only packaging needs the native library: unit tests and lint run without Rust and the NDK.
tasks.matching { it.name.startsWith("merge") && it.name.endsWith("JniLibFolders") }.configureEach {
    dependsOn(buildRust)
}
