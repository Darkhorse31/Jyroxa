package com.swiftget.app.native

import android.util.Log
import com.swiftget.SwiftGet
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong

/**
 * Native bridge for the SwiftGet Rust core download engine.
 *
 * This class provides:
 * 1. External JNI declarations matching `Java_com_swiftget_app_SwiftGetNative_*`.
 * 2. Delegation to `com.swiftget.SwiftGet` (which matches `crates/swiftget-ffi`'s `Java_com_swiftget_SwiftGet_*`).
 * 3. Typed parsing of the engine's JSON status payloads.
 * 4. A graceful fallback simulation engine for development and UI previewing before the Rust cdylib is compiled.
 */
object SwiftGetNative {
    private const val TAG = "SwiftGetNative"

    private val jsonParser = Json {
        ignoreUnknownKeys = true
        isLenient = true
    }

    private var useAppJni = false

    init {
        try {
            System.loadLibrary("swiftget_ffi")
            Log.i(TAG, "libswiftget_ffi loaded for SwiftGetNative")
        } catch (e: UnsatisfiedLinkError) {
            Log.w(TAG, "libswiftget_ffi not yet loaded directly in SwiftGetNative: ${e.message}")
        } catch (e: Throwable) {
            Log.e(TAG, "Failed loading libswiftget_ffi in SwiftGetNative", e)
        }
    }

    // =========================================================================
    // External JNI Functions matching Java_com_swiftget_app_SwiftGetNative_*
    // =========================================================================

    /**
     * JNI export: Java_com_swiftget_app_SwiftGetNative_nativeStart
     */
    @JvmStatic
    external fun nativeStart(url: String, output: String, connections: Int): Long

    /**
     * JNI export: Java_com_swiftget_app_SwiftGetNative_nativePause
     */
    @JvmStatic
    external fun nativePause(id: Long): Boolean

    /**
     * JNI export: Java_com_swiftget_app_SwiftGetNative_nativeResume
     */
    @JvmStatic
    external fun nativeResume(id: Long): Boolean

    /**
     * JNI export: Java_com_swiftget_app_SwiftGetNative_nativeCancel
     */
    @JvmStatic
    external fun nativeCancel(id: Long): Boolean

    /**
     * JNI export: Java_com_swiftget_app_SwiftGetNative_nativeStatus
     */
    @JvmStatic
    external fun nativeStatus(id: Long): String

    // =========================================================================
    // Status Data Class
    // =========================================================================

    @Serializable
    data class StatusPayload(
        val id: Long = 0,
        val state: String = "unknown",
        val downloaded: Long = 0L,
        val total: Long = 0L,
        val speed_bps: Double = 0.0,
        val progress_pct: Double = 0.0
    )

    // =========================================================================
    // Fallback Simulation State (used only when native library is absent)
    // =========================================================================

    private val fallbackIdCounter = AtomicLong(1000)
    private val fallbackTasks = ConcurrentHashMap<Long, FallbackState>()

    private class FallbackState(
        val id: Long,
        val url: String,
        val output: String,
        val connections: Int,
        var state: String = "running",
        var downloaded: Long = 0L,
        var total: Long = 250L * 1024L * 1024L, // 250 MB mock
        var speed: Double = 5.5 * 1024.0 * 1024.0 // 5.5 MB/s mock
    )

    // =========================================================================
    // Public High-Level Bridge APIs
    // =========================================================================

    /**
     * Start a download task.
     * @return Positive task handle (>0) on success, or 0 on error.
     */
    fun start(url: String, output: String, connections: Int): Long {
        // 1. Try crates/swiftget-ffi export (com.swiftget.SwiftGet)
        if (SwiftGet.isLoaded()) {
            return try {
                SwiftGet.nativeStart(url, output, connections)
            } catch (e: UnsatisfiedLinkError) {
                tryAppJniStart(url, output, connections)
            }
        }

        // 2. Try com.swiftget.app.native.SwiftGetNative JNI export
        return tryAppJniStart(url, output, connections)
    }

    private fun tryAppJniStart(url: String, output: String, connections: Int): Long {
        return try {
            nativeStart(url, output, connections)
        } catch (e: UnsatisfiedLinkError) {
            // 3. Fallback mock simulation for development preview
            Log.w(TAG, "Native library not available, starting simulated download")
            val id = fallbackIdCounter.incrementAndGet()
            fallbackTasks[id] = FallbackState(id, url, output, connections)
            id
        }
    }

    fun pause(id: Long): Boolean {
        if (SwiftGet.isLoaded()) {
            return try {
                SwiftGet.nativePause(id)
            } catch (e: UnsatisfiedLinkError) {
                tryAppJniPause(id)
            }
        }
        return tryAppJniPause(id)
    }

    private fun tryAppJniPause(id: Long): Boolean {
        return try {
            nativePause(id)
        } catch (e: UnsatisfiedLinkError) {
            fallbackTasks[id]?.let {
                it.state = "paused"
                it.speed = 0.0
                true
            } ?: false
        }
    }

    fun resume(id: Long): Boolean {
        if (SwiftGet.isLoaded()) {
            return try {
                SwiftGet.nativeResume(id)
            } catch (e: UnsatisfiedLinkError) {
                tryAppJniResume(id)
            }
        }
        return tryAppJniResume(id)
    }

    private fun tryAppJniResume(id: Long): Boolean {
        return try {
            nativeResume(id)
        } catch (e: UnsatisfiedLinkError) {
            fallbackTasks[id]?.let {
                it.state = "running"
                it.speed = 4.8 * 1024.0 * 1024.0
                true
            } ?: false
        }
    }

    fun cancel(id: Long): Boolean {
        if (SwiftGet.isLoaded()) {
            return try {
                SwiftGet.nativeCancel(id)
            } catch (e: UnsatisfiedLinkError) {
                tryAppJniCancel(id)
            }
        }
        return tryAppJniCancel(id)
    }

    private fun tryAppJniCancel(id: Long): Boolean {
        return try {
            nativeCancel(id)
        } catch (e: UnsatisfiedLinkError) {
            fallbackTasks.remove(id) != null
        }
    }

    /**
     * Poll download status and return parsed status payload.
     */
    fun status(id: Long): StatusPayload {
        val rawJson = if (SwiftGet.isLoaded()) {
            try {
                SwiftGet.nativeStatus(id)
            } catch (e: UnsatisfiedLinkError) {
                tryAppJniStatus(id)
            }
        } else {
            tryAppJniStatus(id)
        }

        return try {
            jsonParser.decodeFromString<StatusPayload>(rawJson)
        } catch (e: Exception) {
            StatusPayload(id = id, state = "error")
        }
    }

    private fun tryAppJniStatus(id: Long): String {
        return try {
            nativeStatus(id)
        } catch (e: UnsatisfiedLinkError) {
            val task = fallbackTasks[id]
            if (task == null) {
                return """{"id":$id,"state":"unknown"}"""
            }

            if (task.state == "running") {
                val increment = (task.speed * 0.5).toLong()
                task.downloaded = (task.downloaded + increment).coerceAtMost(task.total)
                if (task.downloaded >= task.total) {
                    task.state = "completed"
                    task.speed = 0.0
                }
            }

            val progressPct = if (task.total > 0) {
                (task.downloaded.toDouble() / task.total.toDouble() * 100.0).coerceIn(0.0, 100.0)
            } else 0.0

            """{"id":$id,"state":"${task.state}","downloaded":${task.downloaded},"total":${task.total},"speed_bps":${task.speed},"progress_pct":$progressPct}"""
        }
    }
}
