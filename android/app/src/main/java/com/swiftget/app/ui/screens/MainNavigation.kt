package com.swiftget.app.ui.screens

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.AddCircle
import androidx.compose.material.icons.filled.CloudDownload
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.swiftget.app.SwiftGetApp

enum class ScreenTab(val title: String, val icon: androidx.compose.ui.graphics.vector.ImageVector) {
    DOWNLOADS("Downloads", Icons.Default.CloudDownload),
    ADD("Add", Icons.Default.AddCircle),
    LINK_GRABBER("LinkGrabber", Icons.Default.Link),
    SETTINGS("Settings", Icons.Default.Settings)
}

@Composable
fun MainNavigation(
    app: SwiftGetApp,
    clipboardDetectedUrl: String?,
    sharedText: String?,
    onDismissClipboard: () -> Unit,
    modifier: Modifier = Modifier
) {
    var currentTab by remember {
        mutableStateOf(
            when {
                sharedText != null && sharedText.contains("\n") -> ScreenTab.LINK_GRABBER
                sharedText != null -> ScreenTab.ADD
                else -> ScreenTab.DOWNLOADS
            }
        )
    }

    var passedUrlToAdd by remember { mutableStateOf<String?>(null) }
    var passedTextToGrabber by remember { mutableStateOf<String?>(sharedText) }

    Scaffold(
        modifier = modifier.fillMaxSize(),
        bottomBar = {
            NavigationBar {
                ScreenTab.values().forEach { tab ->
                    NavigationBarItem(
                        selected = currentTab == tab,
                        onClick = { currentTab = tab },
                        icon = { Icon(tab.icon, contentDescription = tab.title) },
                        label = { Text(tab.title) }
                    )
                }
            }
        }
    ) { innerPadding ->
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding)
        ) {
            when (currentTab) {
                ScreenTab.DOWNLOADS -> {
                    DownloadsScreen(
                        downloadRepository = app.downloadRepository
                    )
                }
                ScreenTab.ADD -> {
                    AddDownloadScreen(
                        downloadRepository = app.downloadRepository,
                        initialUrl = passedUrlToAdd ?: (if (sharedText != null && !sharedText.contains("\n")) sharedText else null),
                        onDownloadStarted = {
                            passedUrlToAdd = null
                            currentTab = ScreenTab.DOWNLOADS
                        }
                    )
                }
                ScreenTab.LINK_GRABBER -> {
                    LinkGrabberScreen(
                        linkProbeRepository = app.linkProbeRepository,
                        downloadRepository = app.downloadRepository,
                        initialSharedText = passedTextToGrabber,
                        onBatchAdded = {
                            passedTextToGrabber = null
                            currentTab = ScreenTab.DOWNLOADS
                        }
                    )
                }
                ScreenTab.SETTINGS -> {
                    SettingsScreen(app = app)
                }
            }

            // Clipboard detected banner at top
            AnimatedVisibility(
                visible = clipboardDetectedUrl != null,
                enter = slideInVertically(initialOffsetY = { -it }),
                exit = slideOutVertically(targetOffsetY = { -it }),
                modifier = Modifier.align(Alignment.TopCenter)
            ) {
                if (clipboardDetectedUrl != null) {
                    Surface(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(12.dp),
                        shape = RoundedCornerShape(12.dp),
                        color = MaterialTheme.colorScheme.primaryContainer,
                        shadowElevation = 6.dp
                    ) {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = 16.dp, vertical = 10.dp),
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Column(modifier = Modifier.weight(1f)) {
                                Text(
                                    text = "Copied link detected",
                                    style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold),
                                    color = MaterialTheme.colorScheme.onPrimaryContainer
                                )
                                Text(
                                    text = clipboardDetectedUrl,
                                    style = MaterialTheme.typography.bodyMedium,
                                    color = MaterialTheme.colorScheme.onPrimaryContainer.copy(alpha = 0.8f),
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis
                                )
                            }

                            Row {
                                TextButton(onClick = onDismissClipboard) {
                                    Text("Dismiss")
                                }
                                Button(
                                    onClick = {
                                        passedUrlToAdd = clipboardDetectedUrl
                                        currentTab = ScreenTab.ADD
                                        onDismissClipboard()
                                    }
                                ) {
                                    Text("Add")
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
