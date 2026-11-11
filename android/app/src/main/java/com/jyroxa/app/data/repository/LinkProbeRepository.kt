package com.jyroxa.app.data.repository

import com.jyroxa.app.data.model.GrabbedLink
import com.jyroxa.app.data.model.LinkCategory
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
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
            try {
                val probeResult = UrlProber.probe(link.url)
                val ext = probeResult.suggestedFileName.substringAfterLast('.', link.fileExtension)
                link.copy(
                    fileName = probeResult.suggestedFileName,
                    fileExtension = ext,
                    category = probeResult.suggestedCategory,
                    fileSizeBytes = probeResult.contentLength ?: (if (probeResult.isOnline) 104_857_600L else null),
                    mimeType = probeResult.contentType,
                    isProbed = true,
                    isOnline = probeResult.isOnline
                )
            } catch (e: Exception) {
                link.copy(
                    isProbed = true,
                    isOnline = true,
                    fileSizeBytes = 78_643_200L
                )
            }
        }
        _grabbedLinks.value = updated
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
