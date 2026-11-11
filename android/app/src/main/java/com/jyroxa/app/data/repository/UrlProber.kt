package com.jyroxa.app.data.repository

import android.util.Log
import com.jyroxa.app.data.model.GrabbedLink
import com.jyroxa.app.data.model.LinkCategory
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.net.HttpURLConnection
import java.net.URI
import java.net.URL
import java.net.URLDecoder
import java.util.Locale
import java.util.regex.Pattern

/**
 * Result of probing a download URL.
 */
data class ProbeResult(
    val initialUrl: String,
    val finalUrl: String,
    val suggestedFileName: String,
    val suggestedCategory: LinkCategory,
    val suggestedSubfolder: String,
    val contentLength: Long?,
    val contentType: String?,
    val isRangeSupported: Boolean,
    val isOnline: Boolean,
    val responseCode: Int
)

/**
 * Probes HTTP/HTTPS URLs (HEAD followed by GET Range: bytes=0-0)
 * and extracts/sanitizes filenames according to RFC 5987 / URL heuristics.
 */
object UrlProber {
    private const val TAG = "UrlProber"
    private const val USER_AGENT =
        "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Mobile Safari/537.36"

    private val MIME_TO_EXT = mapOf(
        "application/pdf" to "pdf",
        "application/zip" to "zip",
        "application/x-zip-compressed" to "zip",
        "application/x-rar-compressed" to "rar",
        "application/vnd.rar" to "rar",
        "application/x-7z-compressed" to "7z",
        "application/x-tar" to "tar",
        "application/gzip" to "gz",
        "application/x-gzip" to "gz",
        "application/x-bzip2" to "bz2",
        "application/x-xz" to "xz",
        "application/vnd.android.package-archive" to "apk",
        "application/x-iso9660-image" to "iso",
        "application/json" to "json",
        "application/xml" to "xml",
        "application/msword" to "doc",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" to "docx",
        "application/vnd.ms-excel" to "xls",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" to "xlsx",
        "application/vnd.ms-powerpoint" to "ppt",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" to "pptx",
        "video/mp4" to "mp4",
        "video/x-matroska" to "mkv",
        "video/webm" to "webm",
        "video/x-msvideo" to "avi",
        "video/quicktime" to "mov",
        "audio/mpeg" to "mp3",
        "audio/mp3" to "mp3",
        "audio/flac" to "flac",
        "audio/x-flac" to "flac",
        "audio/wav" to "wav",
        "audio/x-wav" to "wav",
        "audio/aac" to "aac",
        "audio/ogg" to "ogg",
        "audio/opus" to "opus",
        "image/jpeg" to "jpg",
        "image/png" to "png",
        "image/gif" to "gif",
        "image/webp" to "webp",
        "image/svg+xml" to "svg",
        "text/plain" to "txt",
        "text/html" to "html",
        "text/csv" to "csv"
    )

