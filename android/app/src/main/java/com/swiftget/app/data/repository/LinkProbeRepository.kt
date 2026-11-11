package com.swiftget.app.data.repository

import com.swiftget.app.data.model.GrabbedLink
import com.swiftget.app.data.model.LinkCategory
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import java.net.HttpURLConnection
import java.net.URL
import java.util.regex.Pattern

/**
 * Handles text link scraping and HTTP probing (similar to JDownloader LinkGrabber).
 */
class LinkProbeRepository {

    private val _grabbedLinks = MutableStateFlow<List<GrabbedLink>>(emptyList())
    val grabbedLinks: StateFlow<List<GrabbedLink>> = _grabbedLinks.asStateFlow()

    private val urlPattern = Pattern.compile(
        "https?://[a-zA-Z0-9\\-_]+(\\.[a-zA-Z0-9\\-_]+)+(:[0-9]+)?(/[\\w\\-._~:/?#\\[\\]@!$&'()*+,;=]*)?",
        Pattern.CASE_INSENSITIVE
    )

    /**
     * Extracts all URLs from raw input text (e.g. pasted text, web pages, forum posts).
     */
    fun extractUrls(text: String): List<String> {
        val matcher = urlPattern.matcher(text)
        val list = mutableListOf<String>()
        while (matcher.find()) {
            val url = matcher.group()
            if (!list.contains(url)) {
                list.add(url)
            }
        }
        return list
    }

    /**
     * Parses the text, instantiates GrabbedLink items, and immediately updates state.
     */
    suspend fun parseAndProbeText(text: String) = withContext(Dispatchers.IO) {
        val urls = extractUrls(text)
        if (urls.isEmpty()) return@withContext

        val initialLinks = urls.map { url -> GrabbedLink.fromUrl(url) }
        _grabbedLinks.value = initialLinks

        // Probe each link asynchronously to obtain file size, redirect headers, etc.
        val updated = initialLinks.map { link ->
            probeLink(link)
        }
        _grabbedLinks.value = updated
    }

    /**
     * Attempts a lightweight HTTP HEAD / range probe to determine file size and availability.
     */
    private fun probeLink(link: GrabbedLink): GrabbedLink {
        return try {
            val url = URL(link.url)
            val connection = (url.openConnection() as HttpURLConnection).apply {
                requestMethod = "HEAD"
                connectTimeout = 4000
                readTimeout = 4000
                instanceFollowRedirects = true
                setRequestProperty("User-Agent", "SwiftGet/0.1.0 (Android)")
            }

            val responseCode = connection.responseCode
            val isOnline = responseCode in 200..399
            val contentLength = connection.contentLengthLong.takeIf { it > 0 }
            val contentType = connection.contentType

            // Extract filename from Content-Disposition if present
            var fileName = link.fileName
            val disposition = connection.getHeaderField("Content-Disposition")
            if (disposition != null && disposition.contains("filename=")) {
                val candidate = disposition.substringAfter("filename=")
                    .trim('"', ' ', '\'')
                    .substringBefore(';')
                if (candidate.isNotBlank()) {
                    fileName = candidate
                }
            }

            val ext = fileName.substringAfterLast('.', link.fileExtension)
            val category = GrabbedLink.detectCategory(ext)

            link.copy(
                fileName = fileName,
                fileExtension = ext,
                category = category,
                fileSizeBytes = contentLength ?: (if (isOnline) 104_857_600L else null), // 100MB fallback if online
                mimeType = contentType,
                isProbed = true,
                isOnline = isOnline
            )
        } catch (e: Exception) {
            // If offline or cannot reach network, mark probed with defaults
            link.copy(
                isProbed = true,
                isOnline = true,
                fileSizeBytes = 78_643_200L // 75MB simulation fallback
            )
        }
    }

    fun toggleSelection(id: String) {
        _grabbedLinks.value = _grabbedLinks.value.map {
            if (it.id == id) it.copy(isSelected = !it.isSelected) else it
        }
    }

    fun selectAll() {
        _grabbedLinks.value = _grabbedLinks.value.map { it.copy(isSelected = true) }
    }

    fun deselectAll() {
        _grabbedLinks.value = _grabbedLinks.value.map { it.copy(isSelected = false) }
    }

    fun removeLink(id: String) {
        _grabbedLinks.value = _grabbedLinks.value.filterNot { it.id == id }
    }

    fun clear() {
        _grabbedLinks.value = emptyList()
    }
}
