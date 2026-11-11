package com.swiftget.app.ui.screens

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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.DownloadDone
import androidx.compose.material.icons.filled.FilterList
import androidx.compose.material.icons.filled.PauseCircle
import androidx.compose.material.icons.filled.PlayCircle
import androidx.compose.material.icons.filled.Speed
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.swiftget.app.data.model.DownloadItem
import com.swiftget.app.data.model.DownloadState
import com.swiftget.app.data.repository.DownloadRepository
import com.swiftget.app.ui.components.DownloadCard
import com.swiftget.app.ui.theme.SpeedCyan

enum class DownloadFilterTab(val label: String) {
    ALL("All"),
    DOWNLOADING("Active"),
    COMPLETED("Completed"),
    PAUSED("Paused")
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DownloadsScreen(
    downloadRepository: DownloadRepository,
    modifier: Modifier = Modifier
) {
    val downloads by downloadRepository.downloads.collectAsState()
    val totalSpeed by downloadRepository.totalSpeedBps.collectAsState()
    val activeCount by downloadRepository.runningDownloadsCount.collectAsState()

    var selectedTab by remember { mutableStateOf(DownloadFilterTab.ALL) }

    val filteredDownloads = remember(downloads, selectedTab) {
        when (selectedTab) {
            DownloadFilterTab.ALL -> downloads
            DownloadFilterTab.DOWNLOADING -> downloads.filter { it.state.isRunning }
            DownloadFilterTab.COMPLETED -> downloads.filter { it.state == DownloadState.COMPLETED }
            DownloadFilterTab.PAUSED -> downloads.filter { it.state == DownloadState.PAUSED }
        }
    }

    Scaffold(
        modifier = modifier.fillMaxSize(),
        topBar = {
            TopAppBar(
                title = {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        Text(
                            text = "SwiftGet",
                            style = MaterialTheme.typography.titleLarge.copy(fontWeight = FontWeight.Bold)
                        )

                        if (activeCount > 0) {
                            Surface(
                                shape = MaterialTheme.shapes.small,
                                color = MaterialTheme.colorScheme.primaryContainer,
                                modifier = Modifier.padding(start = 4.dp)
                            ) {
                                Row(
                                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 2.dp),
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(4.dp)
                                ) {
                                    Icon(
                                        Icons.Default.Speed,
                                        contentDescription = null,
                                        tint = SpeedCyan,
                                        modifier = Modifier.size(16.dp)
                                    )
                                    Text(
                                        text = DownloadItem.formatBytesSpeed(totalSpeed),
                                        style = MaterialTheme.typography.labelSmall,
                                        color = MaterialTheme.colorScheme.onPrimaryContainer,
                                        fontWeight = FontWeight.Bold
                                    )
                                }
                            }
                        }
                    }
                },
                actions = {
                    if (activeCount > 0) {
                        IconButton(onClick = { downloadRepository.pauseAll() }) {
                            Icon(Icons.Default.PauseCircle, contentDescription = "Pause All")
                        }
                    } else if (downloads.any { it.state == DownloadState.PAUSED }) {
                        IconButton(onClick = { downloadRepository.resumeAll() }) {
                            Icon(Icons.Default.PlayCircle, contentDescription = "Resume All")
                        }
                    }
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
        ) {
            // Filter Tabs
            LazyRow(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 4.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                items(DownloadFilterTab.values()) { tab ->
                    val count = when (tab) {
                        DownloadFilterTab.ALL -> downloads.size
                        DownloadFilterTab.DOWNLOADING -> downloads.count { it.state.isRunning }
                        DownloadFilterTab.COMPLETED -> downloads.count { it.state == DownloadState.COMPLETED }
                        DownloadFilterTab.PAUSED -> downloads.count { it.state == DownloadState.PAUSED }
                    }

                    FilterChip(
                        selected = selectedTab == tab,
                        onClick = { selectedTab = tab },
                        label = { Text("${tab.label} ($count)") }
                    )
                }
            }

            Spacer(modifier = Modifier.height(4.dp))

            if (filteredDownloads.isEmpty()) {
                EmptyDownloadsPlaceholder(selectedTab)
            } else {
                LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp)
                ) {
                    items(
                        items = filteredDownloads,
                        key = { it.id }
                    ) { item ->
                        DownloadCard(
                            item = item,
                            onPause = { downloadRepository.pauseDownload(it) },
                            onResume = { downloadRepository.resumeDownload(it) },
                            onCancel = { downloadRepository.cancelDownload(it) },
                            onRetry = { downloadRepository.retryDownload(it) }
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun EmptyDownloadsPlaceholder(tab: DownloadFilterTab) {
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
                imageVector = Icons.Default.DownloadDone,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.outline.copy(alpha = 0.5f),
                modifier = Modifier.size(64.dp)
            )
            Text(
                text = "No ${tab.label.lowercase()} downloads",
                style = MaterialTheme.typography.titleMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
            Text(
                text = "Add a URL or share links into SwiftGet",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.outline
            )
        }
    }
}
