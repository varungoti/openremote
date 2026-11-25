import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import org.gradle.api.artifacts.dsl.LockMode

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

// Both native clients consume this same public profile. Never embed enrollment or session secrets.
val sharedServiceProfile = rootProject.file("../config/service-profile.json").readText()
val parsedServiceProfile = groovy.json.JsonSlurper().parseText(sharedServiceProfile) as? Map<*, *>
require(parsedServiceProfile?.keys == setOf("schemaVersion", "serviceName", "apiOrigin") &&
    parsedServiceProfile["schemaVersion"] == 1 && parsedServiceProfile["serviceName"] == "OpenRemote" &&
    parsedServiceProfile["apiOrigin"] is String) { "Invalid shared OpenRemote service profile" }

android {
    namespace = "com.openremote.mobile"
    compileSdk = 36
    defaultConfig {
        applicationId = "com.openremote.mobile"
        minSdk = 26
        targetSdk = 36
        versionCode = 8
        versionName = "0.8.0"
        resValue("string", "app_name", "OpenRemote")
        buildConfigField("String", "SERVICE_PROFILE_JSON", groovy.json.JsonOutput.toJson(sharedServiceProfile))
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }
    buildFeatures { compose = true; buildConfig = true }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    packaging { jniLibs.useLegacyPackaging = false }
    splits {
        abi {
            isEnable = true
            reset()
            include("arm64-v8a", "armeabi-v7a", "x86_64", "x86")
            isUniversalApk = false
        }
    }
    buildTypes {
        debug {
            // A fresh development certificate must not require uninstalling the previous app/identity.
            applicationIdSuffix = ".dev"
            resValue("string", "app_name", "OpenRemote Preview")
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }
    testOptions { unitTests.isReturnDefaultValues = true }
}
kotlin { compilerOptions { jvmTarget.set(JvmTarget.JVM_17) } }

// All resolved app/test/tooling configurations must have reviewed, checked-in lock state.
dependencyLocking {
    lockAllConfigurations()
    lockMode.set(LockMode.STRICT)
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2025.10.01"))
    implementation("androidx.activity:activity-compose:1.11.0")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.9.4")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.9.4")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.9.4")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    implementation("io.github.webrtc-sdk:android:150.7871.04")
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.json:json:20240303")
    testImplementation("com.squareup.okhttp3:mockwebserver:4.12.0")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.10.2")
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    androidTestImplementation("androidx.test:runner:1.7.0")
    debugImplementation("androidx.compose.ui:ui-tooling")
}

// A provider must ship a configured service. The exceptional blank-profile output
// is permitted only for unsigned development inspection, never a signed release.
val verifyReleaseServiceProfile = tasks.register("verifyReleaseServiceProfile") {
    inputs.file(rootProject.file("../config/service-profile.json"))
    val preview = providers.gradleProperty("openremoteUnconfiguredPreview").map { it == "true" }.orElse(false)
    inputs.property("openremoteUnconfiguredPreview", preview)
    doLast {
        val origin = requireNotNull(parsedServiceProfile)["apiOrigin"] as String
        if (origin.isEmpty()) {
            if (!preview.get()) throw GradleException(
                "OpenRemote release packaging is blocked: the provider service profile is unconfigured. " +
                "Configure config/service-profile.json before distributing the app. " +
                "Unsigned development inspection alone may use -PopenremoteUnconfiguredPreview=true."
            )
            val release = project.extensions.getByType<com.android.build.api.dsl.ApplicationExtension>().buildTypes.getByName("release")
            val injectedSigning = project.properties.keys.any { it.startsWith("android.injected.signing.") }
            if (release.signingConfig != null || injectedSigning) throw GradleException(
                "An unconfigured OpenRemote preview must remain unsigned; release signing is not permitted with this exception."
            )
            logger.lifecycle("OpenRemote: explicitly building an UNCONFIGURED UNSIGNED DEVELOPMENT PREVIEW; not ready for customers.")
        }
    }
}
tasks.matching { it.name == "preReleaseBuild" }.configureEach { dependsOn(verifyReleaseServiceProfile) }
