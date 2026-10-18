package app.littlelog.widget

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.security.SecureRandom

/**
 * The one-tap queue.
 *
 * A widget tap can't open the app, so [LogActionReceiver] appends the action
 * here and a WorkManager worker drains it through [WidgetBridge]. The Rust core
 * owns the file's schema (`crates/core/src/pending.rs`):
 *
 * ```json
 * [{ "id": "<uuidv7>", "action": "breast_left", "at": 1760000000000 }]
 * ```
 *
 * The id is the entry id the core will write, which is what makes a re-delivered
 * broadcast harmless — the same tap can never become two entries.
 */
object PendingQueue {
    const val FILE_NAME = "pending_actions.json"

    private val lock = Any()
    private val random = SecureRandom()

    /** Append a tap; returns the id it was given. */
    fun append(context: Context, action: String, at: Long = System.currentTimeMillis()): String {
        val id = uuidV7()
        synchronized(lock) {
            val file = File(context.filesDir, FILE_NAME)
            val queue = read(file)
            queue.put(
                JSONObject()
                    .put("id", id)
                    .put("action", action)
                    .put("at", at),
            )
            file.writeText(queue.toString())
        }
        return id
    }

    /** How many taps are waiting (used by tests and diagnostics). */
    fun size(context: Context): Int = synchronized(lock) { read(File(context.filesDir, FILE_NAME)).length() }

    private fun read(file: File): JSONArray =
        if (file.exists()) {
            runCatching { JSONArray(file.readText()) }.getOrElse { JSONArray() }
        } else {
            JSONArray()
        }

    /** UUIDv7: 48-bit timestamp + version/variant bits + random. */
    private fun uuidV7(): String {
        val bytes = ByteArray(16)
        random.nextBytes(bytes)
        val millis = System.currentTimeMillis()
        for (index in 0 until 6) {
            bytes[5 - index] = ((millis shr (8 * index)) and 0xFF).toByte()
        }
        bytes[6] = ((bytes[6].toInt() and 0x0F) or 0x70).toByte()
        bytes[8] = ((bytes[8].toInt() and 0x3F) or 0x80).toByte()

        val hex = bytes.joinToString("") { "%02x".format(it) }
        return listOf(
            hex.substring(0, 8),
            hex.substring(8, 12),
            hex.substring(12, 16),
            hex.substring(16, 20),
            hex.substring(20),
        ).joinToString("-")
    }
}
