package com.swiftget.app

import com.swiftget.app.native.SwiftGetNative
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SwiftGetNativeTest {

    @Test
    fun testFallbackLifecycle() {
        val handle = SwiftGetNative.start("https://example.com/test.bin", "/tmp/test.bin", 4)
        assertTrue(handle > 0)

        val status = SwiftGetNative.status(handle)
        assertEquals(handle, status.id)
        assertNotNull(status.state)

        val paused = SwiftGetNative.pause(handle)
        assertTrue(paused)

        val resumed = SwiftGetNative.resume(handle)
        assertTrue(resumed)

        val cancelled = SwiftGetNative.cancel(handle)
        assertTrue(cancelled)
    }

    @Test
    fun testUnknownHandleStatus() {
        val status = SwiftGetNative.status(999_999L)
        assertEquals(999_999L, status.id)
        assertEquals("unknown", status.state)
    }
}
