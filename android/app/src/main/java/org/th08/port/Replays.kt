//! Replay browser: the Rust list / import / delete (libth08, replays.rs `bridge`) and its list screen.
package org.th08.port

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.lazy.staggeredgrid.LazyVerticalStaggeredGrid
import androidx.compose.foundation.lazy.staggeredgrid.StaggeredGridCells
import androidx.compose.foundation.lazy.staggeredgrid.StaggeredGridItemSpan
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.style.TextAlign
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
    var menu by remember { mutableStateOf(false) }
    TextButton(onClick = { act.replaySort(!st.byScore) }, enabled = !st.busy) {
        Text(str(if (st.byScore) R.string.replays_sort_score else R.string.replays_sort_date), maxLines = 1, softWrap = false)
    }
    Box {
        IconButton(onClick = { menu = true }) { Icon(Icons.Default.MoreVert, str(R.string.more)) }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(text = { Text(str(R.string.replays_import)) }, enabled = !st.busy, onClick = { menu = false; act.replayImport() })
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ReplaysScreen(rows: List<Setting>, act: LauncherActions) {
    val st = LocalReplays.current
    var confirm by remember { mutableStateOf<ReplayItem?>(null) }
    val items = st.items.orEmpty()
    if (st.items != null && items.isEmpty() && !st.busy && st.message == null) {
        Box(Modifier.fillMaxSize().padding(32.dp), contentAlignment = Alignment.Center) {
            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(str(R.string.replays_none), style = MaterialTheme.typography.titleLarge, textAlign = TextAlign.Center)
                Text(str(R.string.replays_import_summary), style = MaterialTheme.typography.bodyLarge, textAlign = TextAlign.Center,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)
                Button(onClick = act::replayImport) { Text(str(R.string.replays_import), maxLines = 1, softWrap = false) }
            }
        }
    } else {
        BoxWithConstraints {
            val inline = maxWidth >= 600.dp
            LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (st.busy || st.items == null) item(key = "busy") { LinearProgressIndicator(Modifier.fillMaxWidth()) }
                st.message?.let { m ->
                    item(key = "msg") {
                        Text(m, style = MaterialTheme.typography.bodyMedium,
                            color = if (st.failed) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.fillMaxWidth())
                    }
                }
                items(items, key = { it.file }) { r -> ReplayCard(r, rows, act, inline) { confirm = r } }
            }
        }
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

@Composable
private fun ReplayMenu(r: ReplayItem, act: LauncherActions, delete: () -> Unit) {
    var menu by remember { mutableStateOf(false) }
    Box {
        IconButton(onClick = { menu = true }) { Icon(Icons.Default.MoreVert, str(R.string.more)) }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            if (r.error == null) {
                DropdownMenuItem(text = { Text(str(R.string.replays_export)) }, onClick = { menu = false; act.replayExport(r) })
                DropdownMenuItem(text = { Text(str(R.string.replays_share)) }, onClick = { menu = false; act.replayShare(r) })
            }
            DropdownMenuItem(text = { Text(str(R.string.replays_delete)) }, onClick = { menu = false; delete() })
        }
    }
}

// Title = team and difficulty, score on the right; narrow windows put Watch on its own row.
@Composable
private fun ReplayCard(r: ReplayItem, rows: List<Setting>, act: LauncherActions, inline: Boolean, delete: () -> Unit) {
    val lang = LocalLang.current
    val res = LocalStrings.current
    fun word(key: String, v: String) = rows.firstOrNull { it.e.key == key }?.e?.display(v, lang, res) ?: v
    val c = MaterialTheme.colorScheme
    val score = NumberFormat.getIntegerInstance(java.util.Locale.US).format(r.score)
    val watch: @Composable () -> Unit = {
        Button(onClick = { act.replayWatch(r) }) {
            Icon(Icons.Default.PlayArrow, null)
            Spacer(Modifier.width(4.dp))
            Text(str(R.string.replays_watch), maxLines = 1, softWrap = false)
        }
    }
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(c.surfaceContainer).padding(start = 16.dp, end = 8.dp, top = 12.dp, bottom = 8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f).padding(end = 8.dp)) {
                if (r.error != null) {
                    Text(str(R.string.replays_damaged), style = MaterialTheme.typography.titleMedium, color = c.error)
                } else {
                    val practice = if (r.practice) " · " + str(R.string.replays_practice) else ""
                    Text("${word("player", r.shot)} · ${word("rank", r.rank)}$practice", style = MaterialTheme.typography.titleMedium)
                    Text("${word("stage", r.stage)} · ${r.date}", style = MaterialTheme.typography.bodyMedium, color = c.onSurfaceVariant)
                }
                Text(r.file, style = MaterialTheme.typography.bodySmall, color = c.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
            }
            if (r.error == null) {
                Text(score, style = MaterialTheme.typography.titleMedium.copy(fontFeatureSettings = "tnum"), maxLines = 1, softWrap = false,
                    modifier = Modifier.padding(end = 8.dp))
                if (inline) watch()
            }
            if (inline || r.error != null) ReplayMenu(r, act, delete)
        }
        if (!inline && r.error == null) {
            Row(Modifier.fillMaxWidth().padding(top = 4.dp), horizontalArrangement = Arrangement.End, verticalAlignment = Alignment.CenterVertically) {
                watch()
                ReplayMenu(r, act, delete)
            }
        }
    }
}
