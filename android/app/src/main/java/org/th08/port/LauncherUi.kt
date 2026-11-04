// Launcher screen (MATERIAL2), Geode-launcher style: section headers over segmented row blocks, whole-row taps, values
// edited in dialogs, long help behind an info button; two panes (title, status, Play | settings) in wide windows.
package org.th08.port

import android.content.Context
import android.content.res.Configuration
import android.content.res.Resources
import androidx.compose.animation.AnimatedVisibility
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
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
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
import androidx.compose.material3.RadioButton
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
import androidx.window.core.layout.WindowHeightSizeClass
import androidx.window.core.layout.WindowWidthSizeClass
import java.io.File
import java.util.Locale

// One table entry as edited on screen; value "" = unset, booleans "on"/"off".
class Setting(val e: Entry, initial: String) {
    var initial = initial
    var value by mutableStateOf(initial)

    fun ok(): Boolean = (e.kind as? Kind.IntRange)?.let { intOk(it, value) } ?: true
}

sealed interface GameData {
    data object Bundled : GameData
    data class Folder(val dir: File, val found: Boolean) : GameData
    data object Unavailable : GameData
}

private fun GameData.ok() = this == GameData.Bundled || (this as? GameData.Folder)?.found == true

interface LauncherActions {
    fun change(r: Setting, v: String)
    fun reset()
    fun recheck()
    fun pick(r: Setting)
    fun play()
    fun packList() {}
    fun packInstall(code: String) {}
    fun packRemove(code: String) {}
}

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

// "versionName (channel)", from the Gradle build (th08.versionName, th08.channel).
@Composable
private fun VersionLine() = Text("${BuildConfig.VERSION_NAME} (${BuildConfig.CHANNEL})",
    style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)

@Composable
private fun str(id: Int, vararg args: Any): String = LocalStrings.current.getString(id, *args)

@Composable
private fun Entry.t() = text(LocalLang.current)

@Composable
private fun Entry.show(v: String) = display(v, LocalLang.current, LocalStrings.current)

@Composable
private fun Setting.errorText(): String? = (e.kind as? Kind.IntRange)?.let { intError(it, value, LocalStrings.current) }

private val BLOCK = RoundedCornerShape(16.dp)
private val ROW = RoundedCornerShape(4.dp)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LauncherScreen(groups: List<Group>, rows: List<Setting>, data: GameData, act: LauncherActions) {
    val size = currentWindowAdaptiveInfo().windowSizeClass
    // Landscape phones and large screens: two panes; portrait tablets keep one column (capped width).
    val w = size.windowWidthSizeClass
    val wide = w == WindowWidthSizeClass.EXPANDED ||
        (w == WindowWidthSizeClass.MEDIUM && size.windowHeightSizeClass == WindowHeightSizeClass.COMPACT)
    val canPlay = data.ok() && rows.all { it.ok() }
    var info by remember { mutableStateOf<Pair<String, String>?>(null) }
    val showInfo = { title: String, text: String -> info = title to text }
    if (wide) {
        Surface(color = MaterialTheme.colorScheme.surface, modifier = Modifier.fillMaxSize()) {
            Row(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
                Column(
                    Modifier.weight(0.4f).fillMaxHeight().verticalScroll(rememberScrollState()).padding(24.dp),
                    verticalArrangement = Arrangement.spacedBy(16.dp),
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(str(R.string.title), style = MaterialTheme.typography.headlineMedium)
                            Text(str(R.string.subtitle), style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant)
                            VersionLine()
                        }
                        ResetMenu(act)
                    }
                    Block { StatusRow(data, act, showInfo) }
                    PlayButton(canPlay, act)
                }
                Box(Modifier.weight(0.6f).fillMaxHeight(), contentAlignment = Alignment.TopCenter) {
                    SettingsList(groups, rows, act, showInfo, PaddingValues(16.dp), status = null)
                }
            }
        }
    } else {
        val scroll = TopAppBarDefaults.pinnedScrollBehavior()
        Scaffold(
            modifier = Modifier.nestedScroll(scroll.nestedScrollConnection),
            containerColor = MaterialTheme.colorScheme.surface,
            contentWindowInsets = WindowInsets.safeDrawing,
            topBar = {
                TopAppBar(
                    title = {
                        Column {
                            Text(str(R.string.title))
                            VersionLine()
                        }
                    },
                    actions = { ResetMenu(act) },
                    scrollBehavior = scroll,
                    colors = TopAppBarDefaults.topAppBarColors(
                        containerColor = MaterialTheme.colorScheme.surface,
                        scrolledContainerColor = MaterialTheme.colorScheme.surfaceContainer,
                    ),
                )
            },
            bottomBar = {
                Surface(color = MaterialTheme.colorScheme.surface) {
                    Box(
                        Modifier.fillMaxWidth()
                            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Bottom))
                            .padding(horizontal = 16.dp, vertical = 12.dp),
                        contentAlignment = Alignment.Center,
                    ) { PlayButton(canPlay, act, Modifier.widthIn(max = 640.dp)) }
                }
            },
        ) { pad ->
            Box(Modifier.fillMaxSize().padding(pad), contentAlignment = Alignment.TopCenter) {
                SettingsList(groups, rows, act, showInfo, PaddingValues(horizontal = 16.dp, vertical = 8.dp),
                    status = { StatusRow(data, act, showInfo) })
            }
        }
    }
    info?.let { (t, s) -> InfoDialog(t, s) { info = null } }
}

