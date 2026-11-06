// Replay browser of the settings screen: the Rust list / import / delete (libth08, replays.rs `bridge`) behind one
// JSON call, and its full-screen list with Watch, Export, Share and Delete.
package org.th08.port

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Share
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import org.json.JSONObject
import java.text.NumberFormat

object Replays {
    init {
        System.loadLibrary("th08")
    }

    // saves.rs `MAX_REPLAY`.
    const val MAX_RPY = 16L shl 20

    @JvmStatic
    external fun call(request: String): String
}

// One file of the list; `error` set = damaged (the other fields are empty). shot / rank / stage are the value texts
// of the `player`, `rank` and `stage` settings, shown with their words.
data class ReplayItem(
    val file: String,
    val path: String,
    val shot: String = "",
    val rank: String = "",
    val stage: String = "",
    val score: Long = 0,
    val date: String = "",
    val name: String = "",
    val slowdown: Double = 0.0,
    val practice: Boolean = false,
    val error: String? = null,
)

data class ReplayState(
    // null = not read yet.
    val items: List<ReplayItem>? = null,
    val byScore: Boolean = false,
    val busy: Boolean = false,
    val message: String? = null,
    val failed: Boolean = false,
)

val LocalReplays = compositionLocalOf { ReplayState() }

// The bridge's "list" reply; throws on {"error"}.
fun parseReplayList(s: String): List<ReplayItem> {
    val o = JSONObject(s)
    if (o.has("error")) error(o.getString("error"))
    val a = o.getJSONArray("replays")
    return (0 until a.length()).map { i ->
        val r = a.getJSONObject(i)
        if (r.has("error")) {
            ReplayItem(r.getString("file"), r.getString("path"), error = r.getString("error"))
        } else {
            ReplayItem(
                file = r.getString("file"),
                path = r.getString("path"),
                shot = r.getString("shot"),
                rank = r.getString("rank"),
                stage = r.getString("stage"),
                score = r.getLong("score"),
                date = r.getString("date"),
                name = r.getString("name"),
                slowdown = r.getDouble("slowdown"),
                practice = r.getBoolean("practice"),
            )
        }
    }
}

// The settings-screen block: opens the list and imports.
@Composable
fun ReplaysBlock(rows: List<Setting>, act: LauncherActions) {
    val st = LocalReplays.current
    var open by remember { mutableStateOf(false) }
    val n = st.items?.size ?: 0
    ShellRow(str(R.string.replays_browse),
        if (n == 0) str(R.string.replays_none) else str(R.string.replays_count, n),
        modifier = Modifier.clickable { open = true })
    ShellRow(str(R.string.replays_import), str(R.string.replays_import_summary),
        modifier = Modifier.clickable(enabled = !st.busy) { act.replayImport() })
    if (open) ReplaysDialog(rows, act) { open = false }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ReplaysDialog(rows: List<Setting>, act: LauncherActions, close: () -> Unit) {
    val st = LocalReplays.current
    var confirm by remember { mutableStateOf<ReplayItem?>(null) }
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Scaffold(
            containerColor = MaterialTheme.colorScheme.surface,
            contentWindowInsets = WindowInsets.safeDrawing,
            topBar = {
                TopAppBar(
                    title = { Text(str(R.string.replays_title)) },
                    navigationIcon = {
                        IconButton(onClick = close) { Icon(Icons.AutoMirrored.Filled.ArrowBack, str(R.string.cancel)) }
                    },
                    actions = {
                        IconButton(onClick = act::replayImport, enabled = !st.busy) {
                            Icon(Icons.Default.Add, str(R.string.replays_import))
                        }
                    },
                )
            },
        ) { pad ->
            LazyColumn(
                Modifier.fillMaxSize().padding(pad),
                contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                item(key = "sort") {
                    SingleChoiceSegmentedButtonRow(Modifier.widthIn(max = 640.dp).fillMaxWidth()) {
                        listOf(false to R.string.replays_sort_date, true to R.string.replays_sort_score)
                            .forEachIndexed { i, (score, label) ->
                                SegmentedButton(
                                    selected = st.byScore == score,
                                    onClick = { act.replaySort(score) },
                                    shape = SegmentedButtonDefaults.itemShape(i, 2),
                                ) { Text(str(label)) }
                            }
                    }
                }
                if (st.busy) item(key = "busy") { LinearProgressIndicator(Modifier.widthIn(max = 640.dp).fillMaxWidth()) }
                st.message?.let { m ->
                    item(key = "msg") {
                        Text(m, style = MaterialTheme.typography.bodyMedium,
                            color = if (st.failed) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.widthIn(max = 640.dp).fillMaxWidth())
                    }
                }
                val items = st.items.orEmpty()
                if (items.isEmpty() && !st.busy) {
                    item(key = "none") { Text(str(R.string.replays_none), Modifier.padding(16.dp)) }
                }
                items(items, key = { it.file }) { r -> ReplayCard(r, rows, act) { confirm = r } }
            }
        }
        confirm?.let { r ->
            AlertDialog(
                onDismissRequest = { confirm = null },
                text = { Text(str(R.string.replays_delete_confirm, r.file)) },
                confirmButton = {
                    TextButton(onClick = { confirm = null; act.replayDelete(r) }) {
                        Text(str(R.string.replays_delete), color = MaterialTheme.colorScheme.error)
                    }
                },
                dismissButton = { TextButton(onClick = { confirm = null }) { Text(str(R.string.cancel)) } },
            )
        }
    }
}

@Composable
private fun ReplayCard(r: ReplayItem, rows: List<Setting>, act: LauncherActions, delete: () -> Unit) {
    val lang = LocalLang.current
    val res = LocalStrings.current
    fun word(key: String, v: String) = rows.firstOrNull { it.e.key == key }?.e?.display(v, lang, res) ?: v
    Column(
        Modifier.widthIn(max = 640.dp).fillMaxWidth().clip(RoundedCornerShape(16.dp))
            .background(MaterialTheme.colorScheme.surfaceContainer).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        if (r.error != null) {
            Text(r.file, style = MaterialTheme.typography.titleMedium)
            Text(str(R.string.replays_damaged, r.error), color = MaterialTheme.colorScheme.error,
                style = MaterialTheme.typography.bodyMedium)
        } else {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(r.name.ifEmpty { "-" }, style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
                Text(NumberFormat.getIntegerInstance(java.util.Locale.US).format(r.score),
                    style = MaterialTheme.typography.titleMedium)
            }
            val practice = if (r.practice) " · " + str(R.string.replays_practice) else ""
            Text("${word("player", r.shot)} · ${word("rank", r.rank)} · ${word("stage", r.stage)}$practice",
                style = MaterialTheme.typography.bodyMedium)
            Text(str(R.string.replays_detail, r.file, r.date, "%.2f".format(java.util.Locale.US, r.slowdown)),
                style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Spacer(Modifier.width(4.dp))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (r.error == null) {
                Button(onClick = { act.replayWatch(r) }) {
                    Icon(Icons.Default.PlayArrow, null)
                    Spacer(Modifier.width(4.dp))
                    Text(str(R.string.replays_watch))
                }
                OutlinedButton(onClick = { act.replayExport(r) }) { Text(str(R.string.replays_export)) }
                IconButton(onClick = { act.replayShare(r) }) { Icon(Icons.Default.Share, str(R.string.replays_share)) }
            }
            IconButton(onClick = delete) { Icon(Icons.Default.Delete, str(R.string.replays_delete)) }
        }
    }
}
