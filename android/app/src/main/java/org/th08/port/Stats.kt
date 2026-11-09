//! Stats screen: play statistics and the score.dat viewer from the Rust bridge (libth08, stats.rs `bridge`).
package org.th08.port

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowDropDown
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ScrollableTabRow
import androidx.compose.material3.Switch
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.intl.LocaleList
import androidx.compose.ui.text.style.LineBreak
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

// Counts launcher resumes, so screens re-read their files after a game.
val LocalResumes = compositionLocalOf { 0 }

private const val SHOTS = 12

internal fun digits(n: Long): String = NumberFormat.getIntegerInstance(Locale.US).format(n)

internal fun playTime(frames: Long): String {
    val s = frames / 60
    return "%d:%02d:%02d".format(Locale.US, s / 3600, s / 60 % 60, s % 60)
}

private fun JSONArray.objects() = (0 until length()).map { getJSONObject(it) }

@Composable
internal fun rememberStats(): State<Result<JSONObject>?> {
    val dir = LocalContext.current.filesDir
    val lang = LocalLang.current
    return produceState<Result<JSONObject>?>(null, lang, LocalResumes.current) {
        value = withContext(Dispatchers.Default) {
            runCatching {
                val req = JSONObject().put("dir", dir.path).put("game", File(dir, "game").path).put("lang", lang)
                JSONObject(Stats.call(req.toString())).also { if (it.has("error")) error(it.getString("error")) }
            }
        }
    }
}

