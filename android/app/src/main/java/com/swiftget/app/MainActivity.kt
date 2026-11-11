package com.swiftget.app

import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import com.swiftget.app.data.model.DarkThemeConfig
import com.swiftget.app.ui.screens.MainNavigation
import com.swiftget.app.ui.theme.SwiftGetTheme

class MainActivity : ComponentActivity() {

    private var sharedContent by mutableStateOf<String?>(null)
    private var clipboardDetectedUrl by mutableStateOf<String?>(null)
    private var lastDismissedClipboardUrl: String? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)

        handleIncomingIntent(intent)

        val app = application as SwiftGetApp

        setContent {
            val settings by app.settings.collectAsState()

            val isDark = when (settings.darkThemeConfig) {
                DarkThemeConfig.FOLLOW_SYSTEM -> isSystemInDarkTheme()
                DarkThemeConfig.LIGHT -> false
                DarkThemeConfig.DARK -> true
            }

            SwiftGetTheme(
                darkTheme = isDark,
                dynamicColor = settings.dynamicColors
            ) {
                Surface(modifier = Modifier.fillMaxSize()) {
                    MainNavigation(
                        app = app,
                        clipboardDetectedUrl = clipboardDetectedUrl,
                        sharedText = sharedContent,
                        onDismissClipboard = {
                            lastDismissedClipboardUrl = clipboardDetectedUrl
                            clipboardDetectedUrl = null
                        }
                    )
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleIncomingIntent(intent)
    }

    override fun onResume() {
        super.onResume()
        checkClipboard()
    }

    private fun handleIncomingIntent(intent: Intent?) {
        if (intent == null) return

        when (intent.action) {
            Intent.ACTION_SEND -> {
                if (intent.type == "text/plain") {
                    val text = intent.getStringExtra(Intent.EXTRA_TEXT)
                    if (!text.isNullOrBlank()) {
                        sharedContent = text
                    }
                }
            }
            Intent.ACTION_VIEW -> {
                val data: Uri? = intent.data
                if (data != null) {
                    val url = data.toString()
                    if (url.startsWith("http://", ignoreCase = true) ||
                        url.startsWith("https://", ignoreCase = true)
                    ) {
                        sharedContent = url
                    }
                }
            }
        }
    }

    private fun checkClipboard() {
        val app = application as SwiftGetApp
        if (!app.settings.value.clipboardAutoCapture) return

        val clipboard = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return
        val clip = clipboard.primaryClip
        if (clip != null && clip.itemCount > 0) {
            val text = clip.getItemAt(0).text?.toString()?.trim() ?: return
            if ((text.startsWith("http://", ignoreCase = true) ||
                text.startsWith("https://", ignoreCase = true)) &&
                text != lastDismissedClipboardUrl &&
                text != clipboardDetectedUrl
            ) {
                clipboardDetectedUrl = text
            }
        }
    }
}
