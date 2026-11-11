package com.swiftget.app

import android.app.Application
import com.swiftget.app.data.model.AppSettings
import com.swiftget.app.data.repository.DownloadRepository
import com.swiftget.app.data.repository.LinkProbeRepository
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

class SwiftGetApp : Application() {

    val appScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    lateinit var downloadRepository: DownloadRepository
        private set

    lateinit var linkProbeRepository: LinkProbeRepository
        private set

    private val _settings = MutableStateFlow(AppSettings())
    val settings: StateFlow<AppSettings> = _settings.asStateFlow()

    override fun onCreate() {
        super.onCreate()
        instance = this

        downloadRepository = DownloadRepository(appScope)
        linkProbeRepository = LinkProbeRepository()

        // Set default download folder to standard Download/SwiftGet
        val defaultDir = getExternalFilesDir(null)?.resolve("SwiftGet")?.absolutePath
            ?: "/storage/emulated/0/Download/SwiftGet"
        _settings.value = _settings.value.copy(downloadDirectory = defaultDir)
    }

    fun updateSettings(newSettings: AppSettings) {
        _settings.value = newSettings
    }

    companion object {
        lateinit var instance: SwiftGetApp
            private set
    }
}
