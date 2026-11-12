package com.jyroxa.app

import com.jyroxa.app.data.model.DownloadItem
import com.jyroxa.app.data.model.DownloadState
import com.jyroxa.app.data.repository.nextDownloadId
import com.jyroxa.app.data.repository.sanitizeLoaded
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class DownloadIdsTest {
    private fun item(id: Long, url: String = "https://example.com/$id.bin", state: DownloadState = DownloadState.COMPLETED) =
        DownloadItem(id = id, url = url, outputFile = "/tmp/$id.bin", fileName = "$id.bin", state = state)

    @Test
    fun `v011 demo rows are dropped and duplicate ids collapse`() {
        val saved = listOf(
            item(1, "https://releases.ubuntu.com/24.04/ubuntu-24.04-desktop-amd64.iso", DownloadState.RUNNING),
            item(2, "https://static.rust-lang.org/dist/rust-1.80.0-x86_64-unknown-linux-gnu.tar.gz", DownloadState.RUNNING),
            item(1, "https://proof.ovh.net/files/10Mb.dat", DownloadState.RUNNING),
            item(1, "https://proof.ovh.net/files/10Mb.dat", DownloadState.RUNNING),
        )
        val cleaned = sanitizeLoaded(saved)
        assertEquals(1, cleaned.size)
        assertEquals("https://proof.ovh.net/files/10Mb.dat", cleaned[0].url)
        assertEquals(DownloadState.PAUSED, cleaned[0].state)
    }

    @Test
    fun `new ids never collide with saved ones`() {
        val saved = listOf(item(1), item(5_000))
        assertEquals(5_001L, nextDownloadId(saved, nowMs = 10))
        val id = nextDownloadId(saved, nowMs = 1_700_000_000_000)
        assertTrue(saved.none { it.id == id })
        assertEquals(42L, nextDownloadId(emptyList(), nowMs = 42))
    }
}