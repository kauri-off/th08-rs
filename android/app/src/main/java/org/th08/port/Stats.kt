// Stats screen of the settings screen: play statistics and the score.dat viewer from the Rust bridge (libth08,
// stats.rs `bridge`) in one JSON call.
package org.th08.port

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.text.NumberFormat
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

object Stats {
    init {
        System.loadLibrary("th08")
    }

    @JvmStatic
    external fun call(request: String): String
}

private const val SHOTS = 12

@Composable
fun StatsBlock(rows: List<Setting>) {
    var open by remember { mutableStateOf(false) }
    ShellRow(str(R.string.stats_open), str(R.string.stats_open_summary), modifier = Modifier.clickable { open = true })
    if (open) StatsDialog(rows) { open = false }
}

private fun digits(n: Long) = NumberFormat.getIntegerInstance(Locale.US).format(n)

private fun playTime(frames: Long): String {
    val s = frames / 60
    return "%d:%02d:%02d".format(Locale.US, s / 3600, s / 60 % 60, s % 60)
}

private fun JSONArray.objects() = (0 until length()).map { getJSONObject(it) }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun StatsDialog(rows: List<Setting>, close: () -> Unit) {
    val dir = LocalContext.current.filesDir
    val lang = LocalLang.current
    val reply by produceState<Result<JSONObject>?>(null) {
        value = withContext(Dispatchers.Default) {
            runCatching {
                val req = JSONObject().put("dir", dir.path).put("game", File(dir, "game").path).put("lang", lang)
                JSONObject(Stats.call(req.toString())).also { if (it.has("error")) error(it.getString("error")) }
            }
        }
    }
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Scaffold(
            containerColor = MaterialTheme.colorScheme.surface,
            contentWindowInsets = WindowInsets.safeDrawing,
            topBar = {
                TopAppBar(
                    title = { Text(str(R.string.stats_title)) },
                    navigationIcon = {
                        IconButton(onClick = close) { Icon(Icons.AutoMirrored.Filled.ArrowBack, str(R.string.cancel)) }
                    },
                )
            },
        ) { pad ->
            var shot by remember { mutableStateOf(SHOTS) }
            var allCards by remember { mutableStateOf(false) }
            val words = Words(rows)
            LazyColumn(
                Modifier.fillMaxSize().padding(pad),
                contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                val r = reply
                when {
                    r == null -> item(key = "busy") { LinearProgressIndicator(Modifier.widthIn(max = 640.dp).fillMaxWidth()) }
                    r.isFailure -> item(key = "err") {
                        Text(r.exceptionOrNull()?.message ?: "", color = MaterialTheme.colorScheme.error)
                    }
                    else -> {
                        val o = r.getOrThrow()
                        playItems(o, words)
                        scoreDatItems(o, words, shot, { shot = it }, allCards, { allCards = it })
                    }
                }
            }
        }
    }
}

// The launcher words of the `player`, `rank` and `stage` value texts.
private class Words(private val rows: List<Setting>) {
    @Composable
    fun of(key: String, v: String): String {
        val lang = LocalLang.current
        val res = LocalStrings.current
        return rows.firstOrNull { it.e.key == key }?.e?.display(v, lang, res) ?: v
    }
}

@Composable
private fun Card(content: @Composable () -> Unit) {
    Column(
        Modifier.widthIn(max = 640.dp).fillMaxWidth().clip(RoundedCornerShape(16.dp))
            .background(MaterialTheme.colorScheme.surfaceContainer).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) { content() }
}

@Composable
private fun Title(t: String) = Text(t, style = MaterialTheme.typography.titleMedium)

@Composable
private fun Line(left: String, right: String) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(left, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
        Text(right, style = MaterialTheme.typography.bodyMedium, fontFamily = FontFamily.Monospace)
    }
}

@Composable
private fun Weak(t: String) =
    Text(t, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)

