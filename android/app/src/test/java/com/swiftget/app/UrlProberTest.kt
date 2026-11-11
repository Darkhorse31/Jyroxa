package com.swiftget.app

import com.swiftget.app.data.model.DownloadItem
import com.swiftget.app.data.model.DownloadState
import com.swiftget.app.data.repository.UrlProber
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class UrlProberTest {

    @get:Rule
    val tempFolder = TemporaryFolder()

    @Test
    fun testRfc5987ContentDispositionParsing() {
        val header = "attachment; filename*=UTF-8''na%C3%AFve%20document%202026.pdf"
        val fileName = UrlProber.extractFileNameFromDisposition(header)
        assertEquals("naïve document 2026.pdf", fileName)
    }

    @Test
    fun testStandardContentDispositionParsing() {
        val header = "attachment; filename=\"ubuntu-24.04-desktop-amd64.iso\""
        val fileName = UrlProber.extractFileNameFromDisposition(header)
        assertEquals("ubuntu-24.04-desktop-amd64.iso", fileName)
    }

    @Test
    fun testUrlPathExtraction() {
        val url = "https://example.com/downloads/my%20archive%20v1.0.zip?token=xyz123#fragment"
        val fileName = UrlProber.extractFileNameFromUrl(url)
        assertEquals("my archive v1.0.zip", fileName)
    }

    @Test
    fun testSanitizeFileName() {
        val dangerous = "my/illegal:name*with?bad<chars>|test\u0000.tar.gz"
        val sanitized = UrlProber.sanitizeFileName(dangerous)
        assertEquals("my_illegal_name_with_bad_chars_test.tar.gz", sanitized)
    }

    @Test
    fun testAppendMissingExtensionFromMime() {
        val noExt = "archive_package"
        val withExt = UrlProber.appendMissingExtension(noExt, "application/zip")
        assertEquals("archive_package.zip", withExt)

        val genericExt = "document.bin"
        val fixedExt = UrlProber.appendMissingExtension(genericExt, "application/pdf")
        assertEquals("document.pdf", fixedExt)
    }

    @Test
    fun testDeduplicateFileName() {
        val dir = tempFolder.newFolder("downloads")
        File(dir, "report.pdf").createNewFile()

        val dedupe1 = UrlProber.deduplicateFileName(dir, "report.pdf")
        assertEquals("report (1).pdf", dedupe1)

        File(dir, "report (1).pdf").createNewFile()
        val dedupe2 = UrlProber.deduplicateFileName(dir, "report.pdf")
        assertEquals("report (2).pdf", dedupe2)
    }

    @Test
    fun testEwmaEtaFormatting() {
        val item = DownloadItem(
            id = 10L,
            url = "https://example.com/file.zip",
            outputFile = "/tmp/file.zip",
            fileName = "file.zip",
            state = DownloadState.RUNNING,
            downloadedBytes = 1000L,
            totalBytes = 5000L,
            speedBps = 1000.0,
            averageSpeedBps = 900.0,
            etaSeconds = 135L // 2m 15s
        )

        assertEquals("2m 15s left", item.formattedEta)
        assertTrue(item.formattedSpeedAndAvg.contains("avg"))
    }
}
