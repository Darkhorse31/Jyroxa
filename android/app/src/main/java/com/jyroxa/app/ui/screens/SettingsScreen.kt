package com.jyroxa.app.ui.screens

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.material.icons.filled.BatteryAlert
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Memory
import androidx.compose.material.icons.filled.NetworkCheck
import androidx.compose.material.icons.filled.Notifications
import androidx.compose.material.icons.filled.Palette
import androidx.compose.material.icons.filled.Wifi
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import com.jyroxa.app.JyroxaFfi
import com.jyroxa.app.R
import com.jyroxa.app.JyroxaApp
import com.jyroxa.app.data.model.AppSettings
import com.jyroxa.app.data.model.DarkThemeConfig
import com.jyroxa.app.ui.theme.SpeedAmber
import com.jyroxa.app.ui.theme.SpeedGreen
import com.jyroxa.app.util.BatteryOptimizationHelper

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    app: JyroxaApp,
    modifier: Modifier = Modifier
) {
    val context = LocalContext.current
    val settings by app.settings.collectAsState()

    var isBatteryExempt by remember {
        mutableStateOf(BatteryOptimizationHelper.isIgnoringBatteryOptimizations(context))
    }

    var areNotificationsEnabled by remember {
        mutableStateOf(NotificationManagerCompat.from(context).areNotificationsEnabled())
    }

    val notificationLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.RequestPermission()
    ) { granted ->
        areNotificationsEnabled = granted || NotificationManagerCompat.from(context).areNotificationsEnabled()
    }

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
                            text = "Save files automatically into Videos/, Music/, Archives/ subfolders",
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
            SettingsSectionCard(title = "Download Engine & Network", icon = Icons.Default.Memory) {
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
                            text = stringResource(R.string.settings_wifi_only_title),
                            style = MaterialTheme.typography.titleMedium
                        )
                        Text(
                            text = stringResource(R.string.settings_wifi_only_desc),
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

            // Background & Battery Optimization
            SettingsSectionCard(title = "Background Execution & Battery", icon = Icons.Default.BatteryAlert) {
                Text(
                    text = stringResource(R.string.settings_battery_opt_desc),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    if (isBatteryExempt) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(6.dp)
                        ) {
                            Icon(
                                Icons.Default.CheckCircle,
                                contentDescription = null,
                                tint = SpeedGreen,
                                modifier = Modifier.size(20.dp)
                            )
                            Text(
                                text = stringResource(R.string.settings_battery_opt_exempt),
                                style = MaterialTheme.typography.bodyMedium.copy(fontWeight = FontWeight.SemiBold),
                                color = SpeedGreen
                            )
                        }
                    } else {
                        Button(
                            onClick = {
                                try {
                                    context.startActivity(BatteryOptimizationHelper.createRequestIntent(context))
                                } catch (_: Exception) {
                                    context.startActivity(BatteryOptimizationHelper.createSettingsIntent())
                                }
                                isBatteryExempt = BatteryOptimizationHelper.isIgnoringBatteryOptimizations(context)
                            },
                            shape = RoundedCornerShape(8.dp)
                        ) {
                            Text(stringResource(R.string.settings_battery_opt_button))
                        }
                    }
                }
            }

            // Notification Permissions (Android 13+)
            SettingsSectionCard(title = "Notifications & Live Tray", icon = Icons.Default.Notifications) {
                Text(
                    text = stringResource(R.string.settings_notifications_desc),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    if (areNotificationsEnabled) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(6.dp)
                        ) {
                            Icon(
                                Icons.Default.CheckCircle,
                                contentDescription = null,
                                tint = SpeedGreen,
                                modifier = Modifier.size(20.dp)
                            )
                            Text(
                                text = stringResource(R.string.settings_notifications_granted),
                                style = MaterialTheme.typography.bodyMedium.copy(fontWeight = FontWeight.SemiBold),
                                color = SpeedGreen
                            )
                        }
                    } else {
                        Button(
                            onClick = {
                                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                                    notificationLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
                                } else {
                                    val intent = Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).apply {
                                        putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
                                    }
                                    context.startActivity(intent)
                                }
                            },
                            shape = RoundedCornerShape(8.dp)
                        ) {
                            Text(stringResource(R.string.settings_notifications_button))
                        }
                    }
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

            // About & Rust Engine
            val appName = stringResource(R.string.app_name)
            SettingsSectionCard(title = "About $appName", icon = Icons.Default.Info) {
                Text(
                    text = "$appName for Android v0.1.1",
                    style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold)
                )
                Text(
                    text = "High-performance cross-platform download manager with multi-part parallel chunking and LinkGrabber.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )

                val jniStatus = if (JyroxaFfi.isLoaded()) {
                    "Rust Core Engine (libjyroxa_ffi.so): Active"
                } else {
                    "Rust Core Engine (libjyroxa_ffi.so): Simulated / Dev Mode"
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
