package com.jyroxa.app.ui.screens

import android.content.ClipboardManager
import android.content.Context
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.ContentPaste
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Speed
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.SuggestionChip
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.jyroxa.app.R
import com.jyroxa.app.data.model.DownloadItem
import com.jyroxa.app.data.repository.DownloadRepository
import com.jyroxa.app.data.repository.ProbeResult
import com.jyroxa.app.data.repository.UrlProber
import com.jyroxa.app.service.DownloadService
import com.jyroxa.app.ui.theme.SpeedAmber
import com.jyroxa.app.ui.theme.SpeedGreen
import kotlinx.coroutines.delay
import java.io.File

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AddDownloadScreen(
    downloadRepository: DownloadRepository,
    initialUrl: String? = null,
    defaultDirectory: String = "/storage/emulated/0/Download/Jyroxa",
    onDownloadStarted: () -> Unit = {},
    modifier: Modifier = Modifier
) {
    val context = LocalContext.current
    var url by remember { mutableStateOf(initialUrl ?: "") }
    var fileName by remember { mutableStateOf("") }
    var outputDir by remember { mutableStateOf(defaultDirectory) }
    var connections by remember { mutableFloatStateOf(8f) }
    var speedLimitKb by remember { mutableStateOf("") }
    var sha256Checksum by remember { mutableStateOf("") }

    var isProbing by remember { mutableStateOf(false) }
    var probeResult by remember { mutableStateOf<ProbeResult?>(null) }
    var hasUserEditedFileName by remember { mutableStateOf(false) }
    var hasUserEditedFolder by remember { mutableStateOf(false) }

    // Debounce URL input by 500ms, then probe HEAD -> GET Range 0-0
    LaunchedEffect(url) {
        val trimmed = url.trim()
        if (trimmed.startsWith("http://", ignoreCase = true) ||
            trimmed.startsWith("https://", ignoreCase = true)
        ) {
            delay(500) // Debounce
            isProbing = true
            try {
                val result = UrlProber.probe(trimmed, outputDir)
                probeResult = result

                // Auto-fill filename from RFC 5987 / Content-Disposition / URL heuristics if user hasn't edited
                if (!hasUserEditedFileName) {
                    fileName = result.suggestedFileName
                }

                // Category folder suggestion if user hasn't manually edited folder
                if (!hasUserEditedFolder && result.suggestedSubfolder.isNotEmpty()) {
                    val suggestedDir = File(defaultDirectory, result.suggestedSubfolder).absolutePath
                    outputDir = suggestedDir
                }
            } catch (_: Exception) {
                if (!hasUserEditedFileName && fileName.isBlank()) {
                    fileName = UrlProber.extractFileNameFromUrl(trimmed)
                }
            } finally {
                isProbing = false
            }
        }
    }

    Scaffold(
        modifier = modifier.fillMaxSize(),
        topBar = {
            TopAppBar(
                title = {
                    Text(
                        text = "Add Download",
                        style = MaterialTheme.typography.titleLarge.copy(fontWeight = FontWeight.Bold)
                    )
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.background
                )
            )
        }
    ) { innerPadding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding)
                .verticalScroll(rememberScrollState())
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            // URL Input Field with Paste & Probing Indicator
            OutlinedTextField(
                value = url,
                onValueChange = {
                    url = it
                    probeResult = null
                },
                label = { Text("Download URL (HTTP / HTTPS)") },
                placeholder = { Text("https://example.com/file.zip") },
                modifier = Modifier.fillMaxWidth(),
                singleLine = false,
                maxLines = 3,
                trailingIcon = {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        if (isProbing) {
                            CircularProgressIndicator(
                                modifier = Modifier
                                    .size(20.dp)
                                    .padding(end = 4.dp),
                                strokeWidth = 2.dp
                            )
                        }
                        IconButton(onClick = {
                            val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                            val clip = clipboard.primaryClip
                            if (clip != null && clip.itemCount > 0) {
                                val text = clip.getItemAt(0).text?.toString()?.trim() ?: ""
                                if (text.startsWith("http://", ignoreCase = true) ||
                                    text.startsWith("https://", ignoreCase = true)
                                ) {
                                    url = text
                                }
                            }
                        }) {
                            Icon(Icons.Default.ContentPaste, contentDescription = "Paste from clipboard")
                        }
                    }
                }
            )

            // Probing Summary Badge
            AnimatedVisibility(visible = probeResult != null || isProbing) {
                if (isProbing) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        modifier = Modifier.padding(horizontal = 4.dp)
                    ) {
                        Text(
                            text = stringResource(R.string.probe_status_probing),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.primary
                        )
                    }
                } else if (probeResult != null) {
                    val res = probeResult!!
                    Card(
                        modifier = Modifier.fillMaxWidth(),
                        colors = CardDefaults.cardColors(
                            containerColor = MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.4f)
                        ),
                        shape = RoundedCornerShape(12.dp)
                    ) {
                        Column(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(12.dp),
                            verticalArrangement = Arrangement.spacedBy(6.dp)
                        ) {
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.SpaceBetween,
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(6.dp)
                                ) {
                                    Icon(
                                        Icons.Default.CheckCircle,
                                        contentDescription = null,
                                        tint = SpeedGreen,
                                        modifier = Modifier.size(16.dp)
                                    )
                                    Text(
                                        text = if (res.contentLength != null && res.contentLength > 0L) {
                                            "Size: ${DownloadItem.formatBytes(res.contentLength)}"
                                        } else "Size: Unknown (chunked)",
                                        style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Bold)
                                    )
                                }

                                Text(
                                    text = if (res.isRangeSupported) {
                                        stringResource(R.string.probe_range_supported)
                                    } else {
                                        stringResource(R.string.probe_range_not_supported)
                                    },
                                    style = MaterialTheme.typography.labelSmall,
                                    color = if (res.isRangeSupported) SpeedGreen else SpeedAmber,
                                    fontWeight = FontWeight.SemiBold
                                )
                            }

                            if (res.contentType != null) {
                                Text(
                                    text = "Type: ${res.contentType} • Category: ${res.suggestedCategory.displayName}",
                                    style = MaterialTheme.typography.labelSmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant
                                )
                            }

                            if (res.suggestedSubfolder.isNotEmpty()) {
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(8.dp)
                                ) {
                                    Text(
                                        text = "Suggested folder: ${res.suggestedSubfolder}/",
                                        style = MaterialTheme.typography.labelSmall,
                                        color = MaterialTheme.colorScheme.primary
                                    )
                                    SuggestionChip(
                                        onClick = {
                                            outputDir = File(defaultDirectory, res.suggestedSubfolder).absolutePath
                                            hasUserEditedFolder = true
                                        },
                                        label = { Text("Apply") },
                                        modifier = Modifier.height(28.dp)
                                    )
                                }
                            }
                        }
                    }
                }
            }

            // Filename Input Field
            OutlinedTextField(
                value = fileName,
                onValueChange = {
                    fileName = it
                    hasUserEditedFileName = true
                },
                label = { Text("Save As File Name") },
                placeholder = { Text("archive.zip") },
                modifier = Modifier.fillMaxWidth(),
                singleLine = true,
                supportingText = {
                    if (hasUserEditedFileName) {
                        Text("Custom user filename (auto-fill will not overwrite)")
                    }
                }
            )

            // Save Location Field
            OutlinedTextField(
                value = outputDir,
                onValueChange = {
                    outputDir = it
                    hasUserEditedFolder = true
                },
                label = { Text("Download Folder") },
                modifier = Modifier.fillMaxWidth(),
                singleLine = true,
                leadingIcon = {
                    Icon(Icons.Default.Folder, contentDescription = null)
                }
            )

            // Parallel Connections Card
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(
                    containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f)
                ),
                shape = RoundedCornerShape(12.dp)
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text(
                            text = "Parallel Connections",
                            style = MaterialTheme.typography.titleMedium
                        )
                        Text(
                            text = "${connections.toInt()} threads",
                            style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold),
                            color = MaterialTheme.colorScheme.primary
                        )
                    }

                    Text(
                        text = "Splits the transfer into multi-part parallel segments to saturate connection throughput.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )

                    Slider(
                        value = connections,
                        onValueChange = { connections = it },
                        valueRange = 1f..32f,
                        steps = 30
                    )
                }
            }

            // Optional Speed Limit & Checksums Card
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(
                    containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f)
                ),
                shape = RoundedCornerShape(12.dp)
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp)
                ) {
                    Text(
                        text = "Advanced Options (Optional)",
                        style = MaterialTheme.typography.titleMedium
                    )

                    OutlinedTextField(
                        value = speedLimitKb,
                        onValueChange = { speedLimitKb = it },
                        label = { Text("Speed Limit (KB/s, leave empty for max)") },
                        modifier = Modifier.fillMaxWidth(),
                        singleLine = true,
                        leadingIcon = {
                            Icon(Icons.Default.Speed, contentDescription = null)
                        }
                    )

                    OutlinedTextField(
                        value = sha256Checksum,
                        onValueChange = { sha256Checksum = it },
                        label = { Text("SHA-256 / MD5 Checksum Verification") },
                        placeholder = { Text("e.g. 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08") },
                        modifier = Modifier.fillMaxWidth(),
                        singleLine = true
                    )
                }
            }

            Spacer(modifier = Modifier.height(8.dp))

            // Start Download Button
            Button(
                onClick = {
                    if (url.isNotBlank()) {
                        val finalCleanName = UrlProber.sanitizeFileName(
                            fileName.ifBlank { UrlProber.extractFileNameFromUrl(url) }
                        )
                        val targetDirFile = File(outputDir)
                        if (!targetDirFile.exists()) {
                            targetDirFile.mkdirs()
                        }
                        val dedupedName = UrlProber.deduplicateFileName(targetDirFile, finalCleanName)
                        val fullPath = File(targetDirFile, dedupedName).absolutePath

                        downloadRepository.startDownload(
                            url = url.trim(),
                            outputFilePath = fullPath,
                            connections = connections.toInt()
                        )
                        DownloadService.start(context)
                        onDownloadStarted()
                    }
                },
                enabled = url.isNotBlank(),
                modifier = Modifier
                    .fillMaxWidth()
                    .height(52.dp),
                shape = RoundedCornerShape(12.dp)
            ) {
                Icon(Icons.Default.Download, contentDescription = null)
                Spacer(modifier = Modifier.width(8.dp))
                Text(
                    text = "Start Download",
                    style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold)
                )
            }
        }
    }
}
