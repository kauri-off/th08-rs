//! Launcher screen: Home with Play, Replays, Stats and Settings behind a navigation bar (rail in wide windows).
package org.th08.port

import android.content.Context
import android.content.res.Configuration
import android.content.res.Resources
import android.icu.text.BreakIterator
import android.icu.util.ULocale
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.lazy.staggeredgrid.LazyStaggeredGridScope
import androidx.compose.foundation.lazy.staggeredgrid.LazyVerticalStaggeredGrid
import androidx.compose.foundation.lazy.staggeredgrid.StaggeredGridCells
import androidx.compose.foundation.lazy.staggeredgrid.StaggeredGridItemSpan
import androidx.compose.material.icons.filled.ArrowDropDown
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.List
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.Dp
import kotlinx.coroutines.launch
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.adaptive.currentWindowAdaptiveInfo
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.contentDescription
import androidx.window.core.layout.WindowHeightSizeClass
import androidx.window.core.layout.WindowWidthSizeClass
import androidx.compose.foundation.Image
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Typography
import androidx.compose.material3.VerticalDivider
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.intl.LocaleList
import androidx.compose.ui.text.style.LineBreak
import androidx.compose.ui.text.style.TextAlign
import org.json.JSONObject
import java.io.File
import java.util.Locale

// One table entry as edited on screen; value "" = unset, booleans "on"/"off".
class Setting(val e: Entry, initial: String) {
    var initial = initial
    var value by mutableStateOf(initial)

    fun ok(): Boolean = (e.kind as? Kind.IntRange)?.let { intOk(it, value) } ?: true
}

// First found wins, as in lib.rs `android_main`: imported files, bundled assets, the old external folder.
sealed interface GameData {
    data class Imported(val dir: File, val music: Boolean) : GameData
    data object Bundled : GameData
    data class Folder(val dir: File) : GameData
    data object Missing : GameData
}

private fun GameData.ok() = this != GameData.Missing

fun GameData.folder(): File? = (this as? GameData.Imported)?.dir ?: (this as? GameData.Folder)?.dir

interface LauncherActions {
    fun change(r: Setting, v: String)
    fun reset()
    fun recheck()
    fun pick(r: Setting)
    fun play()
    fun packList() {}
    fun packInstall(code: String) {}
    fun packRemove(code: String) {}
    fun saveExport() {}
    fun saveImport(portCfg: Boolean) {}
    fun replaySort(byScore: Boolean) {}
    fun replayImport() {}
    fun replayWatch(r: ReplayItem) {}
    fun replayExport(r: ReplayItem) {}
    fun replayShare(r: ReplayItem) {}
    fun replayDelete(r: ReplayItem) {}
    fun importGame() {}
    fun cancelImport() {}
    fun removeGame() {}
    fun shareError() {}
    fun dismissError() {}
    fun openUrl(url: String) {}
    fun updateStart() {}
    fun updateCancel() {}
    fun updatePermission() {}
    fun updateDismiss() {}
}

// The previous run's last_error.txt (written by the game process), null when there is none.
val LocalLastError = compositionLocalOf<String?> { null }

// A newer release found by the update check (UpdateCheck.kt, null when there is none), the in-app update's step
// (AppUpdate.kt) and whether this build installs it itself.
data class UpdateUi(val info: UpdateInfo?, val step: UpdateStep, val inApp: Boolean) {
    val shown get() = info != null || step !is UpdateStep.Idle
}

val LocalUpdate = compositionLocalOf { UpdateUi(null, UpdateStep.Idle, false) }

// Save export / import state (Saves.kt).
val LocalSaves = compositionLocalOf { SaveState() }

// Translation packs: installed ones, the server list and the running download (Packs.kt).
val LocalPacks = compositionLocalOf { PackState() }

// The resolved launcher language (ja / en / ru) and the app resources in that locale, for the Kotlin-only strings.
val LocalLang = staticCompositionLocalOf { "en" }
val LocalStrings = staticCompositionLocalOf<Resources> { error("no launcher resources") }

// `res` in the locale of `lang`, so values-ja / values-ru follow the setting rather than the device.
fun localized(ctx: Context, lang: String): Resources {
    val conf = Configuration(ctx.resources.configuration).apply { setLocale(Locale.forLanguageTag(lang)) }
    return ctx.createConfigurationContext(conf).resources
}

@Composable
internal fun str(id: Int, vararg args: Any): String = LocalStrings.current.getString(id, *args)

@Composable
internal fun plural(id: Int, n: Int, vararg args: Any): String =
    LocalStrings.current.getQuantityString(id, n, n, *args)

// Group titles by id, for the Developer page's headers.
val LocalGroupTitle = staticCompositionLocalOf<(String, String) -> String> { { id, _ -> id } }

@Composable
private fun Entry.t() = text(LocalLang.current)

@Composable
private fun Entry.show(v: String) = display(v, LocalLang.current, LocalStrings.current)

@Composable
private fun Setting.errorText(): String? = (e.kind as? Kind.IntRange)?.let { intError(it, value, LocalStrings.current) }

private val BLOCK = RoundedCornerShape(16.dp)
private val ROW = RoundedCornerShape(4.dp)

private enum class Dest { Home, Replays, Stats, Settings }

private val Dest.icon: ImageVector
    get() = when (this) {
        Dest.Home -> Icons.Default.Home
        Dest.Settings -> Icons.Default.Settings
        Dest.Replays -> ReplaysIcon
        Dest.Stats -> StatsIcon
    }

private val Dest.label: Int
    get() = when (this) {
        Dest.Home -> R.string.nav_home
        Dest.Settings -> R.string.nav_settings
        Dest.Replays -> R.string.replays_title
        Dest.Stats -> R.string.stats_title
    }

// Every text style carries the launcher language (CJK glyph choice) and, in Japanese, phrase-aware line breaks.
private fun Typography.forLang(lang: String): Typography {
    val locale = LocaleList(lang)
    fun TextStyle.f() = copy(
        localeList = locale,
        lineBreak = if (lang == "ja") LineBreak(LineBreak.Strategy.HighQuality, LineBreak.Strictness.Strict, LineBreak.WordBreak.Phrase) else lineBreak,
    )
    return copy(displayLarge.f(), displayMedium.f(), displaySmall.f(), headlineLarge.f(), headlineMedium.f(), headlineSmall.f(),
        titleLarge.f(), titleMedium.f(), titleSmall.f(), bodyLarge.f(), bodyMedium.f(), bodySmall.f(), labelLarge.f(), labelMedium.f(), labelSmall.f())
}

