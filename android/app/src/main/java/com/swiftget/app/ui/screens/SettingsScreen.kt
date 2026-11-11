package com.swiftget.app.ui.screens

import android.os.Build
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ColorLens
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Memory
import androidx.compose.material.icons.filled.NetworkCheck
import androidx.compose.material.icons.filled.Palette
import androidx.compose.material.icons.filled.Security
import androidx.compose.material.icons.filled.Wifi
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.swiftget.SwiftGet
import com.swiftget.app.SwiftGetApp
import com.swiftget.app.data.model.AppSettings
import com.swiftget.app.data.model.DarkThemeConfig

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    app: SwiftGetApp,
    modifier: Modifier = Modifier
) {
    val settings by app.settings.collectAsState()

    Scaffold(
        modifier = modifier.fillMaxSize(),
        topBar = {
            TopAppBar(
                title = {
                    Text(
                        text = "Settings",
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
            // Storage Settings Card
            SettingsSectionCard(title = "Storage & Downloads", icon = Icons.Default.Folder) {
                OutlinedTextField(
                    value = settings.downloadDirectory,
                    onValueChange = { app.updateSettings(settings.copy(downloadDirectory = it)) },
                    label = { Text("Default Download Folder") },
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true
                )

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            text = "Auto-Categorize Folders",
                            style = MaterialTheme.typography.titleMedium
                        )
                        Text(
                            text = "Save files into Videos/, Music/, Archives/ subfolders",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                    Switch(
                        checked = settings.autoCategorizeFolders,
                        onCheckedChange = { app.updateSettings(settings.copy(autoCategorizeFolders = it)) }
                    )
                }
            }

            // Engine & Multi-Connection Settings
            SettingsSectionCard(title = "Download Engine", icon = Icons.Default.Memory) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(
                        text = "Default Threads: ${settings.defaultConnections}",
                        style = MaterialTheme.typography.titleMedium
                    )
                }

                Slider(
                    value = settings.defaultConnections.toFloat(),
                    onValueChange = { app.updateSettings(settings.copy(defaultConnections = it.toInt())) },
                    valueRange = 1f..32f,
                    steps = 30
                )

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            text = "Wi-Fi Only Downloads",
                            style = MaterialTheme.typography.titleMedium
                        )
                        Text(
                            text = "Pause downloads automatically when on cellular data",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                    Switch(
                        checked = settings.wifiOnly,
                        onCheckedChange = { app.updateSettings(settings.copy(wifiOnly = it)) }
                    )
                }
            }

            // Clipboard Detection Settings
            SettingsSectionCard(title = "Clipboard & Link Capture", icon = Icons.Default.NetworkCheck) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            text = "Clipboard Detection on Resume",
                            style = MaterialTheme.typography.titleMedium
                        )
                        Text(
                            text = "Prompt to download when a URL is copied in another app",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                    Switch(
                        checked = settings.clipboardAutoCapture,
                        onCheckedChange = { app.updateSettings(settings.copy(clipboardAutoCapture = it)) }
                    )
                }
            }

            // Appearance & Dynamic Color
            SettingsSectionCard(title = "Appearance & Theme", icon = Icons.Default.Palette) {
                Text(
                    text = "Dark Theme",
                    style = MaterialTheme.typography.titleMedium
                )

                DarkThemeConfig.values().forEach { mode ->
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        RadioButton(
                            selected = settings.darkThemeConfig == mode,
                            onClick = { app.updateSettings(settings.copy(darkThemeConfig = mode)) }
                        )
                        Text(
                            text = when (mode) {
                                DarkThemeConfig.FOLLOW_SYSTEM -> "Follow System"
                                DarkThemeConfig.LIGHT -> "Always Light"
                                DarkThemeConfig.DARK -> "Always Dark"
                            },
                            style = MaterialTheme.typography.bodyLarge,
                            modifier = Modifier.padding(start = 8.dp)
                        )
                    }
                }

                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Column(modifier = Modifier.weight(1f)) {
                            Text(
                                text = "Dynamic Colors (Material You)",
                                style = MaterialTheme.typography.titleMedium
                            )
                            Text(
                                text = "Adopt wallpaper accent colors across the UI",
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant
                            )
                        }
                        Switch(
                            checked = settings.dynamicColors,
                            onCheckedChange = { app.updateSettings(settings.copy(dynamicColors = it)) }
                        )
                    }
                }
            }

            // About & Rust FFI Engine Info
            SettingsSectionCard(title = "About SwiftGet", icon = Icons.Default.Info) {
                Text(
                    text = "SwiftGet for Android v0.1.0",
                    style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold)
                )
                Text(
                    text = "High-performance cross-platform download manager with IDM-style multi-part parallel chunking and JDownloader-style LinkGrabber.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )

                val jniStatus = if (SwiftGet.isLoaded()) {
                    "Rust Core Engine (libswiftget_ffi.so): Active"
                } else {
                    "Rust Core Engine (libswiftget_ffi.so): Simulated / Dev Mode"
                }
                Text(
                    text = jniStatus,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.primary,
                    fontWeight = FontWeight.SemiBold
                )

                Text(
                    text = "License: MIT License (Open Source)",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.outline
                )
            }
        }
    }
}

@Composable
private fun SettingsSectionCard(
    title: String,
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    content: @Composable () -> Unit
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surface
        ),
        elevation = CardDefaults.cardElevation(defaultElevation = 1.dp)
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                Icon(
                    imageVector = icon,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.primary
                )
                Text(
                    text = title,
                    style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold)
                )
            }
            content()
        }
    }
}
