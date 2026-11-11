plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "com.swiftget.app"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.swiftget.app"
        minSdk = 24
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        vectorDrawables {
            useSupportLibrary = true
        }

        ndk {
            // Support both 32-bit and 64-bit ARM and x86 architectures
            abiFilters.addAll(listOf("armeabi-v7a", "arm64-v8a", "x86", "x86_64"))
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
        debug {
            applicationIdSuffix = ".debug"
            isDebuggable = true
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
        freeCompilerArgs += listOf(
            "-opt-in=androidx.compose.material3.ExperimentalMaterial3Api",
            "-opt-in=kotlinx.coroutines.ExperimentalCoroutinesApi"
        )
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
        jniLibs {
            useLegacyPackaging = false
        }
    }

    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.activity.compose)

    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.ui)
    implementation(libs.androidx.ui.graphics)
    implementation(libs.androidx.ui.tooling.preview)
    implementation(libs.androidx.material3)
    implementation(libs.androidx.material.icons.extended)
    implementation(libs.androidx.navigation.compose)

    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.kotlinx.serialization.json)
    implementation(libs.androidx.datastore.preferences)

    testImplementation(libs.junit)
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.espresso.core)
    androidTestImplementation(platform(libs.androidx.compose.bom))
    androidTestImplementation(libs.androidx.ui.test.junit4)
    debugImplementation(libs.androidx.ui.tooling)
    debugImplementation(libs.androidx.ui.test.manifest)
}

/**
 * Gradle task to build the swiftget-ffi Rust library using cargo-ndk
 * into app/src/main/jniLibs for armeabi-v7a, arm64-v8a, x86, and x86_64.
 */
tasks.register<Exec>("buildCargoNdk") {
    group = "rust"
    description = "Compiles crates/swiftget-ffi for Android ABIs using cargo-ndk into src/main/jniLibs"

    val ffiDir = file("${project.rootDir}/../crates/swiftget-ffi")
    val jniOutDir = file("${project.projectDir}/src/main/jniLibs")

    workingDir = ffiDir
    commandLine(
        "cargo", "ndk",
        "-t", "armeabi-v7a",
        "-t", "arm64-v8a",
        "-t", "x86",
        "-t", "x86_64",
        "-o", jniOutDir.absolutePath,
        "build", "--release"
    )

    doFirst {
        if (!jniOutDir.exists()) {
            jniOutDir.mkdirs()
        }
        println("Building Rust FFI cdylib with cargo-ndk into ${jniOutDir.absolutePath}...")
    }
}