@Composable
internal fun japaneseWrap(text: String, gameText: Boolean = false): String {
    val lang = LocalLang.current
    return remember(lang, text, gameText) {
        if ((!gameText && lang != "ja") || text.none { it in '\u3040'..'\u9fff' }) return@remember text
        val words = BreakIterator.getWordInstance(ULocale.JAPANESE).apply { setText(text) }
        buildString {
            text.forEachIndexed { i, ch ->
                if (i > 0 && gameText && ch == '「' && !text[i - 1].isWhitespace()) {
                    append('\u200b')
                } else if (i > 0 && ch.isLetter() && text[i - 1].isLetter()) {
                    append(if (words.isBoundary(i)) '\u200b' else '\u2060')
                }
                append(ch)
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LauncherScreen(groups: List<Group>, rows: List<Setting>, data: GameData, act: LauncherActions) {
    val base = MaterialTheme.typography
    val lang = LocalLang.current
    MaterialTheme(typography = remember(lang, base) { base.forLang(lang) }) { Screen(groups, rows, data, act) }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Screen(groups: List<Group>, rows: List<Setting>, data: GameData, act: LauncherActions) {
    val wide = currentWindowAdaptiveInfo().windowSizeClass.windowWidthSizeClass != WindowWidthSizeClass.COMPACT
    var dest by rememberSaveable { mutableStateOf(Dest.Home) }
    var cat by rememberSaveable { mutableStateOf<String?>(null) }
    var blocked by rememberSaveable { mutableStateOf(false) }
    BackHandler(dest != Dest.Home) { if (dest == Dest.Settings && !wide && cat != null) cat = null else dest = Dest.Home }
    var info by rememberSaveable { mutableStateOf<List<String>?>(null) }
    val showInfo = { title: String, text: String -> info = listOf(title, text) }
    val canPlay = data.ok() && rows.all { it.ok() }
    if (canPlay) blocked = false
    val ctx = LocalContext.current
    var devOn by remember { mutableStateOf(ctx.getSharedPreferences(UI_PREFS, 0).getBoolean(DEV_KEY, false)) }
    val toggleDev = {
        devOn = !devOn
        ctx.getSharedPreferences(UI_PREFS, 0).edit().putBoolean(DEV_KEY, devOn).apply()
    }
    val cats = categories(rows, data, devOn)
    val fontScale = LocalDensity.current.fontScale
    val large = fontScale > 1.15f
    val select = { d: Dest -> dest = d; if (d != Dest.Settings) cat = null }
    val openCat = { id: String -> cat = id; dest = Dest.Settings }
    Row(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.surface)) {
        if (wide) {
            val startInset = WindowInsets.safeDrawing.asPaddingValues().calculateStartPadding(LocalLayoutDirection.current)
            NavigationRail(
                modifier = Modifier.width((if (large) 120.dp else 88.dp) + startInset),
                windowInsets = WindowInsets.safeDrawing.only(WindowInsetsSides.Vertical + WindowInsetsSides.Start),
            ) {
                Column(Modifier.fillMaxHeight().verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.Center) {
                    Dest.entries.forEach { d ->
                        val label = str(d.label)
                        NavigationRailItem(selected = dest == d, onClick = { select(d) }, modifier = Modifier.fillMaxWidth(),
                            alwaysShowLabel = !large,
                            icon = { Icon(d.icon, label) },
                            label = { Text(label, maxLines = 1, softWrap = false) })
                    }
                }
            }
        }
        val barSides = WindowInsetsSides.Top + WindowInsetsSides.End + if (wide) WindowInsetsSides.Bottom else WindowInsetsSides.Start
        Scaffold(
            modifier = Modifier.weight(1f),
            containerColor = MaterialTheme.colorScheme.surface,
            contentWindowInsets = WindowInsets.safeDrawing.only(
                WindowInsetsSides.End + WindowInsetsSides.Bottom + if (wide) WindowInsetsSides.Top else WindowInsetsSides.Horizontal + WindowInsetsSides.Top),
            topBar = {
                if (dest != Dest.Home) {
                    val sub = cats.firstOrNull { it.id == cat }.takeIf { dest == Dest.Settings && !wide }
                    TopAppBar(
                        windowInsets = WindowInsets.safeDrawing.only(barSides),
                        navigationIcon = {
                            if (sub != null) IconButton(onClick = { cat = null }) { Icon(Icons.AutoMirrored.Filled.ArrowBack, str(R.string.back)) }
                        },
                        title = { Text(sub?.title ?: str(dest.label), maxLines = 1, overflow = TextOverflow.Ellipsis) },
                        actions = {
                            when (dest) {
                                Dest.Settings -> ResetMenu(act)
                                Dest.Replays -> ReplaysActions(act)
                                else -> {}
                            }
                        },
                    )
                }
            },
            bottomBar = {
                if (!wide) {
                    NavigationBar {
                        Dest.entries.forEach { d ->
                            NavigationBarItem(selected = dest == d, onClick = { select(d) },
                                icon = { Icon(d.icon, null) },
                                label = { Text(str(d.label), maxLines = 1, softWrap = false,
                                    fontSize = if (large) 10.sp else MaterialTheme.typography.labelMedium.fontSize) })
                        }
                    }
                }
            },
        ) { pad ->
            Box(Modifier.fillMaxSize().padding(pad)) {
                when (dest) {
                    Dest.Home -> HomeScreen(data, rows, canPlay, blocked, wide, act, play = { if (canPlay) act.play() else blocked = true },
                        openStats = { select(Dest.Stats) }, openData = { openCat(DATA_CAT) },
                        openBad = { rows.firstOrNull { !it.ok() }?.let { r -> openCat(categoryOf(r, cats)) } })
                    Dest.Settings -> SettingsScreen(cats, cat, wide, data, act, showInfo, toggleDev) { cat = it }
                    Dest.Replays -> ReplaysScreen(rows, act)
                    Dest.Stats -> StatsScreen(rows, wide)
                }
            }
        }
    }
    info?.let { (t, s) -> InfoDialog(t, s) { info = null } }
}

@Composable
private fun ResetMenu(act: LauncherActions) {
    var menu by remember { mutableStateOf(false) }
    var confirm by rememberSaveable { mutableStateOf(false) }
    Box {
        IconButton(onClick = { menu = true }) { Icon(Icons.Default.MoreVert, str(R.string.more)) }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(text = { Text(str(R.string.reset_all)) }, onClick = { menu = false; confirm = true })
        }
    }
    if (confirm) {
        AlertDialog(
            onDismissRequest = { confirm = false },
            title = { Text(str(R.string.reset_all)) },
            text = { Text(str(R.string.reset_confirm)) },
            confirmButton = { TextButton(onClick = { confirm = false; act.reset() }) { Text(str(R.string.ok), maxLines = 1) } },
            dismissButton = { TextButton(onClick = { confirm = false }) { Text(str(R.string.cancel), maxLines = 1) } },
        )
    }
}

private class Summary(val best: Long, val frames: Long, val last: JSONObject?)

// Best score, play time and last run from the statistics bridge; null while there is no run to show.
private fun summaryOf(o: JSONObject): Summary? {
    val st = o.getJSONObject("stats")
    if (st.getInt("runs") == 0) return null
    val pairs = st.getJSONArray("pairs")
    val best = (0 until pairs.length()).maxOfOrNull { pairs.getJSONObject(it).getLong("best") } ?: 0L
    val last = st.getJSONArray("last").let { if (it.length() > 0) it.getJSONObject(0) else null }
    return Summary(best, st.getLong("frames"), last)
}

@Composable
private fun HomeScreen(
    data: GameData,
    rows: List<Setting>,
    canPlay: Boolean,
    blocked: Boolean,
    wide: Boolean,
    act: LauncherActions,
    play: () -> Unit,
    openStats: () -> Unit,
    openData: () -> Unit,
    openBad: () -> Unit,
) {
    val reply by rememberStats()
    val summary = reply?.getOrNull()?.let(::summaryOf).takeIf { data.ok() }
    val importing = LocalImport.current.let { it.busy || it.outcome != null }
    val card = !data.ok() || importing
    val update = LocalUpdate.current.takeIf { it.shown }
    val error = LocalLastError.current
    val side: @Composable () -> Unit = {
        if (summary != null) SummaryCard(summary, rows, openStats)
        if (card) DataCard(data, act)
        if (update != null) Block { UpdateRow(update, act) }
        if (error != null) LastErrorRow(error, act)
        if (!card) DataLine(data, openData)
    }
    val hero: @Composable () -> Unit = { Hero(canPlay, blocked, data.ok(), wide, play, openBad) }
    if (wide && (summary != null || card || update != null || error != null)) {
        Row(Modifier.fillMaxSize().padding(horizontal = 24.dp), horizontalArrangement = Arrangement.spacedBy(32.dp)) {
            HomePane(Modifier.weight(1f), hero)
            HomePane(Modifier.weight(1f), side)
        }
    } else {
        HomePane(Modifier.fillMaxSize().padding(horizontal = 24.dp)) { hero(); side() }
    }
}

// A column centred in its viewport that scrolls once its content is taller.
@Composable
private fun HomePane(modifier: Modifier, content: @Composable () -> Unit) {
    BoxWithConstraints(modifier) {
        Column(
            Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).heightIn(min = maxHeight).padding(vertical = 16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) { content() }
    }
}

@Composable
private fun Hero(canPlay: Boolean, blocked: Boolean, dataOk: Boolean, wide: Boolean, play: () -> Unit, openBad: () -> Unit) {
    val c = MaterialTheme.colorScheme
    Box(Modifier.size(if (wide) 72.dp else 96.dp).clip(RoundedCornerShape(24.dp))
        .background(Brush.verticalGradient(listOf(Color(0xFF0D1133), Color(0xFF28195B))))) {
        Image(painterResource(R.drawable.ic_launcher_foreground), null, Modifier.fillMaxSize())
    }
    Text(str(R.string.title), style = MaterialTheme.typography.headlineLarge, textAlign = TextAlign.Center)
    Text(str(R.string.title_ja), style = MaterialTheme.typography.titleMedium.copy(localeList = LocaleList("ja")),
        color = c.onSurfaceVariant, textAlign = TextAlign.Center)
    Spacer(Modifier.height(4.dp))
    val label = str(R.string.play)
    Button(
        onClick = play,
        modifier = Modifier.widthIn(max = 480.dp).fillMaxWidth().heightIn(min = 64.dp).semantics { contentDescription = label },
        colors = if (canPlay) ButtonDefaults.buttonColors() else
            ButtonDefaults.buttonColors(containerColor = c.surfaceContainerHighest, contentColor = c.onSurface),
    ) {
        Icon(Icons.Default.PlayArrow, null, Modifier.size(28.dp))
        Spacer(Modifier.width(8.dp))
        Text(label, style = MaterialTheme.typography.titleLarge, maxLines = 1, softWrap = false)
    }
    if (blocked) {
        Text(str(if (dataOk) R.string.play_fix_settings else R.string.play_need_data), color = c.error, textAlign = TextAlign.Center,
            style = MaterialTheme.typography.bodyMedium)
        if (dataOk) TextButton(onClick = openBad) { Text(str(R.string.nav_settings), maxLines = 1) }
    }
}

@Composable
private fun SummaryCard(s: Summary, rows: List<Setting>, open: () -> Unit) {
    val lang = LocalLang.current
    val res = LocalStrings.current
    fun word(key: String, v: String) = rows.firstOrNull { it.e.key == key }?.e?.display(v, lang, res) ?: v
    val c = MaterialTheme.colorScheme
    Column(
        Modifier.widthIn(max = 480.dp).fillMaxWidth().clip(BLOCK).background(c.surfaceContainer).clickable(onClick = open).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        SummaryLine(str(R.string.home_best), digits(s.best))
        SummaryLine(str(R.string.home_time), playTime(s.frames))
        s.last?.let { r ->
            HorizontalDivider()
            SummaryLine(str(R.string.home_last), digits(r.getLong("score")))
            Text("${word("player", r.getString("shot_key"))} · ${word("rank", r.getString("rank_key"))} · ${word("stage", r.getString("stage_key"))}",
                style = MaterialTheme.typography.bodyMedium, color = c.onSurfaceVariant)
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun SummaryLine(label: String, value: String) {
    FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
        Text(label, Modifier.padding(end = 12.dp), style = MaterialTheme.typography.bodyLarge)
        Text(value, style = MaterialTheme.typography.titleMedium.copy(fontFeatureSettings = "tnum"), maxLines = 1, softWrap = false)
    }
}

@Composable
private fun dataSummary(d: GameData) = when (d) {
    is GameData.Imported if d.music -> str(R.string.data_imported)
    is GameData.Imported -> str(R.string.data_imported_no_music)
    GameData.Bundled -> str(R.string.data_bundled)
    is GameData.Folder -> str(R.string.data_found)
    GameData.Missing -> str(R.string.data_missing)
}

// The quiet Home line while the game data is fine.
@Composable
private fun DataLine(d: GameData, open: () -> Unit) {
    val c = MaterialTheme.colorScheme
    Row(
        Modifier.widthIn(max = 480.dp).clip(BLOCK).clickable(onClick = open).heightIn(min = 48.dp).padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Default.CheckCircle, null, Modifier.size(16.dp), tint = c.onSurfaceVariant)
        Spacer(Modifier.width(8.dp))
        Text(str(R.string.game_data) + ": " + dataSummary(d), style = MaterialTheme.typography.bodyMedium, color = c.onSurfaceVariant)
    }
}

// Home card while the game files must be imported, or an import runs or just ended.
@Composable
private fun DataCard(d: GameData, act: LauncherActions) {
    val c = MaterialTheme.colorScheme
    val st = LocalImport.current
    Column(
        Modifier.widthIn(max = 480.dp).fillMaxWidth().clip(BLOCK).background(c.surfaceContainer).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        if (st.busy) {
            ImportRows(d, act)
        } else {
            if (!d.ok()) Text(str(R.string.data_need), style = MaterialTheme.typography.bodyMedium, textAlign = TextAlign.Center)
            ImportOutcomeText()
            if (!d.ok()) {
                Button(onClick = act::importGame, Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text(str(R.string.import_files), maxLines = 1, softWrap = false) }
                TextButton(onClick = act::recheck) { Text(str(R.string.check_again), maxLines = 1, softWrap = false) }
            }
        }
    }
}

internal const val DATA_CAT = SEC_APP

private class Category(val section: Section, val title: String, val rows: List<Setting>, val state: String) {
    val id get() = section.id
}

private const val UI_PREFS = "launcher_ui"
private const val DEV_KEY = "developer_section"

// The sections of Sections.kt; Developer is listed once the version line has been tapped.
@Composable
private fun categories(rows: List<Setting>, data: GameData, devOn: Boolean): List<Category> {
    val lang = LocalLang.current
    val res = LocalStrings.current
    val byKey = rows.associateBy { it.e.key }
    return SECTIONS.filter { it.id != SEC_DEV || devOn }.map { sec ->
        val rs = sec.keys.mapNotNull { byKey[it] }
        val state = when {
            sec.id == SEC_APP -> dataSummary(data)
            sec.state.isNotEmpty() -> sec.state.mapNotNull { byKey[it] }.joinToString(" · ") { r ->
                val v = r.e.display(r.value, lang, res)
                if (sec.labelled) r.e.text(lang).label + " " + v else v
            }
            else -> rs.count { it.value != it.e.default }.let { n -> if (n > 0) plural(R.plurals.changed_count, n) else str(R.string.section_defaults) }
        }
        Category(sec, str(sec.title), rs, state)
    }
}

private fun categoryOf(r: Setting, cats: List<Category>) = cats.firstOrNull { r in it.rows }?.id ?: DATA_CAT

// Portrait: the section list, or one pushed page. Landscape: sections about a third, one detail column up to 600 dp.
@Composable
private fun SettingsScreen(
    cats: List<Category>,
    cat: String?,
    wide: Boolean,
    data: GameData,
    act: LauncherActions,
    showInfo: (String, String) -> Unit,
    toggleDev: () -> Unit,
    select: (String?) -> Unit,
) {
    val shown = cats.firstOrNull { it.id == cat } ?: if (wide) cats.first() else null
    if (wide) {
        Row(Modifier.fillMaxSize()) {
            CategoryList(cats, shown?.id, chevron = false, Modifier.weight(1f).fillMaxHeight(), select)
            VerticalDivider()
            Box(Modifier.weight(2f).fillMaxHeight(), contentAlignment = Alignment.TopCenter) {
                shown?.let { key(it.id) { CategoryPage(it, data, act, showInfo, toggleDev, Modifier.widthIn(max = 600.dp).fillMaxSize()) } }
            }
        }
    } else if (shown == null) {
        CategoryList(cats, null, chevron = true, Modifier.fillMaxSize(), select)
    } else {
        CategoryPage(shown, data, act, showInfo, toggleDev, Modifier.fillMaxSize())
    }
}

@Composable
private fun CategoryList(cats: List<Category>, selected: String?, chevron: Boolean, modifier: Modifier, select: (String) -> Unit) {
    val c = MaterialTheme.colorScheme
    LazyColumn(modifier, contentPadding = PaddingValues(12.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        items(cats, key = { it.id }) { cat ->
            val on = cat.id == selected
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(28.dp)).background(if (on) c.secondaryContainer else Color.Transparent)
                    .clickable { select(cat.id) }.heightIn(min = 64.dp).padding(horizontal = 16.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(Modifier.weight(1f)) {
                    Text(cat.title, style = MaterialTheme.typography.bodyLarge, color = if (on) c.onSecondaryContainer else c.onSurface)
                    Text(japaneseWrap(cat.state), style = MaterialTheme.typography.bodyMedium, color = if (on) c.onSecondaryContainer else c.onSurfaceVariant)
                }
                if (chevron) Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, tint = c.onSurfaceVariant)
            }
        }
    }
}

@Composable
private fun CategoryPage(cat: Category, data: GameData, act: LauncherActions, showInfo: (String, String) -> Unit, toggleDev: () -> Unit, modifier: Modifier) {
    val lang = LocalLang.current
    LazyColumn(modifier, contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (cat.id == SEC_DEV) {
            item("dev_note") {
                Text(str(R.string.developer_summary), style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(horizontal = 16.dp))
            }
            cat.rows.groupBy { it.e.group }.forEach { (_, rs) ->
                item(rs.first().e.key) {
                    Column {
                        Header(LocalGroupTitle.current(rs.first().e.group, lang))
                        Block { rs.forEach { SettingRow(it, act, showInfo) } }
                    }
                }
            }
            return@LazyColumn
        }
        if (cat.id == SEC_APP) item("status") { Block { StatusRow(data, act, showInfo) } }
        if (cat.rows.isNotEmpty()) item("rows") { Block { cat.rows.forEach { SettingRow(it, act, showInfo) } } }
        if (cat.id == SEC_LANGUAGE) item("tr") { TranslationsBlock(act) }
        if (cat.id == SEC_APP) {
            item("saves") {
                Column {
                    Header(str(R.string.saves_title))
                    SavesBlock(act)
                }
            }
            item("version") {
                Block {
                    ShellRow(str(R.string.version), "${BuildConfig.VERSION_NAME} (${BuildConfig.CHANNEL})", modifier = Modifier.clickable(onClick = toggleDev))
                }
            }
        }
    }
}

@Composable
private fun Header(text: String) {
    Text(text, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.padding(start = 16.dp, top = 16.dp, bottom = 8.dp))
}

// Segmented block: rows 2dp apart on surfaceContainer, the block's outer corners rounded.
@Composable
private fun Block(content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth().clip(BLOCK), verticalArrangement = Arrangement.spacedBy(2.dp)) { content() }
}

@Composable
internal fun ShellRow(
    title: String,
    summary: String?,
    modifier: Modifier = Modifier,
    info: (() -> Unit)? = null,
    below: (@Composable () -> Unit)? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    Column(
        modifier.fillMaxWidth().clip(ROW).background(MaterialTheme.colorScheme.surfaceContainer)
            .heightIn(min = 64.dp).padding(start = 16.dp, end = 16.dp, top = 10.dp, bottom = 10.dp),
        verticalArrangement = Arrangement.Center,
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(title, style = MaterialTheme.typography.bodyLarge)
                if (!summary.isNullOrEmpty()) {
                    Text(japaneseWrap(summary), style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            if (info != null) {
                IconButton(onClick = info) {
                    Icon(Icons.Outlined.Info, str(R.string.about_setting), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            if (trailing != null) {
                Spacer(Modifier.width(8.dp))
                trailing()
            }
        }
        if (below != null) {
            Spacer(Modifier.size(8.dp))
            below()
        }
    }
}

// The failure line of a report: the block after the header, without its "panic:" / "start-up failure:" line.
private fun headline(report: String): String {
    val body = report.substringAfter("\n\n", report).substringAfter(":\n")
    return listOf("\nbacktrace:\n", "\nlast warnings:\n").map { body.indexOf(it) }.filter { it >= 0 }
        .minOrNull().let { if (it == null) body else body.substring(0, it) }.trim()
}
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun UpdateRow(u: UpdateUi, act: LauncherActions) {
    val info = u.info
    val step = u.step
    val fg = MaterialTheme.colorScheme.onSecondaryContainer
    Column(
        Modifier.fillMaxWidth().clip(ROW).background(MaterialTheme.colorScheme.secondaryContainer)
            .padding(start = 16.dp, end = 8.dp, top = 10.dp, bottom = 4.dp),
    ) {
        val line = when (step) {
            is UpdateStep.Downloading -> str(R.string.update_downloading, info?.version ?: "",
                if (step.total > 0) "${step.done * 100 / step.total}%" else "${step.done shr 20} MiB")
            UpdateStep.Verifying -> str(R.string.update_verifying)
            UpdateStep.NeedPermission -> str(R.string.update_permission)
            UpdateStep.Installing -> str(R.string.update_installing)
            is UpdateStep.Failed -> str(step.msg, step.detail)
            is UpdateStep.Updated -> str(R.string.update_done, step.version)
            UpdateStep.Idle -> str(R.string.update_available, info?.version ?: "")
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(if (step is UpdateStep.Failed) Icons.Default.Warning else Icons.Default.Info, null, tint = fg)
            Spacer(Modifier.width(16.dp))
            Text(line, style = MaterialTheme.typography.bodyLarge, color = fg)
        }
        when (step) {
            is UpdateStep.Downloading -> if (step.total > 0) {
                LinearProgressIndicator(progress = { step.done.toFloat() / step.total }, Modifier.fillMaxWidth().padding(top = 8.dp))
            } else {
                LinearProgressIndicator(Modifier.fillMaxWidth().padding(top = 8.dp))
            }
            UpdateStep.Verifying, UpdateStep.Installing -> LinearProgressIndicator(Modifier.fillMaxWidth().padding(top = 8.dp))
            else -> {}
        }
        FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
            when (step) {
                is UpdateStep.Downloading, UpdateStep.Verifying ->
                    TextButton(onClick = act::updateCancel) { Text(str(R.string.update_cancel), maxLines = 1) }
                UpdateStep.NeedPermission -> {
                    TextButton(onClick = act::updateCancel) { Text(str(R.string.update_cancel), maxLines = 1) }
                    TextButton(onClick = act::updatePermission) { Text(str(R.string.update_permission_open), maxLines = 1) }
                }
                UpdateStep.Installing -> {}
                is UpdateStep.Updated -> TextButton(onClick = act::updateDismiss) { Text(str(R.string.update_dismiss), maxLines = 1) }
                UpdateStep.Idle, is UpdateStep.Failed -> if (info != null) {
                    TextButton(onClick = { act.openUrl(info.page) }) { Text(str(R.string.update_notes), maxLines = 1) }
                    val inApp = u.inApp && info.asset != null && info.sums != null
                    // A failed in-app update keeps the browser download as fallback.
                    if (!inApp || step is UpdateStep.Failed) {
                        TextButton(onClick = { act.openUrl(info.download) }) { Text(str(R.string.update_download), maxLines = 1) }
                    }
                    if (inApp) {
                        val label = if (step is UpdateStep.Failed) R.string.update_retry else R.string.update_install
                        TextButton(onClick = act::updateStart) { Text(str(label), maxLines = 1) }
                    }
                } else {
                    TextButton(onClick = act::updateDismiss) { Text(str(R.string.update_dismiss), maxLines = 1) }
                }
            }
        }
    }
}

// One plain sentence with Details and Dismiss; the raw report only behind Details.
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun LastErrorRow(report: String, act: LauncherActions) {
    val c = MaterialTheme.colorScheme
    var details by rememberSaveable { mutableStateOf(false) }
    Column(
        Modifier.fillMaxWidth().widthIn(max = 480.dp).clip(BLOCK).background(c.errorContainer)
            .padding(start = 16.dp, end = 8.dp, top = 12.dp, bottom = 4.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Default.Warning, null, tint = c.onErrorContainer)
            Spacer(Modifier.width(16.dp))
            Text(str(R.string.last_failed), style = MaterialTheme.typography.bodyLarge, color = c.onErrorContainer)
        }
        FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
            TextButton(onClick = { details = true }) { Text(str(R.string.last_failed_details), maxLines = 1, softWrap = false) }
            TextButton(onClick = act::dismissError) { Text(str(R.string.last_failed_dismiss), maxLines = 1, softWrap = false) }
        }
    }
    if (details) {
        AlertDialog(
            onDismissRequest = { details = false },
            title = { Text(str(R.string.last_failed)) },
            text = { Text(headline(report), Modifier.verticalScroll(rememberScrollState())) },
            confirmButton = { TextButton(onClick = { details = false; act.shareError() }) { Text(str(R.string.last_failed_share), maxLines = 1) } },
            dismissButton = { TextButton(onClick = { details = false }) { Text(str(R.string.ok), maxLines = 1) } },
        )
    }
}

// The Game data page's status: what is in use, with the import controls under it.
@Composable
private fun StatusRow(d: GameData, act: LauncherActions, showInfo: (String, String) -> Unit) {
    val ok = d.ok()
    val detail = when (d) {
        is GameData.Imported -> str(R.string.data_imported_detail, d.dir.path)
        GameData.Bundled -> str(R.string.data_bundled_detail)
        is GameData.Folder -> str(R.string.data_found_detail, d.dir.path)
        GameData.Missing -> str(R.string.data_missing_detail)
    }
    val title = str(R.string.game_data)
    Row(
        Modifier.fillMaxWidth().clip(ROW).background(MaterialTheme.colorScheme.surfaceContainer)
            .clickable { showInfo(title, detail) }.heightIn(min = 64.dp).padding(start = 16.dp, end = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(if (ok) Icons.Default.CheckCircle else Icons.Default.Warning, null,
            tint = if (ok) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.error)
        Spacer(Modifier.width(16.dp))
        Text(dataSummary(d), Modifier.weight(1f).padding(vertical = 10.dp), style = MaterialTheme.typography.bodyLarge,
            color = if (ok) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.error)
        if (!ok) TextButton(onClick = act::recheck) { Text(str(R.string.check_again), maxLines = 1, softWrap = false) }
    }
    if (d !is GameData.Bundled) ImportRows(d, act)
}

// A summary that only repeats the label ("Music volume" / "Volume of the music") is not shown.
private fun usefulSummary(label: String, summary: String): String? {
    val stems = { s: String -> s.lowercase().split(Regex("[^\\p{L}\\p{N}]+")).filter { it.length > 2 }.map { it.take(5) } }
    val l = stems(label)
    val extra = stems(summary).filter { it !in l && it !in setOf("the", "all", "for", "and") }
    return summary.takeUnless { l.isNotEmpty() && l.all { it in stems(summary) } && extra.size <= 1 }
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
private fun SettingRow(r: Setting, act: LauncherActions, showInfo: (String, String) -> Unit) {
    val e = r.e
    val t = e.t()
    val details = str(R.string.details, t.summary, t.help)
    val info = { showInfo(t.label, details) }.takeIf { t.help.length > t.summary.length + 24 }
    val summary = usefulSummary(t.label, t.summary)
    var editing by rememberSaveable(e.key) { mutableStateOf(false) }
    // `game_text`: follow / ja, then the installed pack languages, then the current value if no pack of it is installed.
    val packs = if (e.key == "game_text") {
        val inst = LocalPacks.current.installed.filter { it.language != "ja" }.distinctBy { it.language }
            .associate { it.language to if (it.title.isEmpty()) it.language else "${it.language} — ${it.title}" }
        val missing = r.value.takeIf { it !in inst && it !in listOf("", "follow", "ja") }
        if (missing == null) inst else inst + (missing to str(R.string.tr_missing, missing))
    } else emptyMap()
    val opts = e.options()?.let { it + packs.keys.filter { k -> k !in it } }
    val shown: @Composable (String) -> String = { v -> packs[v] ?: e.show(v) }
    val k = e.kind
    when {
        k == Kind.Bool -> ShellRow(
            t.label, summary, info = info,
            modifier = Modifier.toggleable(r.value == "on", role = Role.Switch) { act.change(r, if (it) "on" else "off") },
        ) { Switch(checked = r.value == "on", onCheckedChange = null) }
        opts != null -> {
            val labels = opts.map { shown(it) }
            BoxWithConstraints {
                // Segments only while every label fits on one line; otherwise a list dialog, so no label is cut.
                val measurer = rememberTextMeasurer()
                val style = MaterialTheme.typography.labelLarge
                val density = LocalDensity.current
                val perSegment = (maxWidth - 32.dp) / opts.size - 24.dp
                val fits = opts.size <= 3 && e.key != "game_text" && labels.all { l ->
                    with(density) { measurer.measure(l, style).size.width.toDp() } <= perSegment
                }
                if (fits) {
                    ShellRow(t.label, summary, info = info, below = {
                        SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                            opts.forEachIndexed { i, v ->
                                SegmentedButton(
                                    selected = r.value == v,
                                    onClick = { act.change(r, v) },
                                    modifier = Modifier.testTag("setting_${e.key}_$v").semantics { testTagsAsResourceId = true },
                                    shape = SegmentedButtonDefaults.itemShape(i, opts.size),
                                    icon = {},
                                    colors = SegmentedButtonDefaults.colors(inactiveContainerColor = Color.Transparent),
                                ) { Text(labels[i], maxLines = 1, softWrap = false) }
                            }
                        }
                    })
                } else {
                    ShellRow(t.label, summary, info = info, modifier = Modifier.clickable { editing = true },
                        below = { ValueField(shown(r.value), r.ok()) })
                    if (editing) ChoiceDialog(t.label, opts, labels, r.value, { act.change(r, it) }) { editing = false }
                }
            }
        }
        k is Kind.IntRange && k.max - k.min <= 400 -> {
            val current = (r.value.toLongOrNull() ?: r.e.default.toLongOrNull() ?: k.min).coerceIn(k.min, k.max)
            var v by remember(r.value) { mutableStateOf(current.toFloat()) }
            val n = Math.round(v).toLong()
            ShellRow(t.label, summary, info = info, modifier = Modifier.clickable { editing = true }, below = {
                Slider(
                    modifier = Modifier.testTag(e.key).semantics {
                        contentDescription = t.label
                        testTagsAsResourceId = true
                    },
                    value = v,
                    onValueChange = { v = it },
                    onValueChangeFinished = { act.change(r, Math.round(v).toLong().toString()) },
                    valueRange = k.min.toFloat()..k.max.toFloat(),
                    steps = if (k.max - k.min <= 20) (k.max - k.min - 1).toInt().coerceAtLeast(0) else 0,
                )
            }) {
                Text(if (r.value.isEmpty() && n == current) e.show("") else e.show(n.toString()),
                    style = MaterialTheme.typography.labelLarge, maxLines = 1, softWrap = false,
                    color = if (!r.ok()) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary)
            }
            if (editing) TextDialog(r, act) { editing = false }
        }
        else -> {
            ShellRow(t.label, summary, info = info, modifier = Modifier.clickable { editing = true },
                below = { ValueField(shown(r.value), r.ok()) })
            if (editing) TextDialog(r, act) { editing = false }
        }
    }
}

// One radio row per value; choosing applies and closes.
@Composable
private fun ChoiceDialog(title: String, values: List<String>, labels: List<String>, current: String, pick: (String) -> Unit, close: () -> Unit) {
    AlertDialog(
        onDismissRequest = close,
        title = { Text(title) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                values.forEachIndexed { i, v ->
                    Row(
                        Modifier.fillMaxWidth().selectable(v == current, role = Role.RadioButton) { pick(v); close() }.heightIn(min = 48.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        RadioButton(selected = v == current, onClick = null)
                        Spacer(Modifier.width(16.dp))
                        Text(japaneseWrap(labels[i]), style = MaterialTheme.typography.bodyLarge, modifier = Modifier.padding(vertical = 8.dp))
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = close) { Text(str(R.string.cancel), maxLines = 1) } },
    )
}

// Read-only look of a field under a row: the whole row is the tap target, this only shows the value.
@Composable
private fun ValueField(text: String, ok: Boolean) {
    val c = MaterialTheme.colorScheme
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp)
            .border(BorderStroke(1.dp, if (ok) c.outline else c.error), RoundedCornerShape(8.dp))
            .padding(start = 16.dp, end = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text, Modifier.weight(1f).padding(vertical = 8.dp), style = MaterialTheme.typography.bodyLarge,
            color = if (ok) c.onSurface else c.error)
        Icon(Icons.Default.ArrowDropDown, null, tint = c.onSurfaceVariant)
    }
}

// Installed translations (update / remove), the server's languages in a dialog, the running download, attribution.
@Composable
private fun TranslationsBlock(act: LauncherActions) {
    val st = LocalPacks.current
    var list by rememberSaveable { mutableStateOf(false) }
    val busy = st.busy != null
    Block {
        ShellRow(str(R.string.tr_title), if (st.installed.isEmpty()) str(R.string.tr_none) else null)
        st.installed.forEach { p ->
            val cov = p.coverage?.let { str(R.string.tr_translated, String.format(Locale.ROOT, "%.1f", it)) }
            ShellRow(if (p.title.isEmpty()) p.language else "${p.language} — ${p.title}", cov ?: "") {
                if (p.managed) {
                    val server = st.server?.firstOrNull { it.code == p.language }
                    if (server != null && server.base.isEmpty() && server.revision == p.revision) {
                        Text(str(R.string.tr_up_to_date), style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant)
                    } else {
                        TextButton(onClick = { act.packInstall(p.language) }, enabled = !busy) { Text(str(R.string.tr_update)) }
                    }
                    TextButton(onClick = { act.packRemove(p.language) }, enabled = !busy) { Text(str(R.string.tr_remove)) }
                }
            }
        }
        ShellRow(str(R.string.tr_server), str(R.string.tr_server_summary), modifier = Modifier.clickable(enabled = !busy) {
            if (st.server == null) act.packList()
            list = true
        })
        if (busy || st.outcome != null) {
            val title = when {
                st.busy == "" -> str(R.string.tr_loading)
                st.busy != null && st.building -> str(R.string.tr_building, st.busy)
                st.busy != null -> str(R.string.tr_working, st.busy, st.done, st.total)
                else -> when (val o = st.outcome) {
                    is PackOutcome.Installed -> str(R.string.tr_done, o.lang, String.format(Locale.ROOT, "%.1f", o.percent))
                    is PackOutcome.Removed -> str(R.string.tr_removed, o.lang)
                    is PackOutcome.Failed -> str(R.string.tr_failed, o.lang, o.error)
                    null -> ""
                }
            }
            if (busy && st.busy != "") {
                ShellRow(title, st.line, below = {
                    if (st.total > 0 && !st.building) {
                        LinearProgressIndicator(progress = { st.done.toFloat() / st.total }, Modifier.fillMaxWidth())
                    } else {
                        LinearProgressIndicator(Modifier.fillMaxWidth())
                    }
                })
            } else {
                ShellRow(title, null)
            }
        }
        Text(str(R.string.tr_attribution), style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.fillMaxWidth().clip(ROW).background(MaterialTheme.colorScheme.surfaceContainer)
                .padding(16.dp))
    }
    if (list) ServerDialog(st, act) { list = false }
}

// Export / import of scores, replays and settings as one zip, through the system file picker.
@Composable
private fun SavesBlock(act: LauncherActions) {
    val st = LocalSaves.current
    var portCfg by rememberSaveable { mutableStateOf(false) }
    val enabled = !st.busy && !st.gameRunning
    Block {
        ShellRow(str(R.string.saves_export), str(R.string.saves_export_summary),
            modifier = Modifier.clickable(enabled = enabled) { act.saveExport() })
        ShellRow(str(R.string.saves_import), str(R.string.saves_import_summary),
            modifier = Modifier.clickable(enabled = enabled) { act.saveImport(portCfg) })
        ShellRow(str(R.string.saves_port_cfg), str(R.string.saves_port_cfg_summary),
            modifier = Modifier.toggleable(portCfg, enabled = enabled, role = Role.Switch) { portCfg = it },
        ) { Switch(checked = portCfg, onCheckedChange = null, enabled = enabled) }
        val msg = when {
            st.gameRunning -> str(R.string.saves_game_running)
            st.busy -> null
            else -> when (val o = st.outcome) {
                is SaveOutcome.Exported -> plural(R.plurals.saves_exported, o.files)
                is SaveOutcome.Imported -> plural(if (o.backup != null) R.plurals.saves_imported_backup else R.plurals.saves_imported, o.files)
                is SaveOutcome.Failed -> str(if (o.export) R.string.saves_export_failed else R.string.saves_import_failed, o.error)
                null -> null
            }
        }
        if (st.busy) ShellRow(str(R.string.saves_working), null, below = { LinearProgressIndicator(Modifier.fillMaxWidth()) })
        // The block sits below the fold: an export or import result also shows as a toast.
        val ctx = LocalContext.current
        LaunchedEffect(st.outcome) {
            if (st.outcome != null && msg != null) android.widget.Toast.makeText(ctx, msg, android.widget.Toast.LENGTH_LONG).show()
        }
        if (msg != null) {
            Text(msg, style = MaterialTheme.typography.bodyMedium,
                color = if (st.outcome is SaveOutcome.Failed || st.gameRunning) MaterialTheme.colorScheme.error
                else MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.fillMaxWidth().clip(ROW).background(MaterialTheme.colorScheme.surfaceContainer)
                    .padding(16.dp))
        }
    }
}

@Composable
private fun ServerDialog(st: PackState, act: LauncherActions, close: () -> Unit) {
    var filter by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = close,
        title = { Text(str(R.string.tr_server)) },
        text = {
            Column {
                val server = st.server
                when {
                    server == null && st.busy == "" -> LinearProgressIndicator(Modifier.fillMaxWidth())
                    server == null -> Text((st.outcome as? PackOutcome.Failed)?.error ?: "")
                    else -> {
                        OutlinedTextField(filter, { filter = it }, singleLine = true,
                            placeholder = { Text(str(R.string.tr_filter)) }, modifier = Modifier.fillMaxWidth())
                        Spacer(Modifier.size(8.dp))
                        val f = filter.lowercase()
                        // Keyed items keep their scroll position; a new filter starts at the best match.
                        val listState = rememberLazyListState()
                        LaunchedEffect(f) { listState.scrollToItem(0) }
                        LazyColumn(Modifier.heightIn(max = 420.dp), state = listState) {
                            server.filter { f.isEmpty() || it.code.contains(f) || it.title.lowercase().contains(f) }
                                .sortedBy { if (it.code == f) 0 else if (it.code.startsWith(f)) 1 else 2 }
                                .forEach { p ->
                                    item(key = p.code) {
                                        val have = st.installed.any { it.managed && it.language == p.code }
                                        Row(Modifier.fillMaxWidth().padding(vertical = 4.dp),
                                            verticalAlignment = Alignment.CenterVertically) {
                                            Column(Modifier.weight(1f)) {
                                                Text("${p.code} — ${p.title}", style = MaterialTheme.typography.bodyLarge)
                                                val est = str(R.string.tr_estimate, String.format(Locale.ROOT, "%.0f", p.estimate))
                                                val base = if (p.base.isEmpty()) "" else "; " + str(R.string.tr_on_top, p.base.joinToString(", "))
                                                Text(est + base, style = MaterialTheme.typography.bodySmall,
                                                    color = MaterialTheme.colorScheme.onSurfaceVariant)
                                            }
                                            OutlinedButton(onClick = { act.packInstall(p.code); close() }, enabled = st.busy == null) {
                                                Text(str(if (have) R.string.tr_update else R.string.tr_install))
                                            }
                                        }
                                    }
                                }
                        }
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = close) { Text(str(R.string.cancel)) } },
    )
}

@Composable
private fun InfoDialog(title: String, text: String, close: () -> Unit) {
    AlertDialog(
        onDismissRequest = close,
        icon = { Icon(Icons.Outlined.Info, null) },
        title = { Text(title) },
        text = { Text(japaneseWrap(text), Modifier.verticalScroll(rememberScrollState())) },
        confirmButton = { TextButton(onClick = close) { Text(str(R.string.ok)) } },
    )
}


// Number or text entry; the replay row also offers a file chooser.
@Composable
private fun TextDialog(r: Setting, act: LauncherActions, close: () -> Unit) {
    var t by remember { mutableStateOf(r.value) }
    val k = r.e.kind
    val err = (k as? Kind.IntRange)?.let { intError(it, t.trim(), LocalStrings.current) }
    val hint = when (k) {
        is Kind.IntRange -> "${k.min}–${k.max}"
        is Kind.Text -> k.format
        else -> ""
    }
    AlertDialog(
        onDismissRequest = close,
        title = { Text(r.e.t().label) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                if (k is Kind.IntRange && k.max - k.min <= 400) {
                    Slider(
                        value = (t.toFloatOrNull() ?: r.e.default.toFloatOrNull() ?: k.min.toFloat()).coerceIn(k.min.toFloat(), k.max.toFloat()),
                        onValueChange = { t = Math.round(it).toString() },
                        valueRange = k.min.toFloat()..k.max.toFloat(),
                    )
                }
                OutlinedTextField(
                    value = t,
                    onValueChange = { t = it },
                    singleLine = true,
                    placeholder = { Text(r.e.show(""), fontFamily = FontFamily.Default) },
                    isError = err != null,
                    supportingText = { Text(err ?: str(if (k is Kind.IntRange) R.string.range else R.string.format, hint)) },
                    keyboardOptions = KeyboardOptions(
                        keyboardType = if (k is Kind.IntRange) KeyboardType.Number else KeyboardType.Text,
                    ),
                    modifier = Modifier.fillMaxWidth(),
                )
                val reset = r.e.default.isEmpty() || t != r.e.default
                if (reset) {
                    TextButton(onClick = { act.change(r, r.e.default); close() }) {
                        Text(str(if (r.e.default.isEmpty()) R.string.clear else R.string.default_value), maxLines = 1)
                    }
                }
                if (r.e.key == "replay") {
                    HorizontalDivider(Modifier.padding(vertical = 8.dp))
                    (0..3).forEach { i ->
                        TextButton(onClick = { act.change(r, "demo$i"); close() }) { Text(str(R.string.demo, i + 1)) }
                    }
                    TextButton(onClick = { close(); act.pick(r) }) { Text(str(R.string.choose_replay)) }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = { act.change(r, t.trim()); close() }, enabled = err == null) { Text(str(R.string.ok), maxLines = 1) }
        },
        dismissButton = { TextButton(onClick = close) { Text(str(R.string.cancel), maxLines = 1) } },
    )
}
// Previews with a few table-like rows (no Context: the schema asset is not read).
private object PreviewActions : LauncherActions {
    override fun change(r: Setting, v: String) { r.value = v }
    override fun reset() {}
    override fun recheck() {}
    override fun pick(r: Setting) {}
    override fun play() {}
}

private fun previewRows(): Pair<List<Group>, List<Setting>> {
    fun e(key: String, label: String, summary: String, kind: Kind, default: String, group: String, dev: Boolean = false,
          unit: String = "", words: Map<String, String> = emptyMap()) =
        Entry(key, mapOf("en" to EntryText(label, summary, "$summary.", unit, words)), kind, default, group, dev, true, !dev)
    fun g(id: String, title: String) = Group(id, mapOf("en" to title))
    val groups = listOf(g("display", "Display"), g("touch", "Touch controls"), g("start_game", "Start game"),
        g("cheats", "Cheats/debug"))
    val es = listOf(
        e("vsync", "VSync", "Sync frames to the display", Kind.Bool, "on", "display"),
        e("screen_margin", "Screen margin (dp)", "Extra space kept clear at the screen edges", Kind.IntRange(0, 200),
            "0", "display", unit = " dp", words = mapOf("0" to "Off")),
        e("touch", "Touch overlay", "On-screen buttons for touch screens", Kind.Choice(listOf("auto", "on", "off")),
            "auto", "touch", words = mapOf("auto" to "Auto", "on" to "On", "off" to "Off")),
        e("touch_sens", "Drag sensitivity %", "How far the player moves per finger move", Kind.IntRange(25, 400),
            "100", "touch", unit = "%"),
        e("rank", "Difficulty", "Difficulty for a direct stage or ending start",
            Kind.Choice(listOf("easy", "normal", "hard", "lunatic", "extra")), "", "start_game",
            words = mapOf("" to "Default", "easy" to "Easy", "normal" to "Normal", "hard" to "Hard",
                "lunatic" to "Lunatic", "extra" to "Extra")),
        e("invuln", "Invulnerable", "The player can't die", Kind.Bool, "off", "cheats", dev = true),
    )
    return groups to es.map { Setting(it, it.default) }
}

@Composable
private fun PreviewScreen(dark: Boolean) {
    val (g, rs) = remember { previewRows() }
    val ctx = LocalContext.current
    Th08Theme(dark) {
        CompositionLocalProvider(LocalStrings provides ctx.resources) {
            LauncherScreen(g, rs, GameData.Bundled, PreviewActions)
        }
    }
}

@Preview(name = "Light", widthDp = 412, heightDp = 915)
@Composable
private fun LightPreview() = PreviewScreen(false)

@Preview(name = "Dark", widthDp = 412, heightDp = 915)
@Composable
private fun DarkPreview() = PreviewScreen(true)

@Preview(name = "Landscape", widthDp = 915, heightDp = 412)
@Composable
private fun LandscapePreview() = PreviewScreen(false)