private enum class Tab(val label: Int) {
    Overview(R.string.stats_tab_overview),
    Scores(R.string.stats_scores),
    Cards(R.string.stats_tab_cards),
    History(R.string.stats_tab_history),
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun StatsScreen(rows: List<Setting>, wide: Boolean) {
    val reply by rememberStats()
    val r = reply
    when {
        r == null -> LinearProgressIndicator(Modifier.fillMaxWidth())
        r.isFailure -> Centered(str(R.string.stats_unavailable), null)
        else -> Loaded(r.getOrThrow(), Words(rows), wide)
    }
}

@Composable
private fun Centered(title: String, line: String?) {
    Box(Modifier.fillMaxSize().padding(32.dp), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(title, style = MaterialTheme.typography.titleLarge, textAlign = TextAlign.Center)
            if (line != null) Text(line, style = MaterialTheme.typography.bodyLarge, textAlign = TextAlign.Center,
                color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun Loaded(o: JSONObject, w: Words, wide: Boolean) {
    val st = o.getJSONObject("stats")
    val runs = st.getInt("runs")
    val sd = o.optJSONObject("scoredat")
    if (runs == 0 && sd == null) {
        Centered(str(R.string.stats_none_title), str(if (o.getBoolean("on")) R.string.stats_none else R.string.stats_off))
        return
    }
    val tabs = Tab.entries.filter { (it == Tab.Overview) || (it == Tab.History && runs > 0) || (it != Tab.History && sd != null) }
    var tabName by rememberSaveable { mutableStateOf(tabs.first().name) }
    val tab = tabs.firstOrNull { it.name == tabName } ?: tabs.first()
    var shot by rememberSaveable { mutableStateOf(SHOTS) }
    var allCards by rememberSaveable { mutableStateOf(false) }
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = 16.dp)) {
        item("tiles") { Tiles(st, sd, runs, wide, !o.getBoolean("on")) }
        stickyHeader("tabs") {
            ScrollableTabRow(selectedTabIndex = tabs.indexOf(tab), edgePadding = 16.dp) {
                tabs.forEach { t ->
                    Tab(selected = t == tab, onClick = { tabName = t.name },
                        text = { Text(str(t.label), maxLines = 1) })
                }
            }
        }
        when (tab) {
            Tab.Overview -> overview(o, st, w)
            Tab.Scores -> sd?.let { scores(o, it, w) }
            Tab.Cards -> sd?.let { cards(o, it, w, !wide, shot, { v -> shot = v }, allCards, { v -> allCards = v }) }
            Tab.History -> history(st, w)
        }
    }
}

// Four across in a wide window, two per row on a phone, stacked when large text would not fit two.
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Tiles(st: JSONObject, sd: JSONObject?, runs: Int, wide: Boolean, off: Boolean) {
    val large = LocalDensity.current.fontScale > 1.3f
    val perRow = if (wide) (if (large) 2 else 4) else if (large) 1 else 2
    val best = st.getJSONArray("pairs").objects().maxOfOrNull { it.getLong("best") } ?: 0L
    val total = sd?.getJSONArray("cards")?.length() ?: 0
    val captured = sd?.getJSONArray("cards")?.objects()?.count { it.getJSONArray("story").getJSONArray(SHOTS).getInt(0) > 0 ||
        it.getJSONArray("practice").getJSONArray(SHOTS).getInt(0) > 0 } ?: 0
    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        FlowRow(Modifier.fillMaxWidth(), maxItemsInEachRow = perRow,
            horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Tile(str(R.string.home_time), playTime(st.getLong("frames")))
            Tile(str(R.string.stats_tile_runs), digits(runs.toLong()))
            Tile(str(R.string.home_best), digits(best))
            Tile(str(R.string.stats_tile_cards), if (sd != null) "$captured / $total" else "–")
        }
        if (off) Text(str(R.string.stats_off), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.error)
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun androidx.compose.foundation.layout.FlowRowScope.Tile(label: String, value: String) {
    Column(Modifier.weight(1f).clip(RoundedCornerShape(16.dp)).background(MaterialTheme.colorScheme.surfaceContainer)
        .padding(horizontal = 16.dp, vertical = 12.dp)) {
        Text(label, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(value, style = MaterialTheme.typography.titleMedium.tnum(), maxLines = 1, softWrap = false)
    }
}

// Game text (spell card names) is Japanese in every launcher language: phrase-aware breaks, not mid-word.
private fun TextStyle.cjk(t: String) = if (t.none { it in '\u3040'..'\u30ff' || it in '\u4e00'..'\u9fff' }) this else
    copy(localeList = LocaleList("ja"), lineBreak = LineBreak(LineBreak.Strategy.HighQuality, LineBreak.Strictness.Strict, LineBreak.WordBreak.Phrase))

private fun TextStyle.tnum() = copy(fontFeatureSettings = "tnum")

private class Words(private val rows: List<Setting>) {
    @Composable
    fun of(key: String, v: String): String {
        val lang = LocalLang.current
        val res = LocalStrings.current
        return rows.firstOrNull { it.e.key == key }?.e?.display(v, lang, res) ?: v
    }
}

@Composable
private fun Section(t: String) = Text(t, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary,
    modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 4.dp))

// A list row: title and detail on the left, one right-aligned number column per `values`.
@Composable
private fun Entry(title: String, detail: String?, values: List<String>, head: Boolean = false, stacked: Boolean = false) {
    val c = MaterialTheme.colorScheme
    if (stacked) {
        Column(Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 16.dp, vertical = 6.dp)) {
            Text(title, style = MaterialTheme.typography.bodyLarge.cjk(title))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(detail.orEmpty(), Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium, color = c.onSurfaceVariant)
                values.forEach { v ->
                    Text(v, Modifier.widthIn(min = 72.dp), style = MaterialTheme.typography.titleSmall.tnum(), textAlign = TextAlign.End, maxLines = 1, softWrap = false)
                }
            }
        }
        return
    }
    Row(Modifier.fillMaxWidth().heightIn(min = if (head) 32.dp else 48.dp).padding(horizontal = 16.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f).padding(end = 12.dp)) {
            Text(title, style = (if (head) MaterialTheme.typography.bodySmall else MaterialTheme.typography.bodyLarge).cjk(title),
                color = if (head) c.onSurfaceVariant else c.onSurface)
            if (detail != null) Text(detail, style = MaterialTheme.typography.bodyMedium, color = c.onSurfaceVariant)
        }
        values.forEachIndexed { i, v ->
            Text(v, Modifier.widthIn(min = if (values.size > 1) 72.dp else 0.dp).padding(start = if (i > 0) 8.dp else 0.dp),
                style = if (head) MaterialTheme.typography.bodySmall else MaterialTheme.typography.titleSmall.tnum(),
                color = if (head) c.onSurfaceVariant else c.onSurface, textAlign = TextAlign.End, maxLines = 1, softWrap = false)
        }
    }
}

