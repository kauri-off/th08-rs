// Game data import: the system picker's th08 / thbgm files streamed by descriptor into the Rust converter (libth08,
// th08-repack `import::bridge`), which writes checked th08.pak + thbgm.pak into filesDir/game.
package org.th08.port

import android.app.ActivityManager
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

object GameImport {
    init {
        System.loadLibrary("th08")
    }

    @JvmStatic
    external fun call(request: String): String
}

sealed interface ImportOutcome {
    data class Done(val secs: Double) : ImportOutcome
    data object Cancelled : ImportOutcome
    // kind: wrong_file | version | no_space | failed (Rust `bridge::kind`); `need` = MB short of space.
    data class Failed(val kind: String, val message: String, val needMb: Long = 0) : ImportOutcome
}

data class ImportState(
    val busy: Boolean = false,
    val stage: String? = null,
    val done: Long = 0,
    val total: Long = 0,
    val outcome: ImportOutcome? = null,
) {
    companion object {
        fun parse(s: String): ImportState {
            val o = JSONObject(s)
            if (o.has("error")) return ImportState(outcome = ImportOutcome.Failed("failed", o.getString("error")))
            val out = o.optJSONObject("outcome")
            return ImportState(
                busy = o.getBoolean("busy"),
                stage = if (o.isNull("stage")) null else o.getString("stage"),
                done = o.getLong("done"),
                total = o.getLong("total"),
                outcome = out?.let {
                    when (val k = it.getString("kind")) {
                        "ok" -> ImportOutcome.Done(it.getDouble("secs"))
                        "cancelled" -> ImportOutcome.Cancelled
                        else -> ImportOutcome.Failed(k, it.getString("message"))
                    }
                },
            )
        }
    }
}

val LocalImport = compositionLocalOf { ImportState() }

// The game folder the import writes and the game reads first (lib.rs `android_main`).
fun importDir(ctx: ComponentActivity) = File(ctx.filesDir, "game")

fun hasFile(dir: File, vararg names: String) =
    dir.listFiles()?.any { f -> f.isFile && names.any { f.name.equals(it, ignoreCase = true) } } == true

class GameImporter(private val act: ComponentActivity) {
    var state by mutableStateOf(ImportState())

    private fun result(req: JSONObject): ImportState = runCatching { ImportState.parse(GameImport.call(req.toString())) }
        .getOrElse { ImportState(outcome = ImportOutcome.Failed("failed", it.message ?: it.toString())) }

    private fun call(req: JSONObject) {
        state = result(req)
    }

    private fun request(op: String) = JSONObject().put("op", op).put("game", importDir(act).path)

    fun poll() = call(request("status"))

    fun cancel() = call(request("cancel"))

    // Blocking (deletes the imported files): call off the UI thread, then set `state`.
    fun removeState() = result(request("remove"))

    // Output size estimate per picked file (MB): th08.dat → ~26 MB pack, thbgm.dat → ~48 MB; a .pak as it is.
    private fun needMb(name: String, size: Long): Long = when (name.lowercase()) {
        "th08.dat" -> 32
        "thbgm.dat" -> 56
        else -> size / (1 shl 20) + 1
    }

