// Stats screen: play statistics and the score.dat viewer from the Rust bridge (libth08,
// stats.rs `bridge`) in one JSON call.
package org.th08.port

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.FilterChip
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.foundation.lazy.staggeredgrid.LazyStaggeredGridScope
import androidx.compose.foundation.lazy.staggeredgrid.LazyVerticalStaggeredGrid
import androidx.compose.foundation.lazy.staggeredgrid.StaggeredGridCells
import androidx.compose.foundation.lazy.staggeredgrid.StaggeredGridItemSpan
import androidx.compose.ui.unit.Dp
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
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

private fun digits(n: Long) = NumberFormat.getIntegerInstance(Locale.US).format(n)

private fun playTime(frames: Long): String {
    val s = frames / 60
    return "%d:%02d:%02d".format(Locale.US, s / 3600, s / 60 % 60, s % 60)
}

private fun JSONArray.objects() = (0 until length()).map { getJSONObject(it) }

@Composable
fun StatsScreen(rows: List<Setting>, bottom: Dp) {
    val dir = LocalContext.current.filesDir
    val lang = LocalLang.current
    val reply by produceState<Result<JSONObject>?>(null, lang) {
        value = withContext(Dispatchers.Default) {
            runCatching {
                val req = JSONObject().put("dir", dir.path).put("game", File(dir, "game").path).put("lang", lang)
                JSONObject(Stats.call(req.toString())).also { if (it.has("error")) error(it.getString("error")) }
            }
        }
    }
    var shot by rememberSaveable { mutableStateOf(SHOTS) }
    var allCards by rememberSaveable { mutableStateOf(false) }
    val words = Words(rows)
    LazyVerticalStaggeredGrid(
        columns = StaggeredGridCells.Adaptive(GRID_CELL),
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 8.dp, bottom = bottom),
        verticalItemSpacing = 8.dp,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        val r = reply
        when {
            r == null -> item(key = "busy", span = StaggeredGridItemSpan.FullLine) { LinearProgressIndicator(Modifier.fillMaxWidth()) }
            r.isFailure -> item(key = "err", span = StaggeredGridItemSpan.FullLine) {
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
        Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp))
            .background(MaterialTheme.colorScheme.surfaceContainer).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) { content() }
}

@Composable
private fun Title(t: String) = Text(t, style = MaterialTheme.typography.titleMedium)

// Label left, value right; on a narrow screen the value moves under the label instead of squeezing it.
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Line(left: String, right: String) {
    FlowRow(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        itemVerticalAlignment = Alignment.CenterVertically,
    ) {
        Text(left, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.padding(end = 12.dp))
        Text(right, style = MaterialTheme.typography.bodyMedium, fontFamily = FontFamily.Monospace)
    }
}

// One row of a table: the first cell wider, the others equal and centred, so a header row lines up with its rows.
@Composable
private fun Cells(first: String, cells: List<String>, weak: Boolean = false) {
    val style = if (weak) MaterialTheme.typography.bodySmall else MaterialTheme.typography.bodyMedium
    val color = if (weak) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(first, style = style, color = color, modifier = Modifier.weight(2f))
        for (c in cells) {
            Text(c, style = style, color = color, textAlign = TextAlign.Center, maxLines = 1, modifier = Modifier.weight(1f))
        }
    }
}

@Composable
private fun Weak(t: String) =
    Text(t, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)

private fun LazyStaggeredGridScope.playItems(o: JSONObject, w: Words) {
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

private fun LazyStaggeredGridScope.scoreDatItems(
    o: JSONObject,
    w: Words,
    shot: Int,
    setShot: (Int) -> Unit,
    allCards: Boolean,
    setAll: (Boolean) -> Unit,
) {
    item(key = "sd", span = StaggeredGridItemSpan.FullLine) { Title(str(R.string.stats_scoredat)) }
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
    item(key = "clears", span = StaggeredGridItemSpan.FullLine) {
        Card {
            Title(str(R.string.stats_clears))
            Weak(str(R.string.stats_clears_hint))
            val ranks = mutableListOf<String>()
            for (d in 0 until rankKeys.length()) ranks += w.of("rank", rankKeys.getString(d))
            Cells("", ranks, weak = true)
            sd.getJSONArray("clears").let { rowsA ->
                for (s in 0 until rowsA.length()) {
                    val row = rowsA.getJSONArray(s)
                    Cells(w.of("player", shotKeys.getString(s)), (0 until row.length()).map { row.getString(it) })
                }
            }
        }
    }
    item(key = "cards_head", span = StaggeredGridItemSpan.FullLine) {
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