@Composable
private fun Note(t: String) = Text(t, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant,
    modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp))

private fun LazyListScope.overview(o: JSONObject, st: JSONObject, w: Words) {
    if (st.getInt("runs") == 0) {
        item("none") { Note(str(R.string.stats_none)) }
        return
    }
    item("pairs_h") { Section(str(R.string.stats_by_pair)) }
    items(st.getJSONArray("pairs").objects(), key = { "p" + it.getString("shot_key") + it.getString("rank_key") }) { p ->
        val n = p.getInt("runs")
        Entry("${w.of("player", p.getString("shot_key"))} · ${w.of("rank", p.getString("rank_key"))}",
            plural(R.plurals.stats_runs, n), listOf(digits(p.getLong("best"))))
    }
    val stageKeys = o.getJSONArray("stage_keys")
    item("stages_h") {
        Section(str(R.string.stats_per_stage))
        Entry(str(R.string.stats_col_stage), null, listOf(str(R.string.stats_col_deaths), str(R.string.stats_col_bombs)), head = true)
    }
    val stages = st.getJSONArray("stages").objects()
    items(stages.size, key = { "s$it" }) { i ->
        Entry(w.of("stage", stageKeys.getString(i)), null, listOf(stages[i].getInt("deaths").toString(), stages[i].getInt("bombs").toString()))
    }
    val failed = st.getJSONArray("failed").objects()
    item("failed_h") {
        Section(str(R.string.stats_failed))
        if (failed.isEmpty()) Note(str(R.string.stats_empty))
        else Entry("", null, listOf(str(R.string.stats_col_failed), str(R.string.stats_col_captured)), head = true)
    }
    items(failed, key = { "f" + it.getInt("card") }) { c ->
        val a = c.getInt("attempts")
        val cap = c.getInt("captures")
        val no = c.getInt("card") + 1
        Entry(c.getString("name").ifEmpty { str(R.string.stats_card_no, no) }, "No. %03d".format(Locale.US, no),
            listOf((a - cap).toString(), "$cap / $a"))
    }
}

private fun LazyListScope.history(st: JSONObject, w: Words) {
    item("last_h") { Section(str(R.string.stats_last)) }
    items(st.getJSONArray("last").objects(), key = { "r" + it.getLong("t") }) { r ->
        val fmt = remember { SimpleDateFormat("yyyy-MM-dd HH:mm", Locale.US) }
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
        Entry("${w.of("player", r.getString("shot_key"))} · ${w.of("rank", r.getString("rank_key"))} · ${w.of("stage", r.getString("stage_key"))}",
            "${fmt.format(Date(r.getLong("t") * 1000))} · ${str(mode)} · ${playTime(r.getLong("frames"))}\n" +
                "${plural(R.plurals.stats_deaths, r.getInt("deaths"))} · ${plural(R.plurals.stats_bombs, r.getInt("bombs"))} · " +
                "${plural(R.plurals.stats_continues, r.getInt("continues"))} · ${str(end)}",
            listOf(digits(r.getLong("score"))))
    }
}

