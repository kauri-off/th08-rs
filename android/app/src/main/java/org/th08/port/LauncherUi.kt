// Launcher screen (MATERIAL2), Geode-launcher style: section headers over segmented row blocks, whole-row taps, values
// edited in dialogs, long help behind an info button; two panes (title, status, Play | settings) in wide windows.
package org.th08.port

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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.window.core.layout.WindowHeightSizeClass
import androidx.window.core.layout.WindowWidthSizeClass
import java.io.File

// One table entry as edited on screen; value "" = unset, booleans "on"/"off".
class Setting(val e: Entry, initial: String) {
    var initial = initial
    var value by mutableStateOf(initial)

    fun error(): String? = (e.kind as? Kind.IntRange)?.let { intError(it, value) }
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
}

private const val TITLE = "Imperishable Night"
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
    val canPlay = data.ok() && rows.none { it.error() != null }
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
                            Text(TITLE, style = MaterialTheme.typography.headlineMedium)
                            Text("Touhou 8 port", style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant)
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
                    title = { Text(TITLE) },
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
        IconButton(onClick = { menu = true }) { Icon(Icons.Default.MoreVert, "More") }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(text = { Text("Reset all settings") }, onClick = { menu = false; act.reset() })
        }
    }
}

@Composable
private fun PlayButton(enabled: Boolean, act: LauncherActions, modifier: Modifier = Modifier) {
    Button(onClick = act::play, enabled = enabled, modifier = modifier.fillMaxWidth().height(56.dp)) {
        Icon(Icons.Default.PlayArrow, null)
        Spacer(Modifier.width(8.dp))
        Text("Play", style = MaterialTheme.typography.titleMedium)
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
                Header("Developer")
                Block {
                    ShellRow(
                        title = "Developer options",
                        summary = "Cheats and tools for the next start only" +
                            if (changed > 0) " · $changed changed" else "",
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
            Header(g.title)
            Block { rs.forEach { SettingRow(it, act, showInfo) } }
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
                    Icon(Icons.Outlined.Info, "About this setting", tint = MaterialTheme.colorScheme.onSurfaceVariant)
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
        GameData.Bundled -> "Included in this app" to "th08.dat and thbgm.dat are bundled in this APK."
        is GameData.Folder if d.found -> "Found in the game folder" to "th08.dat was found in:\n${d.dir.path}"
        is GameData.Folder -> "Missing: add th08.dat and thbgm.dat" to
            "Copy th08.dat and thbgm.dat from your Imperishable Night folder into:\n${d.dir.path}\n\n" +
            "For example from a computer:\nadb push th08.dat thbgm.dat ${d.dir.path}/"
        GameData.Unavailable -> "No storage for the game folder" to "External storage is unavailable."
    }
    Row(
        Modifier.fillMaxWidth().clip(ROW).background(MaterialTheme.colorScheme.surfaceContainer)
            .clickable { showInfo("Game data", detail) }.heightIn(min = 64.dp).padding(start = 16.dp, end = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(if (ok) Icons.Default.CheckCircle else Icons.Default.Warning, null,
            tint = if (ok) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.error)
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f).padding(vertical = 10.dp)) {
            Text("Game data", style = MaterialTheme.typography.bodyLarge)
            Text(summary, style = MaterialTheme.typography.bodyMedium,
                color = if (ok) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.error)
        }
        if (!ok) TextButton(onClick = act::recheck) { Text("Check again") }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SettingRow(r: Setting, act: LauncherActions, showInfo: (String, String) -> Unit) {
    val e = r.e
    val info = { showInfo(e.label, e.summary + ".\n\nDetails: " + e.help) }
    var editing by rememberSaveable(e.key) { mutableStateOf(false) }
    val opts = e.options()
    when {
        e.kind == Kind.Bool -> ShellRow(
            e.label, e.summary, info = info,
            modifier = Modifier.toggleable(r.value == "on", role = Role.Switch) { act.change(r, if (it) "on" else "off") },
        ) { Switch(checked = r.value == "on", onCheckedChange = null) }
        // 2–3 options: segmented buttons in the row.
        opts != null && opts.size <= 3 -> ShellRow(e.label, e.summary, info = info, below = {
            SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                opts.forEachIndexed { i, v ->
                    SegmentedButton(
                        selected = r.value == v,
                        onClick = { act.change(r, v) },
                        shape = SegmentedButtonDefaults.itemShape(i, opts.size),
                        colors = SegmentedButtonDefaults.colors(inactiveContainerColor = Color.Transparent),
                    ) { Text(e.display(v), maxLines = 1, overflow = TextOverflow.Ellipsis) }
                }
            }
        })
        else -> {
            ShellRow(e.label, e.summary, info = info, modifier = Modifier.clickable { editing = true }) {
                Text(e.display(r.value), style = MaterialTheme.typography.labelLarge,
                    color = if (r.error() != null) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary,
                    maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.widthIn(max = 160.dp))
            }
            if (editing) {
                val done = { editing = false }
                val k = e.kind
                when {
                    opts != null -> ChoiceDialog(r, opts, act, done)
                    k is Kind.IntRange && k.max - k.min <= 400 -> SliderDialog(r, k, act, done)
                    else -> TextDialog(r, act, done)
                }
            }
        }
    }
}

@Composable
private fun InfoDialog(title: String, text: String, close: () -> Unit) {
    AlertDialog(
        onDismissRequest = close,
        icon = { Icon(Icons.Outlined.Info, null) },
        title = { Text(title) },
        text = { Text(text) },
        confirmButton = { TextButton(onClick = close) { Text("OK") } },
    )
}

// Radio list; the replay row adds the attract demos' and a picked file's entries and a file chooser.
@Composable
private fun ChoiceDialog(r: Setting, opts: List<String>, act: LauncherActions, close: () -> Unit) {
    AlertDialog(
        onDismissRequest = close,
        title = { Text(r.e.label) },
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
                        Text(r.e.display(v), style = MaterialTheme.typography.bodyLarge)
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = close) { Text("Cancel") } },
    )
}

