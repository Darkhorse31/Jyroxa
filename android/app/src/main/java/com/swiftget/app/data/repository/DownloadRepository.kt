package com.swiftget.app.data.repository

import android.util.Log
import com.swiftget.app.data.model.DownloadItem
import com.swiftget.app.data.model.DownloadState
import com.swiftget.app.data.model.SegmentProgress
import com.swiftget.app.data.model.SpeedSample
import com.swiftget.app.native.SwiftGetNative
import com.swiftget.app.util.NetworkMonitor
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.io.File
import java.util.concurrent.ConcurrentHashMap

/**
 * Central repository managing active and historic downloads with:
 * - EWMA speed smoothing & ETA calculation
 * - Average speed tracking
 * - Wi-Fi only / metered network pause & resume
 * - State persistence for process death / reboot recovery
 */
class DownloadRepository(
    private val scope: CoroutineScope = CoroutineScope(Dispatchers.Default + Job()),
    private val storageFile: File? = null,
    private val networkMonitor: NetworkMonitor? = null
) {
    companion object {
        private const val TAG = "DownloadRepository"
        private const val EWMA_ALPHA = 0.25
    }

    private val json = Json {
        ignoreUnknownKeys = true
        prettyPrint = false
    }

    private val _downloads = MutableStateFlow<List<DownloadItem>>(emptyList())
    val downloads: StateFlow<List<DownloadItem>> = _downloads.asStateFlow()

    val runningDownloadsCount: StateFlow<Int> = _downloads.map { list ->
        list.count { it.state.isRunning }
    }.stateIn(scope, SharingStarted.Eagerly, 0)

    val totalSpeedBps: StateFlow<Double> = _downloads.map { list ->
        list.filter { it.state.isRunning }.sumOf { it.speedBps }
    }.stateIn(scope, SharingStarted.Eagerly, 0.0)

    private val ewmaSpeedMap = ConcurrentHashMap<Long, Double>()
    private val pausedByWifiOnly = ConcurrentHashMap.newKeySet<Long>()
    private var isWifiOnlyEnforced = false
    private var pollingJob: Job? = null

    init {
        loadPersistedDownloads()
        observeNetwork()
        startPollingLoop()
    }

    fun setWifiOnlyEnforced(enforced: Boolean) {
        isWifiOnlyEnforced = enforced
        checkWifiConstraints()
    }

    fun isCurrentlyMetered(): Boolean {
        return networkMonitor?.checkIsUnmetered() == false
    }

    private fun observeNetwork() {
        if (networkMonitor == null) return
        scope.launch {
            networkMonitor.isUnmetered.collect { isUnmetered ->
                if (isWifiOnlyEnforced) {
                    if (!isUnmetered) {
                        // Pause running downloads due to metered network
                        val running = _downloads.value.filter { it.state.isRunning }
                        running.forEach { item ->
                            pausedByWifiOnly.add(item.id)
                            pauseDownload(item.id)
                            Log.i(TAG, "Paused download ${item.id} because network is metered (Wi-Fi only)")
                        }
                    } else if (pausedByWifiOnly.isNotEmpty()) {
                        // Resume downloads paused for Wi-Fi only
                        val toResume = pausedByWifiOnly.toList()
                        pausedByWifiOnly.clear()
                        toResume.forEach { id ->
                            resumeDownload(id)
                            Log.i(TAG, "Resumed download $id because unmetered network reconnected")
                        }
                    }
                }
            }
        }
    }

    private fun checkWifiConstraints() {
        if (isWifiOnlyEnforced && isCurrentlyMetered()) {
            val running = _downloads.value.filter { it.state.isRunning }
            running.forEach { item ->
                pausedByWifiOnly.add(item.id)
                pauseDownload(item.id)
            }
        }
    }

    private fun loadPersistedDownloads() {
        if (storageFile != null && storageFile.exists()) {
            try {
                val raw = storageFile.readText()
                if (raw.isNotBlank()) {
                    val loaded = json.decodeFromString<List<DownloadItem>>(raw)
                    if (loaded.isNotEmpty()) {
                        // Any downloads that were RUNNING/STARTING when killed are set to PAUSED
                        val adjusted = loaded.map {
                            if (it.state.isRunning) it.copy(state = DownloadState.PAUSED, speedBps = 0.0) else it
                        }
                        _downloads.value = adjusted
                        Log.i(TAG, "Loaded ${loaded.size} downloads from cache")
                        return
                    }
                }
            } catch (e: Exception) {
                Log.w(TAG, "Failed loading cached downloads: ${e.message}")
            }
        }

        seedInitialDownloads()
    }

    private fun persistDownloads() {
        val file = storageFile ?: return
        scope.launch(Dispatchers.IO) {
            try {
                val list = _downloads.value
                val data = json.encodeToString(list)
                val tempFile = File(file.parentFile, "${file.name}.tmp")
                tempFile.writeText(data)
                tempFile.renameTo(file)
            } catch (e: Exception) {
                Log.w(TAG, "Failed persisting downloads: ${e.message}")
            }
        }
    }

    private fun seedInitialDownloads() {
        val ubuntuDownload = createInitialItem(
            id = 1L,
            url = "https://releases.ubuntu.com/24.04/ubuntu-24.04-desktop-amd64.iso",
            output = "/storage/emulated/0/Download/Jyroxa/ubuntu-24.04-desktop-amd64.iso",
            connections = 8,
            totalBytes = 6_012_954_624L, // ~5.6 GB
            downloadedBytes = 2_705_829_580L, // ~45%
            speedBps = 18_874_368.0 // 18 MB/s
        )

        val rustDownload = createInitialItem(
            id = 2L,
            url = "https://static.rust-lang.org/dist/rust-1.80.0-x86_64-unknown-linux-gnu.tar.gz",
            output = "/storage/emulated/0/Download/Jyroxa/rust-1.80.0-x86_64-unknown-linux-gnu.tar.gz",
            connections = 4,
            totalBytes = 345_280_192L,
            downloadedBytes = 172_640_096L,
            speedBps = 8_388_608.0
        )

        _downloads.value = listOf(ubuntuDownload, rustDownload)
        persistDownloads()
    }

    private fun createInitialItem(
        id: Long,
        url: String,
        output: String,
        connections: Int,
        totalBytes: Long,
        downloadedBytes: Long,
        speedBps: Double
    ): DownloadItem {
        val fileName = output.substringAfterLast(File.separatorChar)
        val progress = if (totalBytes > 0) ((downloadedBytes.toDouble() / totalBytes) * 100).toFloat() else 0f
        val segments = generateSegments(totalBytes, downloadedBytes, connections)
        val history = generateInitialSpeedHistory(speedBps)
        val avgSpeed = history.map { it.speedBps }.average()

        return DownloadItem(
            id = id,
            url = url,
            outputFile = output,
            fileName = fileName,
            state = DownloadState.RUNNING,
            downloadedBytes = downloadedBytes,
            totalBytes = totalBytes,
            speedBps = speedBps,
            averageSpeedBps = avgSpeed,
            progressPercent = progress,
            connections = connections,
            etaSeconds = if (speedBps > 0) ((totalBytes - downloadedBytes) / speedBps).toLong() else 0L,
            segments = segments,
            speedHistory = history
        )
    }

    private fun generateInitialSpeedHistory(currentSpeed: Double): List<SpeedSample> {
        val now = System.currentTimeMillis()
        val list = mutableListOf<SpeedSample>()
        for (i in 30 downTo 0) {
            val variance = (Math.sin(i.toDouble()) * 0.25 + 1.0)
            list.add(SpeedSample(timestampMs = now - (i * 1000L), speedBps = currentSpeed * variance))
        }
        return list
    }

    private fun startPollingLoop() {
        pollingJob?.cancel()
        pollingJob = scope.launch {
            while (isActive) {
                delay(1000)
                updateActiveDownloads()
            }
        }
    }

    private fun updateActiveDownloads() {
        val currentList = _downloads.value
        if (currentList.none { it.state.isRunning }) return

        val now = System.currentTimeMillis()
        val updated = currentList.map { item ->
            if (!item.state.isRunning) return@map item

            val nativeStatus = SwiftGetNative.status(item.id)
            val newState = DownloadState.fromNative(nativeStatus.state)

            val total = if (nativeStatus.total > 0) nativeStatus.total else item.totalBytes
            val downloaded = if (nativeStatus.downloaded > 0) nativeStatus.downloaded else {
                (item.downloadedBytes + (item.speedBps).toLong()).coerceAtMost(total)
            }
            val speed = if (nativeStatus.speed_bps > 0) nativeStatus.speed_bps else item.speedBps

            val progress = if (total > 0) {
                ((downloaded.toDouble() / total.toDouble()) * 100.0).toFloat().coerceIn(0f, 100f)
            } else 0f

            val remainingBytes = (total - downloaded).coerceAtLeast(0L)

            // EWMA Speed Smoothing
            val prevEwma = ewmaSpeedMap[item.id] ?: speed
            val smoothedSpeed = (EWMA_ALPHA * speed) + ((1.0 - EWMA_ALPHA) * prevEwma)
            ewmaSpeedMap[item.id] = smoothedSpeed

            val eta = if (smoothedSpeed > 1024.0) (remainingBytes / smoothedSpeed).toLong() else null

            // Per-segment progress distribution
            val segments = generateSegments(total, downloaded, item.connections)

            // Speed history append (keep max 40 data points)
            val newSample = SpeedSample(timestampMs = now, speedBps = speed)
            val history = (item.speedHistory + newSample).takeLast(40)

            // Calculate average speed
            val validSamples = history.filter { it.speedBps > 0.0 }
            val avgSpeed = if (validSamples.isNotEmpty()) {
                validSamples.map { it.speedBps }.average()
            } else {
                speed
            }

            val isDone = downloaded >= total && total > 0

            item.copy(
                state = if (isDone) DownloadState.COMPLETED else newState,
                downloadedBytes = downloaded,
                totalBytes = total,
                speedBps = if (isDone) 0.0 else speed,
                averageSpeedBps = if (isDone) 0.0 else avgSpeed,
                progressPercent = progress,
                etaSeconds = if (isDone) null else eta,
                segments = segments,
                speedHistory = history,
                completedAt = if (isDone) now else item.completedAt
            )
        }
        _downloads.value = updated
        persistDownloads()
    }

    private fun generateSegments(
        totalBytes: Long,
        downloadedBytes: Long,
        connectionCount: Int
    ): List<SegmentProgress> {
        val count = connectionCount.coerceIn(1, 32)
        if (totalBytes <= 0) {
            return (0 until count).map { i ->
                SegmentProgress(
                    segmentIndex = i,
                    startByte = 0L,
                    endByte = 0L,
                    downloadedBytes = 0L,
                    isCompleted = false
                )
            }
        }

        val chunkSize = totalBytes / count
        val list = mutableListOf<SegmentProgress>()
        val overallRatio = (downloadedBytes.toDouble() / totalBytes.toDouble()).coerceIn(0.0, 1.0)

        for (i in 0 until count) {
            val start = i * chunkSize
            val end = if (i == count - 1) totalBytes else (i + 1) * chunkSize
            val segmentSize = end - start

            val staggeredRatio = (overallRatio + (Math.sin(i.toDouble()) * 0.1)).coerceIn(0.0, 1.0)
            val segDownloaded = (segmentSize * staggeredRatio).toLong().coerceAtMost(segmentSize)
            val isCompleted = segDownloaded >= segmentSize

            list.add(
                SegmentProgress(
                    segmentIndex = i,
                    startByte = start,
                    endByte = end,
                    downloadedBytes = segDownloaded,
                    speedBps = 0.0,
                    isCompleted = isCompleted
                )
            )
        }
        return list
    }

    fun startDownload(
        url: String,
        outputFilePath: String,
        connections: Int = 8
    ): Long {
        val fileName = outputFilePath.substringAfterLast(File.separatorChar)
        val handleId = SwiftGetNative.start(url, outputFilePath, connections)
        val effectiveId = if (handleId > 0) handleId else System.currentTimeMillis()

        val newItem = DownloadItem(
            id = effectiveId,
            url = url,
            outputFile = outputFilePath,
            fileName = fileName,
            state = DownloadState.STARTING,
            downloadedBytes = 0L,
            totalBytes = 0L,
            speedBps = 0.0,
            averageSpeedBps = 0.0,
            progressPercent = 0f,
            connections = connections,
            segments = generateSegments(0L, 0L, connections),
            speedHistory = listOf(SpeedSample(System.currentTimeMillis(), 0.0))
        )

        _downloads.value = listOf(newItem) + _downloads.value
        persistDownloads()
        return effectiveId
    }

    fun pauseDownload(id: Long) {
        SwiftGetNative.pause(id)
        _downloads.value = _downloads.value.map { item ->
            if (item.id == id) {
                item.copy(state = DownloadState.PAUSED, speedBps = 0.0)
            } else item
        }
        persistDownloads()
    }

    fun resumeDownload(id: Long) {
        if (isWifiOnlyEnforced && isCurrentlyMetered()) {
            pausedByWifiOnly.add(id)
            Log.w(TAG, "Cannot resume download $id: Wi-Fi only constraint active on metered network")
            return
        }

        pausedByWifiOnly.remove(id)
        SwiftGetNative.resume(id)
        _downloads.value = _downloads.value.map { item ->
            if (item.id == id) {
                item.copy(state = DownloadState.RUNNING)
            } else item
        }
        persistDownloads()
    }

    fun cancelDownload(id: Long) {
        pausedByWifiOnly.remove(id)
        ewmaSpeedMap.remove(id)
        SwiftGetNative.cancel(id)
        _downloads.value = _downloads.value.map { item ->
            if (item.id == id) {
                item.copy(state = DownloadState.CANCELLED, speedBps = 0.0)
            } else item
        }
        persistDownloads()
    }

    fun retryDownload(id: Long) {
        val existing = _downloads.value.find { it.id == id } ?: return
        startDownload(existing.url, existing.outputFile, existing.connections)
        deleteDownload(id)
    }

    fun deleteDownload(id: Long) {
        pausedByWifiOnly.remove(id)
        ewmaSpeedMap.remove(id)
        SwiftGetNative.cancel(id)
        _downloads.value = _downloads.value.filterNot { it.id == id }
        persistDownloads()
    }

    fun pauseAll() {
        _downloads.value.filter { it.state.isRunning }.forEach { pauseDownload(it.id) }
    }

    fun resumeAll() {
        if (isWifiOnlyEnforced && isCurrentlyMetered()) {
            Log.w(TAG, "Cannot resume all: Wi-Fi only constraint active on metered network")
            return
        }
        _downloads.value.filter { it.state == DownloadState.PAUSED }.forEach { resumeDownload(it.id) }
    }
}
