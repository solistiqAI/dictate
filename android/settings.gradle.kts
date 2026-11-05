// Dictate for Android (docs/android.md): the app in `app/`, the JNI crate around dictate-core in
// `rust/` (built by `app`'s `buildRust` task through cargo-ndk).
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "dictate"
include(":app")
