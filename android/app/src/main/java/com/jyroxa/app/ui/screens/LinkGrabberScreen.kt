package com.jyroxa.app.ui.screens

import android.content.ClipboardManager
import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.ContentPaste
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.SelectAll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.jyroxa.app.data.model.LinkCategory
import com.jyroxa.app.data.repository.DownloadRepository
import com.jyroxa.app.data.repository.LinkProbeRepository
import com.jyroxa.app.data.repository.UrlProber
import com.jyroxa.app.service.DownloadService
import com.jyroxa.app.ui.components.LinkItemCard
import kotlinx.coroutines.launch
import java.io.File

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LinkGrabberScreen(
    linkProbeRepository: LinkProbeRepository,
    downloadRepository: DownloadRepository,
    initialSharedText: String? = null,
    defaultDownloadDirectory: String = "/storage/emulated/0/Download/Jyroxa",
    onBatchAdded: () -> Unit = {},
    modifier: Modifier = Modifier
) {
    val context = LocalContext.current
    val coroutineScope = rememberCoroutineScope()
    val grabbedLinks by linkProbeRepository.grabbedLinks.collectAsState()

    var inputText by remember { mutableStateOf(initialSharedText ?: "") }
    var isProbing by remember { mutableStateOf(false) }
    var selectedCategory by remember { mutableStateOf(LinkCategory.ALL) }

    val filteredLinks = remember(grabbedLinks, selectedCategory) {
        if (selectedCategory == LinkCategory.ALL) {
            grabbedLinks
        } else {
            grabbedLinks.filter { it.category == selectedCategory }
        }
    }

    val selectedCount = remember(grabbedLinks) {
        grabbedLinks.count { it.isSelected }
    }

    Scaffold(
        modifier = modifier.fillMaxSize(),
        topBar = {
            TopAppBar(
                title = {
                    Text(
                        text = "LinkGrabber",
                        style = MaterialTheme.typography.titleLarge.copy(fontWeight = FontWeight.Bold)
                    )
                },
                actions = {
                    if (grabbedLinks.isNotEmpty()) {
                        IconButton(onClick = { linkProbeRepository.clear() }) {
                            Icon(Icons.Default.Clear, contentDescription = "Clear links")
                        }
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.background
                )
            )
        },
        bottomBar = {
            if (grabbedLinks.isNotEmpty()) {
                Surface(
                    modifier = Modifier.fillMaxWidth(),
                    shadowElevation = 8.dp,
                    color = MaterialTheme.colorScheme.surface
                ) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(16.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween
                    ) {
                        Text(
                            text = "$selectedCount selected",
                            style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.SemiBold)
                        )

                        Button(
                            onClick = {
                                val selected = grabbedLinks.filter { it.isSelected }
                                val baseDirFile = File(defaultDownloadDirectory)
                                if (!baseDirFile.exists()) baseDirFile.mkdirs()

                                selected.forEach { link ->
                                    val cleanName = UrlProber.sanitizeFileName(link.fileName)
                                    val dedupedName = UrlProber.deduplicateFileName(baseDirFile, cleanName)
                                    val outPath = File(baseDirFile, dedupedName).absolutePath
                                    downloadRepository.startDownload(
                                        url = link.url,
                                        outputFilePath = outPath,
                                        connections = 8
                                    )
                                }
                                DownloadService.start(context)
                                linkProbeRepository.clear()
                                onBatchAdded()
                            },
                            enabled = selectedCount > 0,
                            shape = RoundedCornerShape(12.dp)
                        ) {
                            Icon(Icons.Default.Download, contentDescription = null)
                            Spacer(modifier = Modifier.width(8.dp))
                            Text("Download Selected ($selectedCount)")
                        }
                    }
                }
            }
        }
    ) { innerPadding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding)
        ) {
            // Text Input Box (collapsible or top section)
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                OutlinedTextField(
                    value = inputText,
                    onValueChange = { inputText = it },
                    label = { Text("Paste Links, Web Snippets, or Forum Posts") },
                    placeholder = { Text("Paste any text containing http/https links...") },
                    modifier = Modifier.fillMaxWidth(),
                    maxLines = 4,
                    trailingIcon = {
                        IconButton(onClick = {
                            val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                            val clip = clipboard.primaryClip
                            if (clip != null && clip.itemCount > 0) {
                                inputText = clip.getItemAt(0).text?.toString() ?: ""
                            }
                        }) {
                            Icon(Icons.Default.ContentPaste, contentDescription = "Paste")
                        }
                    }
                )

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.End,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    if (isProbing) {
                        CircularProgressIndicator(modifier = Modifier.size(24.dp))
                        Spacer(modifier = Modifier.width(12.dp))
                    }

                    Button(
                        onClick = {
                            if (inputText.isNotBlank()) {
                                isProbing = true
                                coroutineScope.launch {
                                    linkProbeRepository.parseAndProbeText(inputText)
                                    isProbing = false
                                }
                            }
                        },
                        enabled = inputText.isNotBlank() && !isProbing,
                        shape = RoundedCornerShape(8.dp)
                    ) {
                        Icon(Icons.Default.Search, contentDescription = null)
                        Spacer(modifier = Modifier.width(6.dp))
                        Text("Analyze & Probe")
                    }
                }
            }

            if (grabbedLinks.isNotEmpty()) {
                // Category Filter Chips
                LazyRow(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 4.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    items(LinkCategory.values()) { category ->
                        val count = if (category == LinkCategory.ALL) {
                            grabbedLinks.size
                        } else {
                            grabbedLinks.count { it.category == category }
                        }

                        if (count > 0 || category == LinkCategory.ALL) {
                            FilterChip(
                                selected = selectedCategory == category,
                                onClick = { selectedCategory = category },
                                label = { Text("${category.displayName} ($count)") }
                            )
                        }
                    }
                }

                // Selection Actions Bar
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 4.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(
                        text = "${filteredLinks.size} links detected",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )

                    Row {
                        TextButton(onClick = { linkProbeRepository.selectAll() }) {
                            Text("Select All")
                        }
                        TextButton(onClick = { linkProbeRepository.deselectAll() }) {
                            Text("Deselect All")
                        }
                    }
                }

                // Probed Links List
                LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(16.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    items(
                        items = filteredLinks,
                        key = { it.id }
                    ) { link ->
                        LinkItemCard(
                            link = link,
                            onToggleSelect = { linkProbeRepository.toggleSelection(it) }
                        )
                    }
                }
            } else if (!isProbing) {
                // Empty Prompt
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(32.dp),
                    contentAlignment = Alignment.Center
                ) {
                    Column(
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        Icon(
                            imageVector = Icons.Default.Link,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f),
                            modifier = Modifier.size(56.dp)
                        )
                        Text(
                            text = "No links grabbed yet",
                            style = MaterialTheme.typography.titleMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                        Text(
                            text = "Paste text above or share web links from any browser",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.outline
                        )
                    }
                }
            }
        }
    }
}
