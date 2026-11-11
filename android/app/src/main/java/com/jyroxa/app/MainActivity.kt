package com.jyroxa.app

import android.Manifest
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.core.content.ContextCompat
import com.jyroxa.app.data.model.DarkThemeConfig
import com.jyroxa.app.ui.screens.MainNavigation
import com.jyroxa.app.ui.theme.JyroxaTheme

class MainActivity : ComponentActivity() {

    private var sharedContent by mutableStateOf<String?>(null)
    private var clipboardDetectedUrl by mutableStateOf<String?>(null)
    private var lastDismissedClipboardUrl: String? = null

    private val notificationPermissionLauncher =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { _ ->
            // Handled
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)

        // Request POST_NOTIFICATIONS permission on Android 13+ (API 33+)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            if (ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
                notificationPermissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
            }
        }

        handleIncomingIntent(intent)

        val app = application as JyroxaApp

        setContent {
            val settings by app.settings.collectAsState()

            val isDark = when (settings.darkThemeConfig) {
                DarkThemeConfig.FOLLOW_SYSTEM -> isSystemInDarkTheme()
                DarkThemeConfig.LIGHT -> false
                DarkThemeConfig.DARK -> true
            }

            JyroxaTheme(
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
        val app = application as JyroxaApp
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