    /**
     * Executes asynchronous probing of the target URL.
     */
    suspend fun probe(
        targetUrl: String,
        targetDirectory: String? = null
    ): ProbeResult = withContext(Dispatchers.IO) {
        val trimmed = targetUrl.trim()
        var currentUrl = trimmed
        var connection: HttpURLConnection? = null
        var finalUrl = trimmed
        var responseCode = -1
        var contentDisposition: String? = null
        var contentLength: Long? = null
        var contentType: String? = null
        var acceptRanges: String? = null

        // 1. First attempt: HTTP HEAD with redirect following
        try {
            var redirects = 0
            while (redirects < 5) {
                val urlObj = URL(currentUrl)
                connection = (urlObj.openConnection() as HttpURLConnection).apply {
                    requestMethod = "HEAD"
                    connectTimeout = 5000
                    readTimeout = 5000
                    instanceFollowRedirects = false
                    setRequestProperty("User-Agent", USER_AGENT)
                    setRequestProperty("Accept-Encoding", "identity")
                }

                responseCode = connection.responseCode
                if (responseCode in 300..399) {
                    val location = connection.getHeaderField("Location")
                    if (!location.isNullOrBlank()) {
                        val baseUri = URI(currentUrl)
                        currentUrl = baseUri.resolve(location).toString()
                        finalUrl = currentUrl
                        connection.disconnect()
                        redirects++
                        continue
                    }
                }
                break
            }

            finalUrl = currentUrl
            contentDisposition = connection?.getHeaderField("Content-Disposition")
            val len = connection?.contentLengthLong ?: -1L
            contentLength = if (len > 0L) len else null
            contentType = connection?.contentType
            acceptRanges = connection?.getHeaderField("Accept-Ranges")

        } catch (e: Exception) {
            Log.w(TAG, "HEAD probe failed for $trimmed: ${e.message}")
        } finally {
            try {
                connection?.disconnect()
            } catch (_: Exception) {}
        }

        // 2. Second attempt: If HEAD returned error, 405 Method Not Allowed, or no size/disposition, probe via GET Range 0-0
        val isHeadSufficient = responseCode in 200..299 && (contentDisposition != null || contentLength != null)
        if (!isHeadSufficient) {
            try {
                var redirects = 0
                currentUrl = finalUrl
                while (redirects < 5) {
                    val urlObj = URL(currentUrl)
                    connection = (urlObj.openConnection() as HttpURLConnection).apply {
                        requestMethod = "GET"
                        connectTimeout = 6000
                        readTimeout = 6000
                        instanceFollowRedirects = false
                        setRequestProperty("User-Agent", USER_AGENT)
                        setRequestProperty("Range", "bytes=0-0")
                        setRequestProperty("Accept-Encoding", "identity")
                    }

                    responseCode = connection.responseCode
                    if (responseCode in 300..399) {
                        val location = connection.getHeaderField("Location")
                        if (!location.isNullOrBlank()) {
                            val baseUri = URI(currentUrl)
                            currentUrl = baseUri.resolve(location).toString()
                            finalUrl = currentUrl
                            connection.disconnect()
                            redirects++
                            continue
                        }
                    }
                    break
                }

                finalUrl = currentUrl
                if (contentDisposition.isNullOrBlank()) {
                    contentDisposition = connection?.getHeaderField("Content-Disposition")
                }
                if (contentType.isNullOrBlank()) {
                    contentType = connection?.contentType
                }
                if (acceptRanges.isNullOrBlank()) {
                    acceptRanges = connection?.getHeaderField("Accept-Ranges")
                }

                // If 206 Partial Content was returned, check Content-Range for total length: "bytes 0-0/1234567"
                val contentRange = connection?.getHeaderField("Content-Range")
                if (contentRange != null && contentRange.contains("/")) {
                    val totalStr = contentRange.substringAfterLast("/").trim()
                    val parsedTotal = totalStr.toLongOrNull()
                    if (parsedTotal != null && parsedTotal > 0L) {
                        contentLength = parsedTotal
                    }
                } else if (contentLength == null) {
                    val len = connection?.contentLengthLong ?: -1L
                    if (len > 0L) contentLength = len
                }

            } catch (e: Exception) {
                Log.w(TAG, "GET Range probe failed for $currentUrl: ${e.message}")
            } finally {
                try {
                    connection?.inputStream?.close()
                } catch (_: Exception) {}
                try {
                    connection?.disconnect()
                } catch (_: Exception) {}
            }
        }

        val isOnline = responseCode in 200..399
        val isRangeSupported = (responseCode == 206) || (acceptRanges?.equals("bytes", ignoreCase = true) == true)

        // 3. Resolve filename
        var rawFileName = extractFileNameFromDisposition(contentDisposition)
        if (rawFileName.isNullOrBlank()) {
            rawFileName = extractFileNameFromUrl(finalUrl)
        }

        // 4. Fallback extension from Content-Type if extension missing or generic
        val cleanMime = contentType?.substringBefore(';')?.trim()?.lowercase(Locale.ROOT)
        var fileNameWithExt = appendMissingExtension(rawFileName, cleanMime)

        // 5. Sanitize filename
        var sanitized = sanitizeFileName(fileNameWithExt)

        // 6. Suggest category & subfolder
        val ext = sanitized.substringAfterLast('.', "").lowercase(Locale.ROOT)
        val category = GrabbedLink.detectCategory(ext)
        val suggestedSubfolder = when (category) {
            LinkCategory.VIDEO -> "Videos"
            LinkCategory.AUDIO -> "Music"
            LinkCategory.ARCHIVE -> "Archives"
            LinkCategory.DOCUMENT -> "Documents"
            LinkCategory.IMAGE -> "Pictures"
            LinkCategory.EXECUTABLE -> "Apps"
            else -> ""
        }

        // 7. Deduplicate " (1)" if directory exists and target file is present
        if (!targetDirectory.isNullOrBlank()) {
            val targetDirFile = File(targetDirectory)
            if (targetDirFile.exists()) {
                sanitized = deduplicateFileName(targetDirFile, sanitized)
            }
        }

        ProbeResult(
            initialUrl = trimmed,
            finalUrl = finalUrl,
            suggestedFileName = sanitized,
            suggestedCategory = category,
            suggestedSubfolder = suggestedSubfolder,
            contentLength = contentLength,
            contentType = cleanMime,
            isRangeSupported = isRangeSupported,
            isOnline = isOnline,
            responseCode = responseCode
        )
    }

