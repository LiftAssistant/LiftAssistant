pluginManagement {
    val flutterSdkPath =
        run {
            val localPropertiesFile = file("local.properties")
            require(localPropertiesFile.exists()) {
                "local.properties not found in client/android; run 'flutter pub get' in client/ first."
            }
            val rawText = localPropertiesFile.readText(Charsets.UTF_8)
            val properties = java.util.Properties()
            properties.load(rawText.reader())
            if (rawText.any { it.code > 127 }) {
                localPropertiesFile.outputStream().use { properties.store(it, null) }
            }
            val flutterSdkPath = properties.getProperty("flutter.sdk")
            require(flutterSdkPath != null) { "flutter.sdk not set in local.properties" }
            flutterSdkPath
        }

    includeBuild("$flutterSdkPath/packages/flutter_tools/gradle")

    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

plugins {
    id("dev.flutter.flutter-plugin-loader") version "1.0.0"
    id("com.android.application") version "9.1.0" apply false
    id("org.jetbrains.kotlin.android") version "2.4.0" apply false
}

include(":app")
