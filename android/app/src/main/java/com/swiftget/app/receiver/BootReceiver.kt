package com.swiftget.app.receiver

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import com.swiftget.app.SwiftGetApp
import com.swiftget.app.worker.WorkManagerHelper

/**
 * Listens for system boot completion to schedule WorkManager fallback recovery
 * of interrupted downloads.
 */
class BootReceiver : BroadcastReceiver() {

    companion object {
        private const val TAG = "BootReceiver"
    }

    override fun onReceive(context: Context, intent: Intent?) {
        val action = intent?.action ?: return
        Log.i(TAG, "Received broadcast action: $action")

        if (action == Intent.ACTION_BOOT_COMPLETED ||
            action == Intent.ACTION_MY_PACKAGE_REPLACED ||
            action == "android.intent.action.QUICKBOOT_POWERON"
        ) {
            val app = context.applicationContext as? SwiftGetApp
            val wifiOnly = app?.settings?.value?.wifiOnly ?: false
            WorkManagerHelper.enqueueResumeWork(context, wifiOnly)
        }
    }
}
