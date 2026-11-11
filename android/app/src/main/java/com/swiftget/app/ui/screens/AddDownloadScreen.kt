package com.swiftget.app.ui.screens

import android.content.ClipboardManager
import android.content.Context
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
import androidx.compose.material.icons.filled.ContentPaste
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.Speed
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.swiftget.app.data.repository.DownloadRepository
import com.swiftget.app.service.DownloadService
import java.io.File
import java.net.URI

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AddDownloadScreen(
    downloadRepository: DownloadRepository,
    initialUrl: String? = null,
    defaultDirectory: String = "/storage/emulated/0/Download/SwiftGet",
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

    // Derive filename automatically from URL changes
    LaunchedEffect(url) {
        if (url.isNotBlank()) {
            val derived = try {
                val cleanPath = URI(url.trim()).path ?: ""
                val raw = cleanPath.substringAfterLast('/').substringBefore('?')
                if (raw.isNotBlank()) raw else "download_${System.currentTimeMillis()}"
            } catch (e: Exception) {
                "download_${System.currentTimeMillis()}"
            }
            if (fileName.isBlank() || fileName.startsWith("download_")) {
                fileName = derived
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
            // URL Input Field with Paste Icon
            OutlinedTextField(
                value = url,
                onValueChange = { url = it },
                label = { Text("Download URL (HTTP / HTTPS)") },
                placeholder = { Text("https://example.com/file.zip") },
                modifier = Modifier.fillMaxWidth(),
                singleLine = false,
                maxLines = 3,
                trailingIcon = {
                    IconButton(onClick = {
                        val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                        val clip = clipboard.primaryClip
                        if (clip != null && clip.itemCount > 0) {
                            val text = clip.getItemAt(0).text?.toString() ?: ""
                            if (text.startsWith("http://") || text.startsWith("https://")) {
                                url = text
                            }
                        }
                    }) {
                        Icon(Icons.Default.ContentPaste, contentDescription = "Paste from clipboard")
                    }
                }
            )

            // Filename Input Field
            OutlinedTextField(
                value = fileName,
                onValueChange = { fileName = it },
                label = { Text("Save As File Name") },
                placeholder = { Text("file.zip") },
                modifier = Modifier.fillMaxWidth(),
                singleLine = true
            )

            // Save Location Field
            OutlinedTextField(
                value = outputDir,
                onValueChange = { outputDir = it },
                label = { Text("Download Folder") },
                modifier = Modifier.fillMaxWidth(),
                singleLine = true,
                leadingIcon = {
                    Icon(Icons.Default.Folder, contentDescription = null)
                }
            )

            // Parallel Connections Card (Segmented multi-thread configuration)
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
                        text = "Splits the file into multiple parallel segments to saturate connection speed, IDM/ADM style.",
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
                        val fullPath = File(outputDir, fileName.ifBlank { "download_${System.currentTimeMillis()}" }).absolutePath
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
