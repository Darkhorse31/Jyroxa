package com.swiftget.app.data.model

import kotlinx.serialization.Serializable

@Serializable
enum class DarkThemeConfig {
    FOLLOW_SYSTEM,
    LIGHT,
    DARK
}

@Serializable
data class AppSettings(
    val downloadDirectory: String = "/storage/emulated/0/Download/SwiftGet",
    val defaultConnections: Int = 8,
    val speedLimitKbps: Int = 0, // 0 = unlimited
    val wifiOnly: Boolean = false,
    val clipboardAutoCapture: Boolean = true,
    val darkThemeConfig: DarkThemeConfig = DarkThemeConfig.FOLLOW_SYSTEM,
    val dynamicColors: Boolean = true,
    val autoCategorizeFolders: Boolean = true
)
