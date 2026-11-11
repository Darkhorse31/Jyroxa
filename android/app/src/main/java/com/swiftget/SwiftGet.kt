package com.swiftget

import android.util.Log

/**
 * Direct JNI bindings matching the exact symbol exports in `crates/swiftget-ffi`:
 * - Java_com_swiftget_SwiftGet_nativeStart
 * - Java_com_swiftget_SwiftGet_nativePause
 * - Java_com_swiftget_SwiftGet_nativeResume
 * - Java_com_swiftget_SwiftGet_nativeCancel
 * - Java_com_swiftget_SwiftGet_nativeStatus
 */
object SwiftGet {
    private const val TAG = "SwiftGetJNI"
    private var isLoaded = false

    init {
        try {
            System.loadLibrary("swiftget_ffi")
            isLoaded = true
            Log.i(TAG, "libswiftget_ffi.so loaded successfully")
        } catch (e: UnsatisfiedLinkError) {
            Log.w(TAG, "libswiftget_ffi.so not present in jniLibs (build using ./gradlew buildCargoNdk)", e)
            isLoaded = false
        } catch (e: Throwable) {
            Log.e(TAG, "Error loading libswiftget_ffi.so", e)
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
