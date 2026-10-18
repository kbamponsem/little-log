package app.littlelog.widget

import android.content.Context
import androidx.work.BackoffPolicy
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import java.util.TimeZone
import java.util.concurrent.TimeUnit

/**
 * Drains queued widget taps without the app being open.
 *
 * WorkManager runs this in the app's process (no activity, no WebView): the JNI
 * call opens SQLite, applies the taps and rewrites the widget state file, then
 * Glance is asked to re-render.
 */
class WidgetDrainWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val filesDir = applicationContext.filesDir.absolutePath
        return try {
            WidgetBridge.nativeDrain(filesDir, tzOffsetMinutes())
            WidgetUpdater.update(applicationContext)
            Result.success()
        } catch (error: Throwable) {
            // The queue is only cleared once the entries are in the database, so
            // retrying can never double-log; ids make a repeat harmless anyway.
            WidgetUpdater.update(applicationContext)
            Result.retry()
        }
    }

    companion object {
        private const val WORK_NAME = "little-log-widget-drain"

        /**
         * Ask for a drain. KEEP means taps arriving while one is running are
         * picked up by that same run (it reads the whole queue).
         */
        fun schedule(context: Context) {
            val request = OneTimeWorkRequestBuilder<WidgetDrainWorker>()
                .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 10, TimeUnit.SECONDS)
                .build()
            WorkManager.getInstance(context)
                .enqueueUniqueWork(WORK_NAME, ExistingWorkPolicy.KEEP, request)
        }

        /** Minutes the device is ahead of UTC, so "today" is the parent's today. */
        fun tzOffsetMinutes(): Int =
            TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 60_000
    }
}