@Composable
private fun SliderDialog(r: Setting, k: Kind.IntRange, act: LauncherActions, close: () -> Unit) {
    val start = r.value.toLongOrNull() ?: r.e.default.toLongOrNull() ?: k.min
    var v by remember { mutableStateOf(start.toFloat()) }
    val n = Math.round(v).toLong()
    AlertDialog(
        onDismissRequest = close,
        title = { Text(r.e.label) },
        text = {
            Column {
                Text(r.e.display(n.toString()), style = MaterialTheme.typography.headlineSmall,
                    color = MaterialTheme.colorScheme.primary, modifier = Modifier.align(Alignment.CenterHorizontally))
                Slider(
                    value = v,
                    onValueChange = { v = it },
                    valueRange = k.min.toFloat()..k.max.toFloat(),
                    steps = if (k.max - k.min <= 20) (k.max - k.min - 1).toInt().coerceAtLeast(0) else 0,
                )
                Row {
                    Text(r.e.display(k.min.toString()), style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Spacer(Modifier.weight(1f))
                    Text(r.e.display(k.max.toString()), style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        },
        confirmButton = { TextButton(onClick = { act.change(r, n.toString()); close() }) { Text("OK") } },
        dismissButton = {
            Row {
                TextButton(onClick = { act.change(r, r.e.default); close() }) {
                    Text(if (r.e.default.isEmpty()) "Clear" else "Default")
                }
                TextButton(onClick = close) { Text("Cancel") }
            }
        },
    )
}

// Number or text entry; the replay row also offers a file chooser.
@Composable
private fun TextDialog(r: Setting, act: LauncherActions, close: () -> Unit) {
    var t by remember { mutableStateOf(r.value) }
    val k = r.e.kind
    val err = (k as? Kind.IntRange)?.let { intError(it, t.trim()) }
    val hint = when (k) {
        is Kind.IntRange -> "${k.min}–${k.max}"
        is Kind.Text -> k.format
        else -> ""
    }
    AlertDialog(
        onDismissRequest = close,
        title = { Text(r.e.label) },
        text = {
            Column {
                OutlinedTextField(
                    value = t,
                    onValueChange = { t = it },
                    singleLine = true,
                    placeholder = { Text(r.e.display(""), fontFamily = FontFamily.Default) },
                    isError = err != null,
                    supportingText = { Text(err ?: if (k is Kind.IntRange) "Range $hint" else "Format: $hint") },
                    keyboardOptions = KeyboardOptions(
                        keyboardType = if (k is Kind.IntRange) KeyboardType.Number else KeyboardType.Text,
                    ),
                    modifier = Modifier.fillMaxWidth(),
                )
                if (r.e.key == "replay") {
                    HorizontalDivider(Modifier.padding(vertical = 8.dp))
                    (0..3).forEach { i ->
                        TextButton(onClick = { act.change(r, "demo$i"); close() }) { Text("Demo ${i + 1}") }
                    }
                    TextButton(onClick = { close(); act.pick(r) }) { Text("Choose a replay file…") }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = { act.change(r, t.trim()); close() }, enabled = err == null) { Text("OK") }
        },
        dismissButton = {
            Row {
                if (r.e.default.isEmpty()) TextButton(onClick = { act.change(r, ""); close() }) { Text("Clear") }
                TextButton(onClick = close) { Text("Cancel") }
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
        Entry(key, label, summary, "$summary.", unit, words, kind, default, group, dev, true, !dev)
    val groups = listOf(Group("display", "Display"), Group("touch", "Touch controls"), Group("start_game", "Start game"),
        Group("cheats", "Cheats/debug"))
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
    Th08Theme(dark) { LauncherScreen(g, rs, GameData.Bundled, PreviewActions) }
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