    fun start(uris: List<Uri>) {
        if (uris.isEmpty() || state.busy) return
        val picked = uris.map { uri ->
            val (name, size) = act.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE),
                null, null, null)?.use { c ->
                if (c.moveToFirst()) (c.getString(0) ?: "") to (if (c.isNull(1)) 0L else c.getLong(1)) else "" to 0L
            } ?: ("" to 0L)
            Triple(uri, name, size)
        }
        val need = picked.sumOf { needMb(it.second, it.third) } + 16
        val free = act.filesDir.usableSpace / (1 shl 20)
        if (free < need) {
            state = ImportState(outcome = ImportOutcome.Failed("no_space", "", need - free))
            return
        }
        val files = JSONArray()
        val fds = mutableListOf<Int>()
        for ((uri, name, _) in picked) {
            val fd = runCatching { act.contentResolver.openFileDescriptor(uri, "r")!!.detachFd() }.getOrElse {
                Log.e("th08", "import: cannot open $name: $it")
                fds.forEach { f -> runCatching { ParcelFileDescriptor.adoptFd(f).close() } }
                state = ImportState(outcome = ImportOutcome.Failed("failed", "$name: ${it.message ?: it}"))
                return
            }
            fds.add(fd)
            files.put(JSONObject().put("name", name).put("fd", fd))
        }
        call(request("start").put("files", files).put("low_ram", lowRam()))
    }

    // Fewer parallel track encodes (each holds a whole track's PCM) on phones with little memory.
    private fun lowRam(): Boolean {
        val am = act.getSystemService(ActivityManager::class.java) ?: return false
        val mem = ActivityManager.MemoryInfo().also(am::getMemoryInfo)
        return am.isLowRamDevice || mem.totalMem < 3L * 1024 * 1024 * 1024
    }

    // Logs the outcome once a job ended (timings for the emulator check).
    fun logOutcome() {
        when (val o = state.outcome) {
            is ImportOutcome.Done -> Log.i("th08", "import done in %.1f s".format(o.secs))
            is ImportOutcome.Failed -> Log.w("th08", "import failed (${o.kind}): ${o.message}")
            ImportOutcome.Cancelled -> Log.i("th08", "import cancelled")
            null -> {}
        }
    }
}

@Composable
private fun Modifier.rowSurface() = clip(RoundedCornerShape(4.dp)).background(MaterialTheme.colorScheme.surfaceContainer)

private fun Modifier.padding16() = padding(horizontal = 16.dp, vertical = 12.dp)

// Rows under the Game data status: import / remove buttons, the running job and the last outcome.
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ImportRows(d: GameData, act: LauncherActions) {
    val st = LocalImport.current
    var confirm by remember { mutableStateOf(false) }
    if (st.busy) {
        val pct = if (st.total > 0) (100 * st.done / st.total).toInt() else 0
        val title = when (st.stage) {
            "bgm" -> str(R.string.import_bgm, pct)
            else -> str(R.string.import_content, pct)
        }
        ShellRow(title, str(R.string.import_keep_open), below = {
            if (st.total > 0) {
                LinearProgressIndicator(progress = { st.done.toFloat() / st.total }, Modifier.fillMaxWidth())
            } else {
                LinearProgressIndicator(Modifier.fillMaxWidth())
            }
        }) { TextButton(onClick = act::cancelImport) { Text(str(R.string.cancel)) } }
        return
    }
    st.outcome?.let { o ->
        val (text, err) = when (o) {
            is ImportOutcome.Done -> str(R.string.import_done) to false
            ImportOutcome.Cancelled -> str(R.string.import_cancelled) to false
            is ImportOutcome.Failed -> when (o.kind) {
                "wrong_file" -> str(R.string.import_wrong_file, o.message)
                "version" -> str(R.string.import_version)
                "no_space" -> if (o.needMb > 0) str(R.string.import_no_space_mb, o.needMb) else str(R.string.import_no_space)
                else -> str(R.string.import_failed, o.message)
            } to true
        }
        Text(text, style = MaterialTheme.typography.bodyMedium,
            color = if (err) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.fillMaxWidth().rowSurface().padding16())
    }
    val imported = d is GameData.Imported
    FlowRow(Modifier.fillMaxWidth().rowSurface().padding16(),
        horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedButton(onClick = act::importGame) {
            Text(str(if (imported && d.music) R.string.import_again else R.string.import_files))
        }
        if (imported) OutlinedButton(onClick = { confirm = true }) { Text(str(R.string.import_remove)) }
    }
    if (confirm) {
        AlertDialog(
            onDismissRequest = { confirm = false },
            title = { Text(str(R.string.import_remove)) },
            text = { Text(str(R.string.import_remove_text)) },
            confirmButton = { TextButton(onClick = { confirm = false; act.removeGame() }) { Text(str(R.string.import_remove)) } },
            dismissButton = { TextButton(onClick = { confirm = false }) { Text(str(R.string.cancel)) } },
        )
    }
}
