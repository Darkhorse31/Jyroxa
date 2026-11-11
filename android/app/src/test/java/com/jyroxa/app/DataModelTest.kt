package com.jyroxa.app

import com.jyroxa.app.data.model.DownloadItem
import com.jyroxa.app.data.model.DownloadState
import com.jyroxa.app.data.model.GrabbedLink
import com.jyroxa.app.data.model.LinkCategory
import com.jyroxa.app.data.model.SegmentProgress
import com.jyroxa.app.data.repository.LinkProbeRepository
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DataModelTest {

    @Test
    fun testDownloadStateMapping() {
        assertEquals(DownloadState.RUNNING, DownloadState.fromNative("running"))
        assertEquals(DownloadState.RUNNING, DownloadState.fromNative("downloading"))
        assertEquals(DownloadState.STARTING, DownloadState.fromNative("starting"))
        assertEquals(DownloadState.PAUSED, DownloadState.fromNative("paused"))
        assertEquals(DownloadState.COMPLETED, DownloadState.fromNative("completed"))
        assertEquals(DownloadState.FAILED, DownloadState.fromNative("failed: 404 Not Found"))
        assertEquals(DownloadState.CANCELLED, DownloadState.fromNative("cancelled"))
    }

    @Test
    fun testSegmentProgressCalculation() {
        val segment = SegmentProgress(
            segmentIndex = 0,
            startByte = 0L,
            endByte = 1000L,
            downloadedBytes = 500L
        )
        assertEquals(1000L, segment.totalBytes)
        assertEquals(50f, segment.progressPercent, 0.01f)
        assertFalse(segment.isCompleted)

        val completedSeg = segment.copy(downloadedBytes = 1000L, isCompleted = true)
        assertEquals(100f, completedSeg.progressPercent, 0.01f)
        assertTrue(completedSeg.isCompleted)
    }

    @Test
    fun testByteFormatting() {
        assertEquals("0 B", DownloadItem.formatBytes(0))
        assertEquals("500 B", DownloadItem.formatBytes(500))
        assertEquals("1.0 KB", DownloadItem.formatBytes(1024))
        assertEquals("10.00 MB", DownloadItem.formatBytes(10 * 1024 * 1024))
        assertEquals("2.50 GB", DownloadItem.formatBytes((2.5 * 1024 * 1024 * 1024).toLong()))
    }

    @Test
    fun testSpeedFormatting() {
        assertEquals("0 B/s", DownloadItem.formatBytesSpeed(0.0))
        assertEquals("512.0 KB/s", DownloadItem.formatBytesSpeed(512.0 * 1024.0))
        assertEquals("15.50 MB/s", DownloadItem.formatBytesSpeed(15.5 * 1024.0 * 1024.0))
    }

    @Test
    fun testGrabbedLinkCategoryDetection() {
        assertEquals(LinkCategory.VIDEO, GrabbedLink.detectCategory("mp4"))
        assertEquals(LinkCategory.VIDEO, GrabbedLink.detectCategory("mkv"))
        assertEquals(LinkCategory.AUDIO, GrabbedLink.detectCategory("mp3"))
        assertEquals(LinkCategory.AUDIO, GrabbedLink.detectCategory("flac"))
        assertEquals(LinkCategory.ARCHIVE, GrabbedLink.detectCategory("zip"))
        assertEquals(LinkCategory.ARCHIVE, GrabbedLink.detectCategory("tar"))
        assertEquals(LinkCategory.DOCUMENT, GrabbedLink.detectCategory("pdf"))
        assertEquals(LinkCategory.IMAGE, GrabbedLink.detectCategory("png"))
        assertEquals(LinkCategory.EXECUTABLE, GrabbedLink.detectCategory("apk"))
    }

    @Test
    fun testUrlExtractionFromText() {
        val repo = LinkProbeRepository()
        val text = """
            Here is the download link: https://example.com/files/archive.zip
            Also check out the video at http://cdn.test.org/media/sample.mp4 and mirrors.
        """.trimIndent()

        val urls = repo.extractUrls(text)
        assertEquals(2, urls.size)
        assertTrue(urls.contains("https://example.com/files/archive.zip"))
        assertTrue(urls.contains("http://cdn.test.org/media/sample.mp4"))
    }
}
