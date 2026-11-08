// Replay browser: the Rust list / import / delete (libth08, replays.rs `bridge`) behind one
// JSON call, and its list screen with Watch, Export, Share and Delete.
package org.th08.port

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.lazy.staggeredgrid.LazyVerticalStaggeredGrid
import androidx.compose.foundation.lazy.staggeredgrid.StaggeredGridCells
import androidx.compose.foundation.lazy.staggeredgrid.StaggeredGridItemSpan
import androidx.compose.ui.unit.Dp
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
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
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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

@Composable
fun ReplaysActions(act: LauncherActions) {
    val st = LocalReplays.current
    IconButton(onClick = act::replayImport, enabled = !st.busy) { Icon(Icons.Default.Add, str(R.string.replays_import)) }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ReplaysScreen(rows: List<Setting>, act: LauncherActions, bottom: Dp) {
    val st = LocalReplays.current
    var confirm by remember { mutableStateOf<ReplayItem?>(null) }
    LazyVerticalStaggeredGrid(
        columns = StaggeredGridCells.Adaptive(GRID_CELL),
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 8.dp, bottom = bottom),
        verticalItemSpacing = 8.dp,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        item(key = "sort", span = StaggeredGridItemSpan.FullLine) {
            SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                listOf(false to R.string.replays_sort_date, true to R.string.replays_sort_score)
                    .forEachIndexed { i, (score, label) ->
                        SegmentedButton(
                            selected = st.byScore == score,
                            onClick = { act.replaySort(score) },
                            shape = SegmentedButtonDefaults.itemShape(i, 2),
                            icon = {},
                        ) { Text(str(label), maxLines = 1) }
                    }
            }
        }
        if (st.busy || st.items == null) {
            item(key = "busy", span = StaggeredGridItemSpan.FullLine) { LinearProgressIndicator(Modifier.fillMaxWidth()) }
        }
        st.message?.let { m ->
            item(key = "msg", span = StaggeredGridItemSpan.FullLine) {
                Text(m, style = MaterialTheme.typography.bodyMedium,
                    color = if (st.failed) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.fillMaxWidth())
            }
        }
        val items = st.items.orEmpty()
        if (st.items != null && items.isEmpty() && !st.busy) {
            item(key = "none", span = StaggeredGridItemSpan.FullLine) {
                Column(Modifier.fillMaxWidth().padding(32.dp), horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(str(R.string.replays_none), style = MaterialTheme.typography.titleMedium)
                    Text(str(R.string.replays_import_summary), style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                    OutlinedButton(onClick = act::replayImport) { Text(str(R.string.replays_import), maxLines = 1) }
                }
            }
        }
        items(items.size, key = { items[it].file }) { i -> val r = items[i]; ReplayCard(r, rows, act) { confirm = r } }
    }
    confirm?.let { r ->
        AlertDialog(
            onDismissRequest = { confirm = null },
            text = { Text(str(R.string.replays_delete_confirm, r.file)) },
            confirmButton = {
                TextButton(onClick = { confirm = null; act.replayDelete(r) }) {
                    Text(str(R.string.replays_delete), color = MaterialTheme.colorScheme.error, maxLines = 1)
                }
            },
            dismissButton = { TextButton(onClick = { confirm = null }) { Text(str(R.string.cancel), maxLines = 1) } },
        )
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ReplayCard(r: ReplayItem, rows: List<Setting>, act: LauncherActions, delete: () -> Unit) {
    val lang = LocalLang.current
    val res = LocalStrings.current
    fun word(key: String, v: String) = rows.firstOrNull { it.e.key == key }?.e?.display(v, lang, res) ?: v
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp))
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
                    Text(str(R.string.replays_watch), maxLines = 1)
                }
                OutlinedButton(onClick = { act.replayExport(r) }) { Text(str(R.string.replays_export), maxLines = 1) }
                IconButton(onClick = { act.replayShare(r) }) { Icon(Icons.Default.Share, str(R.string.replays_share)) }
            }
            IconButton(onClick = delete) { Icon(Icons.Default.Delete, str(R.string.replays_delete)) }
        }
    }
}