    /**
     * Extracts filename from Content-Disposition header conforming to RFC 5987 / RFC 6266.
     * Supports:
     * - filename*=UTF-8''<encoded>
     * - filename="<name>"
     * - filename=<name>
     */
    fun extractFileNameFromDisposition(disposition: String?): String? {
        if (disposition.isNullOrBlank()) return null

        // 1. Check for RFC 5987 filename*=charset'lang'encoded
        val rfc5987Pattern = Pattern.compile(
            "filename\\*\\s*=\\s*([^';]+)'([^']*)'([^;]+)",
            Pattern.CASE_INSENSITIVE
        )
        val rfcMatcher = rfc5987Pattern.matcher(disposition)
        if (rfcMatcher.find()) {
            val charset = rfcMatcher.group(1)?.trim() ?: "UTF-8"
            val encodedValue = rfcMatcher.group(3)?.trim() ?: ""
            try {
                val decoded = URLDecoder.decode(encodedValue, charset.ifBlank { "UTF-8" })
                if (decoded.isNotBlank()) {
                    return decoded
                }
            } catch (e: Exception) {
                Log.w(TAG, "Failed to decode RFC5987 filename: ${e.message}")
            }
        }

        // 2. Check for standard filename="<value>" or filename=<value>
        val standardPattern = Pattern.compile(
            "filename\\s*=\\s*(\"([^\"]+)\"|([^;\\s]+))",
            Pattern.CASE_INSENSITIVE
        )
        val standardMatcher = standardPattern.matcher(disposition)
        if (standardMatcher.find()) {
            val candidate = standardMatcher.group(2) ?: standardMatcher.group(3)
            val trimmedCandidate = candidate?.trim('"', '\'', ' ')
            if (!trimmedCandidate.isNullOrBlank()) {
                return trimmedCandidate
            }
        }

        return null
    }

    /**
     * Extracts filename from URL path, stripping query parameters, URL-decoding, and taking the last segment.
     */
    fun extractFileNameFromUrl(url: String): String {
        return try {
            val uri = URI(url.trim())
            val path = uri.path ?: ""
            val clean = path.substringAfterLast('/').substringBefore('?')
            val decoded = URLDecoder.decode(clean, "UTF-8").trim()
            if (decoded.isNotBlank()) decoded else "download_${System.currentTimeMillis()}"
        } catch (e: Exception) {
            val stripped = url.substringAfterLast('/').substringBefore('?').substringBefore('#')
            try {
                URLDecoder.decode(stripped, "UTF-8").trim().ifBlank { "download_${System.currentTimeMillis()}" }
            } catch (_: Exception) {
                "download_${System.currentTimeMillis()}"
            }
        }
    }

    /**
     * If the filename lacks an extension or has a generic one (.bin/.htm), attempts to append from Content-Type.
     */
    fun appendMissingExtension(fileName: String, mimeType: String?): String {
        if (mimeType.isNullOrBlank()) return fileName

        val hasDot = fileName.contains('.')
        val ext = if (hasDot) fileName.substringAfterLast('.', "").lowercase(Locale.ROOT) else ""

        val mappedExt = MIME_TO_EXT[mimeType]
        if (mappedExt != null) {
            if (!hasDot || ext.isBlank()) {
                return "$fileName.$mappedExt"
            }
            // If existing extension is generic but mime indicates specific archive/video
            if (ext in listOf("bin", "download", "tmp", "dat") && mappedExt !in listOf("bin", "dat")) {
                return "${fileName.substringBeforeLast('.')}.$mappedExt"
            }
        }
        return fileName
    }

    /**
     * Strips illegal filesystem characters and control characters, trimming whitespace and dots.
     */
    fun sanitizeFileName(name: String): String {
        var clean = name.replace(Regex("[\\\\/:*?\"<>|\\r\\n\\t]"), "_")
        // Remove ASCII and Unicode control characters
        clean = clean.replace(Regex("[\\p{Cntrl}]"), "")
        // Collapse consecutive underscores
        clean = clean.replace(Regex("_{2,}"), "_")
        clean = clean.trim(' ', '.')
        return clean.ifBlank { "download_${System.currentTimeMillis()}" }
    }

    /**
     * Generates a deduplicated filename like "file (1).ext", "file (2).ext" if the file already exists in directory.
     */
    fun deduplicateFileName(directory: File, fileName: String): String {
        if (!File(directory, fileName).exists()) {
            return fileName
        }

        val dotIndex = fileName.lastIndexOf('.')
        val baseName = if (dotIndex > 0) fileName.substring(0, dotIndex) else fileName
        val ext = if (dotIndex > 0) fileName.substring(dotIndex) else ""

        // Check if already ends with " (X)"
        val pattern = Pattern.compile("^(.*?)\\s*\\((\\d+)\\)$")
        val matcher = pattern.matcher(baseName)
        val (coreBase, startingIndex) = if (matcher.matches()) {
            (matcher.group(1) ?: baseName) to (matcher.group(2)?.toIntOrNull() ?: 1)
        } else {
            baseName to 1
        }

        var counter = startingIndex
        var candidate = "$coreBase ($counter)$ext"
        while (File(directory, candidate).exists()) {
            counter++
            candidate = "$coreBase ($counter)$ext"
        }
        return candidate
    }
}