private fun LazyListScope.playItems(o: JSONObject, w: Words) {
    val st = o.getJSONObject("stats")
    val stageKeys = o.getJSONArray("stage_keys")
    item(key = "head") {
        Card {
            Title(str(R.string.stats_playtime, playTime(st.getLong("frames")), st.getInt("runs")))
            if (!o.getBoolean("on")) Text(str(R.string.stats_off), color = MaterialTheme.colorScheme.error)
            if (st.getInt("runs") == 0) Weak(str(R.string.stats_none))
        }
    }
    if (st.getInt("runs") == 0) return
    item(key = "pairs") {
        Card {
            Title(str(R.string.stats_by_pair))
            for (p in st.getJSONArray("pairs").objects()) {
                Line("${w.of("player", p.getString("shot_key"))} · ${w.of("rank", p.getString("rank_key"))}",
                    str(R.string.stats_runs_best, p.getInt("runs"), digits(p.getLong("best"))))
            }
        }
    }
    item(key = "stages") {
        Card {
            Title(str(R.string.stats_per_stage))
            st.getJSONArray("stages").objects().forEachIndexed { i, s ->
                Line(w.of("stage", stageKeys.getString(i)), str(R.string.stats_deaths_bombs, s.getInt("deaths"), s.getInt("bombs")))
            }
        }
    }
    item(key = "failed") {
        Card {
            Title(str(R.string.stats_failed))
            val failed = st.getJSONArray("failed").objects()
            if (failed.isEmpty()) Weak(str(R.string.stats_empty))
            for (c in failed) {
                val a = c.getInt("attempts")
                val cap = c.getInt("captures")
                val no = c.getInt("card") + 1
                Text(c.getString("name").ifEmpty { str(R.string.stats_card_no, no) }.let { "%03d  %s".format(no, it) },
                    style = MaterialTheme.typography.bodyMedium)
                Weak(str(R.string.stats_failed_line, a - cap, cap, a))
            }
        }
    }
    item(key = "last") {
        Card {
            Title(str(R.string.stats_last))
            val fmt = remember { SimpleDateFormat("yyyy-MM-dd HH:mm", Locale.US) }
            for (r in st.getJSONArray("last").objects()) {
                val mode = when (r.getString("mode")) {
                    "practice" -> R.string.stats_mode_practice
                    "spell" -> R.string.stats_mode_spell
                    else -> R.string.stats_mode_story
                }
                val end = when (r.getString("reason")) {
                    "menu" -> R.string.stats_end_menu
                    "ending" -> R.string.stats_end_ending
                    "retry" -> R.string.stats_end_retry
                    "quit" -> R.string.stats_end_quit
                    else -> R.string.stats_end_none
                }
                Line("${fmt.format(Date(r.getLong("t") * 1000))} · ${str(mode)}", digits(r.getLong("score")))
                Weak("${w.of("player", r.getString("shot_key"))} · ${w.of("rank", r.getString("rank_key"))} · " +
                    "${w.of("stage", r.getString("stage_key"))} · ${playTime(r.getLong("frames"))}")
                Weak(str(R.string.stats_run_detail, r.getInt("deaths"), r.getInt("bombs"), r.getInt("continues"), str(end)))
            }
        }
    }
}

private fun LazyListScope.scoreDatItems(
    o: JSONObject,
    w: Words,
    shot: Int,
    setShot: (Int) -> Unit,
    allCards: Boolean,
    setAll: (Boolean) -> Unit,
) {
    item(key = "sd") { Title(str(R.string.stats_scoredat)) }
    if (!o.has("scoredat")) {
        item(key = "sd_err") { Card { Weak(str(R.string.stats_sd_error, o.optString("sd_error"))) } }
        return
    }
    val sd = o.getJSONObject("scoredat")
    val shotKeys = o.getJSONArray("shot_keys")
    val rankKeys = o.getJSONArray("rank_keys")
    item(key = "scores") {
        Card {
            Title(str(R.string.stats_scores))
            val tables = sd.getJSONArray("scores").objects()
            if (tables.isEmpty()) Weak(str(R.string.stats_empty))
            for (t in tables) {
                Text("${w.of("rank", t.getString("rank_key"))} · ${w.of("player", t.getString("shot_key"))}",
                    style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                t.getJSONArray("entries").objects().forEachIndexed { i, e ->
                    val stage = if (e.getInt("stage") == 99) str(R.string.stats_cleared) else w.of("stage", e.getString("stage_key"))
                    Line("${i + 1}. ${e.getString("name").ifEmpty { "-" }} · $stage · ${e.getString("date")}", digits(e.getLong("score")))
                }
            }
        }
    }
    item(key = "clears") {
        Card {
            Title(str(R.string.stats_clears))
            Weak(str(R.string.stats_clears_hint))
            val ranks = mutableListOf<String>()
            for (d in 0 until rankKeys.length()) ranks += w.of("rank", rankKeys.getString(d))
            Weak(ranks.joinToString("  "))
            sd.getJSONArray("clears").let { rowsA ->
                for (s in 0 until rowsA.length()) {
                    val row = rowsA.getJSONArray(s)
                    Line(w.of("player", shotKeys.getString(s)), (0 until row.length()).joinToString(" ") { "%-4s".format(row.getString(it)) })
                }
            }
        }
    }
    item(key = "cards_head") {
        Card {
            Title(str(R.string.stats_cards))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(selected = shot == SHOTS, onClick = { setShot(SHOTS) }, label = { Text(str(R.string.stats_all_shots)) })
                for (k in 0 until SHOTS) {
                    FilterChip(selected = shot == k, onClick = { setShot(k) }, label = { Text(w.of("player", shotKeys.getString(k))) })
                }
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(str(R.string.stats_cards_all), Modifier.weight(1f))
                Switch(checked = allCards, onCheckedChange = setAll)
            }
        }
    }
    val cards = sd.getJSONArray("cards").objects().filter { c ->
        allCards || c.getJSONArray("story").getJSONArray(SHOTS).getInt(1) != 0 ||
            c.getJSONArray("practice").getJSONArray(SHOTS).getInt(1) != 0
    }
    items(cards.size, key = { "card${cards[it].getInt("id")}" }) { i ->
        val c = cards[i]
        val s = c.getJSONArray("story").getJSONArray(shot)
        val p = c.getJSONArray("practice").getJSONArray(shot)
        val id = c.getInt("id") + 1
        Card {
            Text("%03d  %s".format(Locale.US, id, c.getString("name").ifEmpty { str(R.string.stats_card_no, id) }),
                style = MaterialTheme.typography.bodyMedium)
            Weak(str(R.string.stats_cards_line, s.getInt(0), s.getInt(1), p.getInt(0), p.getInt(1)))
        }
    }
}
