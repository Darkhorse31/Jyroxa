package com.jyroxa.app

import android.util.Log

/**
 * Direct JNI bindings matching the exact symbol exports in `crates/jyroxa-ffi`:
 * - Java_com_jyroxa_app_JyroxaFfi_nativeStart
 * - Java_com_jyroxa_app_JyroxaFfi_nativePause
 * - Java_com_jyroxa_app_JyroxaFfi_nativeResume
 * - Java_com_jyroxa_app_JyroxaFfi_nativeCancel
 * - Java_com_jyroxa_app_JyroxaFfi_nativeStatus
 */
object JyroxaFfi {
    private const val TAG = "JyroxaFfiJNI"
    private var isLoaded = false

    init {
        try {
            System.loadLibrary("jyroxa_ffi")
            isLoaded = true
            Log.i(TAG, "libjyroxa_ffi.so loaded successfully")
        } catch (e: UnsatisfiedLinkError) {
            Log.w(TAG, "libjyroxa_ffi.so not present in jniLibs (build using ./gradlew buildCargoNdk)", e)
            isLoaded = false
        } catch (e: Throwable) {
            Log.e(TAG, "Error loading libjyroxa_ffi.so", e)
            isLoaded = false
        }
    }

    /**
     * Indicates whether the compiled Rust cdylib was successfully loaded into memory.
     */
    fun isLoaded(): Boolean = isLoaded

    @JvmStatic
    external fun nativeStart(url: String, output: String, connections: Int): Long

    @JvmStatic
    external fun nativePause(id: Long): Boolean

    @JvmStatic
    external fun nativeResume(id: Long): Boolean

    @JvmStatic
    external fun nativeCancel(id: Long): Boolean

    @JvmStatic
    external fun nativeStatus(id: Long): String
}