private fun LazyListScope.scores(o: JSONObject, sd: JSONObject, w: Words) {
    val tables = sd.getJSONArray("scores").objects()
    if (tables.isEmpty()) item("no_scores") { Note(str(R.string.stats_empty)) }
    tables.forEachIndexed { ti, t ->
        item("t$ti") { Section("${w.of("rank", t.getString("rank_key"))} · ${w.of("player", t.getString("shot_key"))}") }
        val es = t.getJSONArray("entries").objects()
        items(es.size, key = { "t$ti:$it" }) { i ->
            val e = es[i]
            val stage = if (e.getInt("stage") == 99) str(R.string.stats_cleared) else w.of("stage", e.getString("stage_key"))
            Entry("${i + 1}. ${e.getString("name").ifEmpty { "-" }}", "$stage · ${e.getString("date")}", listOf(digits(e.getLong("score"))))
        }
    }
    val shotKeys = o.getJSONArray("shot_keys")
    val rankKeys = o.getJSONArray("rank_keys")
    val clears = sd.getJSONArray("clears")
    item("clears_h") {
        Section(str(R.string.stats_clears))
        Note(str(R.string.stats_clears_hint))
    }
    val rows = (0 until clears.length()).map { s -> s to clears.getJSONArray(s) }.filter { (_, row) -> (0 until row.length()).any { row.getString(it).any(Char::isLetter) } }
    if (rows.isEmpty()) item("no_clears") { Note(str(R.string.stats_empty)) }
    items(rows, key = { "c" + it.first }) { (s, row) ->
        val marks = (0 until row.length()).filter { row.getString(it).any(Char::isLetter) }.map { "${w.of("rank", rankKeys.getString(it))} ${row.getString(it)}" }
        Entry(w.of("player", shotKeys.getString(s)), marks.joinToString(" · "), emptyList())
    }
}

private fun LazyListScope.cards(o: JSONObject, sd: JSONObject, w: Words, stacked: Boolean, shot: Int, setShot: (Int) -> Unit, all: Boolean, setAll: (Boolean) -> Unit) {
    val shotKeys = o.getJSONArray("shot_keys")
    item("cards_controls") {
        Section(str(R.string.stats_cards))
        var menu by remember { mutableStateOf(false) }
        Box(Modifier.padding(horizontal = 8.dp)) {
            TextButton(onClick = { menu = true }) {
                Text(if (shot == SHOTS) str(R.string.stats_all_shots) else w.of("player", shotKeys.getString(shot)))
                Icon(Icons.Default.ArrowDropDown, null)
            }
            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                DropdownMenuItem(text = { Text(str(R.string.stats_all_shots)) }, onClick = { menu = false; setShot(SHOTS) })
                for (k in 0 until SHOTS) {
                    DropdownMenuItem(text = { Text(w.of("player", shotKeys.getString(k))) }, onClick = { menu = false; setShot(k) })
                }
            }
        }
        Row(Modifier.fillMaxWidth().toggleable(all, role = Role.Switch, onValueChange = setAll).heightIn(min = 48.dp).padding(horizontal = 16.dp),
            verticalAlignment = Alignment.CenterVertically) {
            Text(str(R.string.stats_cards_all), Modifier.weight(1f).padding(end = 12.dp), style = MaterialTheme.typography.bodyLarge)
            Switch(checked = all, onCheckedChange = null)
        }
        Entry("", null, listOf(str(R.string.stats_mode_story), str(R.string.stats_mode_spell)), head = true)
    }
    val cards = sd.getJSONArray("cards").objects().filter { c ->
        all || c.getJSONArray("story").getJSONArray(SHOTS).getInt(1) != 0 || c.getJSONArray("practice").getJSONArray(SHOTS).getInt(1) != 0
    }
    if (cards.isEmpty()) item("no_cards") { Note(str(R.string.stats_empty)) }
    items(cards, key = { "card" + it.getInt("id") }) { c ->
        val s = c.getJSONArray("story").getJSONArray(shot)
        val p = c.getJSONArray("practice").getJSONArray(shot)
        val id = c.getInt("id") + 1
        Entry(c.getString("name").ifEmpty { str(R.string.stats_card_no, id) }, "No. %03d".format(Locale.US, id),
            listOf("${s.getInt(0)} / ${s.getInt(1)}", "${p.getInt(0)} / ${p.getInt(1)}"), stacked = stacked)
    }
}
