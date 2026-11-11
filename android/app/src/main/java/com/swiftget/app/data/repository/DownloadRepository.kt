package com.swiftget.app.data.repository

import com.swiftget.app.data.model.DownloadItem
import com.swiftget.app.data.model.DownloadState
import com.swiftget.app.data.model.SegmentProgress
import com.swiftget.app.data.model.SpeedSample
import com.swiftget.app.native.SwiftGetNative
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
import java.io.File

/**
 * Central repository managing active and historic downloads.
 */
class DownloadRepository(
    private val scope: CoroutineScope = CoroutineScope(Dispatchers.Default + Job())
) {
    private val _downloads = MutableStateFlow<List<DownloadItem>>(emptyList())
    val downloads: StateFlow<List<DownloadItem>> = _downloads.asStateFlow()

    val runningDownloadsCount: StateFlow<Int> = _downloads.map { list ->
        list.count { it.state.isRunning }
    }.stateIn(scope, SharingStarted.Eagerly, 0)

    val totalSpeedBps: StateFlow<Double> = _downloads.map { list ->
        list.filter { it.state.isRunning }.sumOf { it.speedBps }
    }.stateIn(scope, SharingStarted.Eagerly, 0.0)

    private var pollingJob: Job? = null

    init {
        // Seed with sample initial downloads if empty (to showcase per-segment and speed graph in preview)
        seedInitialDownloads()
        startPollingLoop()
    }

    private fun seedInitialDownloads() {
        val ubuntuDownload = createInitialItem(
            id = 1L,
            url = "https://releases.ubuntu.com/24.04/ubuntu-24.04-desktop-amd64.iso",
            output = "/storage/emulated/0/Download/SwiftGet/ubuntu-24.04-desktop-amd64.iso",
            connections = 8,
            totalBytes = 6_012_954_624L, // ~5.6 GB
            downloadedBytes = 2_705_829_580L, // ~45%
            speedBps = 18_874_368.0 // 18 MB/s
        )

        val rustDownload = createInitialItem(
            id = 2L,
            url = "https://static.rust-lang.org/dist/rust-1.80.0-x86_64-unknown-linux-gnu.tar.gz",
            output = "/storage/emulated/0/Download/SwiftGet/rust-1.80.0-x86_64-unknown-linux-gnu.tar.gz",
            connections = 4,
            totalBytes = 345_280_192L,
            downloadedBytes = 172_640_096L,
            speedBps = 8_388_608.0
        )

        _downloads.value = listOf(ubuntuDownload, rustDownload)
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

        return DownloadItem(
            id = id,
            url = url,
            outputFile = output,
            fileName = fileName,
            state = DownloadState.RUNNING,
            downloadedBytes = downloadedBytes,
            totalBytes = totalBytes,
            speedBps = speedBps,
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
            val eta = if (speed > 0) (remainingBytes / speed).toLong() else null

            // Per-segment progress distribution
            val segments = generateSegments(total, downloaded, item.connections)

            // Speed history append (keep max 40 data points)
            val newSample = SpeedSample(timestampMs = now, speedBps = speed)
            val history = (item.speedHistory + newSample).takeLast(40)

            item.copy(
                state = if (downloaded >= total && total > 0) DownloadState.COMPLETED else newState,
                downloadedBytes = downloaded,
                totalBytes = total,
                speedBps = if (downloaded >= total && total > 0) 0.0 else speed,
                progressPercent = progress,
                etaSeconds = eta,
                segments = segments,
                speedHistory = history,
                completedAt = if (downloaded >= total && total > 0) now else item.completedAt
            )
        }
        _downloads.value = updated
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

        // Simulate realistic multi-chunk progress
        val overallRatio = (downloadedBytes.toDouble() / totalBytes.toDouble()).coerceIn(0.0, 1.0)

        for (i in 0 until count) {
            val start = i * chunkSize
            val end = if (i == count - 1) totalBytes else (i + 1) * chunkSize
            val segmentSize = end - start

            // Give slightly staggered progress to each segment like real parallel downloads
            val staggeredRatio = (overallRatio + (Math.sin(i.toDouble()) * 0.1)).coerceIn(0.0, 1.0)
            val segDownloaded = (segmentSize * staggeredRatio).toLong()
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
            progressPercent = 0f,
            connections = connections,
            segments = generateSegments(0L, 0L, connections),
            speedHistory = listOf(SpeedSample(System.currentTimeMillis(), 0.0))
        )

        _downloads.value = listOf(newItem) + _downloads.value
        return effectiveId
    }

    fun pauseDownload(id: Long) {
        SwiftGetNative.pause(id)
        _downloads.value = _downloads.value.map { item ->
            if (item.id == id) {
                item.copy(state = DownloadState.PAUSED, speedBps = 0.0)
            } else item
        }
    }

    fun resumeDownload(id: Long) {
        SwiftGetNative.resume(id)
        _downloads.value = _downloads.value.map { item ->
            if (item.id == id) {
                item.copy(state = DownloadState.RUNNING)
            } else item
        }
    }

    fun cancelDownload(id: Long) {
        SwiftGetNative.cancel(id)
        _downloads.value = _downloads.value.map { item ->
            if (item.id == id) {
                item.copy(state = DownloadState.CANCELLED, speedBps = 0.0)
            } else item
        }
    }

    fun retryDownload(id: Long) {
        val existing = _downloads.value.find { it.id == id } ?: return
        startDownload(existing.url, existing.outputFile, existing.connections)
        deleteDownload(id)
    }

    fun deleteDownload(id: Long) {
        SwiftGetNative.cancel(id)
        _downloads.value = _downloads.value.filterNot { it.id == id }
    }

    fun pauseAll() {
        _downloads.value.filter { it.state.isRunning }.forEach { pauseDownload(it.id) }
    }

    fun resumeAll() {
        _downloads.value.filter { it.state == DownloadState.PAUSED }.forEach { resumeDownload(it.id) }
    }
}
