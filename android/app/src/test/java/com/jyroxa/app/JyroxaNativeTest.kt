package com.jyroxa.app

import com.jyroxa.app.native.JyroxaNative
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class JyroxaNativeTest {

    @Test
    fun testFallbackLifecycle() {
        val handle = JyroxaNative.start("https://example.com/test.bin", "/tmp/test.bin", 4)
        assertTrue(handle > 0)

        val status = JyroxaNative.status(handle)
        assertEquals(handle, status.id)
        assertNotNull(status.state)

        val paused = JyroxaNative.pause(handle)
        assertTrue(paused)

        val resumed = JyroxaNative.resume(handle)
        assertTrue(resumed)

        val cancelled = JyroxaNative.cancel(handle)
        assertTrue(cancelled)
    }

    @Test
    fun testUnknownHandleStatus() {
        val status = JyroxaNative.status(999_999L)
        assertEquals(999_999L, status.id)
        assertEquals("unknown", status.state)
    }
}