@Composable
private fun ResetMenu(act: LauncherActions) {
    var menu by remember { mutableStateOf(false) }
    Box {
        IconButton(onClick = { menu = true }) { Icon(Icons.Default.MoreVert, str(R.string.more)) }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(text = { Text(str(R.string.reset_all)) }, onClick = { menu = false; act.reset() })
        }
    }
}

@Composable
private fun PlayButton(enabled: Boolean, act: LauncherActions, modifier: Modifier = Modifier) {
    Button(onClick = act::play, enabled = enabled, modifier = modifier.fillMaxWidth().height(56.dp)) {
        Icon(Icons.Default.PlayArrow, null)
        Spacer(Modifier.width(8.dp))
        Text(str(R.string.play), style = MaterialTheme.typography.titleMedium)
    }
}

@Composable
private fun SettingsList(
    groups: List<Group>,
    rows: List<Setting>,
    act: LauncherActions,
    showInfo: (String, String) -> Unit,
    pad: PaddingValues,
    status: (@Composable () -> Unit)?,
) {
    var devOpen by rememberSaveable { mutableStateOf(rows.any { it.e.developer && it.value != it.e.default }) }
    val player = rows.filterNot { it.e.developer }
    val dev = rows.filter { it.e.developer }
    LazyColumn(
        modifier = Modifier.widthIn(max = 640.dp).fillMaxSize(),
        contentPadding = pad,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (status != null) item(key = "status") { Block { status() } }
        groupBlocks(groups, player, act, showInfo)
        if (dev.isNotEmpty()) {
            item(key = "dev") {
                val changed = dev.count { it.value != it.e.default }
                Header(str(R.string.developer))
                Block {
                    ShellRow(
                        title = str(R.string.developer_options),
                        summary = str(R.string.developer_summary) +
                            if (changed > 0) str(R.string.changed_count, changed) else "",
                        modifier = Modifier.clickable { devOpen = !devOpen },
                    ) { Icon(if (devOpen) Icons.Default.KeyboardArrowUp else Icons.Default.KeyboardArrowDown, null) }
                }
            }
            if (devOpen) groupBlocks(groups, dev, act, showInfo)
        }
    }
}

