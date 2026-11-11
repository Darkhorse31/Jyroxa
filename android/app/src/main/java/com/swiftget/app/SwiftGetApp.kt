package com.swiftget.app

import android.app.Application
import android.os.Environment
import android.util.Log
import com.swiftget.app.data.model.AppSettings
import com.swiftget.app.data.repository.DownloadRepository
import com.swiftget.app.data.repository.LinkProbeRepository
import com.swiftget.app.util.NetworkMonitor
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.io.File

class SwiftGetApp : Application() {

    val appScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    lateinit var networkMonitor: NetworkMonitor
        private set

    lateinit var downloadRepository: DownloadRepository
        private set

    lateinit var linkProbeRepository: LinkProbeRepository
        private set

    private val json = Json { ignoreUnknownKeys = true }
    private val _settings = MutableStateFlow(AppSettings())
    val settings: StateFlow<AppSettings> = _settings.asStateFlow()

    private val settingsFile: File by lazy { File(filesDir, "settings.json") }
    private val cacheFile: File by lazy { File(filesDir, "downloads_cache.json") }

    override fun onCreate() {
        super.onCreate()
        instance = this

        networkMonitor = NetworkMonitor(this)

        loadSettings()

        downloadRepository = DownloadRepository(
            scope = appScope,
            storageFile = cacheFile,
            networkMonitor = networkMonitor
        )
        downloadRepository.setWifiOnlyEnforced(_settings.value.wifiOnly)

        linkProbeRepository = LinkProbeRepository()
    }

    private fun loadSettings() {
        var loaded: AppSettings? = null
        if (settingsFile.exists()) {
            try {
                val content = settingsFile.readText()
                if (content.isNotBlank()) {
                    loaded = json.decodeFromString<AppSettings>(content)
                }
            } catch (e: Exception) {
                Log.w("SwiftGetApp", "Failed reading settings file: ${e.message}")
            }
        }

        val baseDownloadDir = try {
            val publicDir = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS)
            val appSub = File(publicDir, getString(R.string.app_name))
            if (!appSub.exists()) appSub.mkdirs()
            appSub.absolutePath
        } catch (_: Exception) {
            getExternalFilesDir(null)?.resolve(getString(R.string.app_name))?.absolutePath
                ?: "/storage/emulated/0/Download/${getString(R.string.app_name)}"
        }

        _settings.value = (loaded ?: AppSettings()).copy(
            downloadDirectory = loaded?.downloadDirectory?.ifBlank { baseDownloadDir } ?: baseDownloadDir
        )
    }

    fun updateSettings(newSettings: AppSettings) {
        _settings.value = newSettings
        downloadRepository.setWifiOnlyEnforced(newSettings.wifiOnly)
        appScope.launch(Dispatchers.IO) {
            try {
                settingsFile.writeText(json.encodeToString(newSettings))
            } catch (e: Exception) {
                Log.w("SwiftGetApp", "Failed saving settings: ${e.message}")
            }
        }
    }

    companion object {
        lateinit var instance: SwiftGetApp
            private set
    }
}
