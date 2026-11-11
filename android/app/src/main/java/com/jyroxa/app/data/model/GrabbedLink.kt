package com.jyroxa.app.data.model

import kotlinx.serialization.Serializable
import java.net.URI
import java.util.Locale

@Serializable
enum class LinkCategory(val displayName: String) {
    ALL("All"),
    VIDEO("Video"),
    AUDIO("Audio"),
    ARCHIVE("Archives"),
    DOCUMENT("Documents"),
    IMAGE("Images"),
    EXECUTABLE("Apps / Executables"),
    OTHER("Other")
}

@Serializable
data class GrabbedLink(
    val id: String,
    val url: String,
    val fileName: String,
    val fileExtension: String,
    val category: LinkCategory,
    val fileSizeBytes: Long? = null,
    val mimeType: String? = null,
    val isProbed: Boolean = false,
    val isOnline: Boolean = true,
    val isSelected: Boolean = true
) {
    val formattedSize: String
        get() = when {
            fileSizeBytes == null -> if (isProbed) "Unknown" else "Probing..."
            fileSizeBytes <= 0 -> "0 B"
            else -> DownloadItem.formatBytes(fileSizeBytes)
        }

    companion object {
        fun fromUrl(url: String): GrabbedLink {
            val trimmed = url.trim()
            val cleanPath = try {
                URI(trimmed).path ?: ""
            } catch (e: Exception) {
                trimmed
            }
            val rawName = cleanPath.substringAfterLast('/').substringBefore('?')
            val name = if (rawName.isBlank()) "download_file" else rawName
            val ext = name.substringAfterLast('.', "").lowercase(Locale.ROOT)
            val category = detectCategory(ext)
            val id = java.util.UUID.randomUUID().toString()

            return GrabbedLink(
                id = id,
                url = trimmed,
                fileName = name,
                fileExtension = ext,
                category = category,
                fileSizeBytes = null,
                mimeType = null,
                isProbed = false,
                isOnline = true,
                isSelected = true
            )
        }

        fun detectCategory(extension: String): LinkCategory {
            return when (extension.lowercase(Locale.ROOT)) {
                "mp4", "mkv", "webm", "avi", "mov", "flv", "wmv", "m4v", "3gp", "ts" -> LinkCategory.VIDEO
                "mp3", "flac", "wav", "aac", "ogg", "m4a", "opus", "wma" -> LinkCategory.AUDIO
                "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso", "tgz", "zst" -> LinkCategory.ARCHIVE
                "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "epub", "csv" -> LinkCategory.DOCUMENT
                "jpg", "jpeg", "png", "gif", "webp", "svg", "bmp", "ico", "avif" -> LinkCategory.IMAGE
                "apk", "exe", "msi", "dmg", "deb", "rpm", "bin", "sh" -> LinkCategory.EXECUTABLE
                else -> LinkCategory.OTHER
            }
        }
    }
}