private fun LazyListScope.groupBlocks(
    groups: List<Group>,
    rows: List<Setting>,
    act: LauncherActions,
    showInfo: (String, String) -> Unit,
) {
    for (g in groups) {
        val rs = rows.filter { it.e.group == g.id }
        if (rs.isEmpty()) continue
        item(key = "g:" + g.id + rs.first().e.developer) {
            Header(g.title(LocalLang.current))
            Block { rs.forEach { SettingRow(it, act, showInfo) } }
            if (g.id == "language" && !rs.first().e.developer) {
                Spacer(Modifier.size(8.dp))
                TranslationsBlock(act)
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
private fun ShellRow(
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
                    Text(summary, style = MaterialTheme.typography.bodyMedium,
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

@Composable
private fun StatusRow(d: GameData, act: LauncherActions, showInfo: (String, String) -> Unit) {
    val ok = d.ok()
    val (summary, detail) = when (d) {
        GameData.Bundled -> str(R.string.data_bundled) to str(R.string.data_bundled_detail)
        is GameData.Folder if d.found -> str(R.string.data_found) to str(R.string.data_found_detail, d.dir.path)
        is GameData.Folder -> str(R.string.data_missing) to str(R.string.data_missing_detail, d.dir.path)
        GameData.Unavailable -> str(R.string.data_unavailable) to str(R.string.data_unavailable_detail)
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
        Column(Modifier.weight(1f).padding(vertical = 10.dp)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            Text(summary, style = MaterialTheme.typography.bodyMedium,
                color = if (ok) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.error)
        }
        if (!ok) TextButton(onClick = act::recheck) { Text(str(R.string.check_again)) }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SettingRow(r: Setting, act: LauncherActions, showInfo: (String, String) -> Unit) {
    val e = r.e
    val t = e.t()
    val details = str(R.string.details, t.summary, t.help)
    val info = { showInfo(t.label, details) }
    var editing by rememberSaveable(e.key) { mutableStateOf(false) }
    // `game_text`: follow / ja, then the installed pack languages.
    val packs = if (e.key == "game_text") {
        LocalPacks.current.installed.filter { it.language != "ja" }.distinctBy { it.language }
            .associate { it.language to if (it.title.isEmpty()) it.language else "${it.language} — ${it.title}" }
    } else emptyMap()
    val opts = e.options()?.let { it + packs.keys.filter { k -> k !in it } }
    val shown: @Composable (String) -> String = { v -> packs[v] ?: e.show(v) }
    when {
        e.kind == Kind.Bool -> ShellRow(
            t.label, t.summary, info = info,
            modifier = Modifier.toggleable(r.value == "on", role = Role.Switch) { act.change(r, if (it) "on" else "off") },
        ) { Switch(checked = r.value == "on", onCheckedChange = null) }
        // 2–3 options: segmented buttons in the row.
        opts != null && opts.size <= 3 && e.key != "game_text" -> ShellRow(t.label, t.summary, info = info, below = {
            SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                opts.forEachIndexed { i, v ->
                    SegmentedButton(
                        selected = r.value == v,
                        onClick = { act.change(r, v) },
                        shape = SegmentedButtonDefaults.itemShape(i, opts.size),
                        colors = SegmentedButtonDefaults.colors(inactiveContainerColor = Color.Transparent),
                    ) { Text(e.show(v), maxLines = 1, overflow = TextOverflow.Ellipsis) }
                }
            }
        })
        else -> {
            ShellRow(t.label, t.summary, info = info, modifier = Modifier.clickable { editing = true }) {
                Text(shown(r.value), style = MaterialTheme.typography.labelLarge,
                    color = if (!r.ok()) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary,
                    maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.widthIn(max = 160.dp))
            }
            if (editing) {
                val done = { editing = false }
                val k = e.kind
                when {
                    opts != null -> ChoiceDialog(r, opts, act, done, shown)
                    k is Kind.IntRange && k.max - k.min <= 400 -> SliderDialog(r, k, act, done)
                    else -> TextDialog(r, act, done)
                }
            }
        }
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
        text = { Text(text) },
        confirmButton = { TextButton(onClick = close) { Text(str(R.string.ok)) } },
    )
}

// Radio list; the replay row adds the attract demos' and a picked file's entries and a file chooser.
@Composable
private fun ChoiceDialog(
    r: Setting,
    opts: List<String>,
    act: LauncherActions,
    close: () -> Unit,
    shown: @Composable (String) -> String = { r.e.show(it) },
) {
    AlertDialog(
        onDismissRequest = close,
        title = { Text(r.e.t().label) },
        text = {
            Column(Modifier.selectableGroup().verticalScroll(rememberScrollState())) {
                opts.forEach { v ->
                    Row(
                        Modifier.fillMaxWidth().heightIn(min = 48.dp).clip(RoundedCornerShape(8.dp))
                            .selectable(r.value == v, role = Role.RadioButton) { act.change(r, v); close() }
                            .padding(horizontal = 8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        RadioButton(selected = r.value == v, onClick = null)
                        Spacer(Modifier.width(16.dp))
                        Text(shown(v), style = MaterialTheme.typography.bodyLarge)
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = close) { Text(str(R.string.cancel)) } },
    )
}

@Composable
private fun SliderDialog(r: Setting, k: Kind.IntRange, act: LauncherActions, close: () -> Unit) {
    val start = r.value.toLongOrNull() ?: r.e.default.toLongOrNull() ?: k.min
    var v by remember { mutableStateOf(start.toFloat()) }
    val n = Math.round(v).toLong()
    AlertDialog(
        onDismissRequest = close,
        title = { Text(r.e.t().label) },
        text = {
            Column {
                Text(r.e.show(n.toString()), style = MaterialTheme.typography.headlineSmall,
                    color = MaterialTheme.colorScheme.primary, modifier = Modifier.align(Alignment.CenterHorizontally))
                Slider(
                    value = v,
                    onValueChange = { v = it },
                    valueRange = k.min.toFloat()..k.max.toFloat(),
                    steps = if (k.max - k.min <= 20) (k.max - k.min - 1).toInt().coerceAtLeast(0) else 0,
                )
                Row {
                    Text(r.e.show(k.min.toString()), style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Spacer(Modifier.weight(1f))
                    Text(r.e.show(k.max.toString()), style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        },
        confirmButton = { TextButton(onClick = { act.change(r, n.toString()); close() }) { Text(str(R.string.ok)) } },
        dismissButton = {
            Row {
                TextButton(onClick = { act.change(r, r.e.default); close() }) {
                    Text(str(if (r.e.default.isEmpty()) R.string.clear else R.string.default_value))
                }
                TextButton(onClick = close) { Text(str(R.string.cancel)) }
            }
        },
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
            Column {
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
            TextButton(onClick = { act.change(r, t.trim()); close() }, enabled = err == null) { Text(str(R.string.ok)) }
        },
        dismissButton = {
            Row {
                if (r.e.default.isEmpty()) TextButton(onClick = { act.change(r, ""); close() }) { Text(str(R.string.clear)) }
                TextButton(onClick = close) { Text(str(R.string.cancel)) }
            }
        },
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
