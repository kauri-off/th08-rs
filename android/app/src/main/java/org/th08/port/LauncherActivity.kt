// Android settings screen (MATERIAL1): Material 3 cards built from the settings table, Play starts the Rust
// NativeActivity in the ":game" process. Player rows are saved to port.cfg on change (as the egui launcher's
// `Model::changes`) when the table marks them `persist`; the other rows go to launch.cfg for the next start only.
package org.th08.port

import android.app.NativeActivity
import android.content.Intent
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.calculateEndPadding
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LargeTopAppBar
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MenuAnchorType
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
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

class LauncherActivity : ComponentActivity() {
    private val cfgFile get() = File(filesDir, "port.cfg")

    private lateinit var schema: Schema
    private lateinit var rows: List<Setting>
    private var data by mutableStateOf<GameData>(GameData.Unavailable)

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        schema = Schema.load(this)
        val cfg = PortCfg.read(cfgFile)
        rows = schema.entries.filter { it.android }.map { e ->
            val saved = cfg[e.key]?.let { norm(e.kind, it) }?.takeIf { it.isNotEmpty() }
            Setting(e, saved ?: e.default)
        }
        setContent { Th08Theme { Screen() } }
    }

    // Bundled assets win, as in android_main; else <external files>/game (adb push target).
    private fun checkData(): GameData {
        if (assets.list("")?.contains("th08.dat") == true) return GameData.Bundled
        val dir = getExternalFilesDir(null)?.let { File(it, "game") } ?: return GameData.Unavailable
        dir.mkdirs()
        val found = dir.listFiles()?.any { it.isFile && it.name.equals("th08.dat", ignoreCase = true) } == true
        return GameData.Folder(dir, found)
    }

    private fun change(r: Setting, v: String) {
        r.value = v
        if (!r.e.persist || r.error() != null) return
        PortCfg.write(cfgFile, r.e.key, v.takeIf { it.isNotEmpty() && it != r.e.default })
    }

    private fun reset() {
        rows.forEach { change(it, it.e.default) }
    }

    private fun play() {
        val once = listOf("launcher" to "off") + rows
            .filter { !it.e.persist && it.value != it.initial && it.value.isNotEmpty() && it.error() == null }
            .map { it.e.key to it.value }
        PortCfg.writeLaunch(File(filesDir, "launch.cfg"), once)
        startActivity(Intent(this, NativeActivity::class.java))
    }

    @OptIn(ExperimentalMaterial3Api::class)
    @Composable
    private fun Screen() {
        LifecycleResumeEffect(Unit) {
            data = checkData()
            onPauseOrDispose { }
        }
        val scroll = TopAppBarDefaults.exitUntilCollapsedScrollBehavior()
        var menu by remember { mutableStateOf(false) }
        var devOpen by remember { mutableStateOf(rows.any { it.e.developer && it.value != it.e.default }) }
        val canPlay = data != GameData.Unavailable && (data as? GameData.Folder)?.found != false &&
            rows.none { it.error() != null }
        Scaffold(
            modifier = Modifier.nestedScroll(scroll.nestedScrollConnection),
            contentWindowInsets = WindowInsets.safeDrawing,
            topBar = {
                LargeTopAppBar(
                    title = { Text("Imperishable Night") },
                    scrollBehavior = scroll,
                    actions = {
                        IconButton(onClick = { menu = true }) { Icon(Icons.Default.MoreVert, "More") }
                        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                            DropdownMenuItem(text = { Text("Reset settings") }, onClick = { menu = false; reset() })
                        }
                    },
                )
            },
            floatingActionButton = {
                if (canPlay) {
                    ExtendedFloatingActionButton(
                        onClick = ::play,
                        icon = { Icon(Icons.Default.PlayArrow, null) },
                        text = { Text("Play") },
                    )
                }
            },
        ) { pad ->
            val dir = LocalLayoutDirection.current
            LazyColumn(
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(
                    start = pad.calculateStartPadding(dir) + 16.dp,
                    end = pad.calculateEndPadding(dir) + 16.dp,
                    top = pad.calculateTopPadding() + 8.dp,
                    bottom = pad.calculateBottomPadding() + 96.dp,
                ),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                item { StatusCard(data) }
                val player = rows.filterNot { it.e.developer }
                val dev = rows.filter { it.e.developer }
                items(schema.groups.filter { g -> player.any { it.e.group == g.id } }) { g ->
                    GroupCard(g.title, player.filter { it.e.group == g.id })
                }
                item {
                    DeveloperCard(devOpen, { devOpen = !devOpen }) {
                        for (g in schema.groups) {
                            val rs = dev.filter { it.e.group == g.id }
                            if (rs.isEmpty()) continue
                            Text(g.title, style = MaterialTheme.typography.titleSmall,
                                color = MaterialTheme.colorScheme.primary,
                                modifier = Modifier.padding(start = 16.dp, top = 12.dp, bottom = 4.dp))
                            rs.forEach { SettingRow(it) }
                        }
                    }
                }
            }
        }
    }

    @Composable
    private fun StatusCard(d: GameData) {
        val ok = d == GameData.Bundled || (d as? GameData.Folder)?.found == true
        ElevatedCard(
            colors = if (ok) CardDefaults.elevatedCardColors() else CardDefaults.elevatedCardColors(
                containerColor = MaterialTheme.colorScheme.errorContainer,
                contentColor = MaterialTheme.colorScheme.onErrorContainer,
            ),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(if (ok) Icons.Default.CheckCircle else Icons.Default.Warning, null,
                        tint = if (ok) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.error)
                    Spacer(Modifier.width(12.dp))
                    Text("Game data", style = MaterialTheme.typography.titleMedium)
                }
                when (d) {
                    GameData.Bundled -> Text("th08.dat and thbgm.dat are bundled in this APK.")
                    is GameData.Folder if d.found -> {
                        Text("th08.dat found in the game folder:")
                        Text(d.dir.path, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                    }
                    is GameData.Folder -> {
                        Text("Copy th08.dat and thbgm.dat from your Imperishable Night folder into:")
                        Text(d.dir.path, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                        Text("For example from a computer: adb push th08.dat thbgm.dat ${d.dir.path}/",
                            style = MaterialTheme.typography.bodySmall)
                        TextButton(onClick = { data = checkData() }) { Text("Check again") }
                    }
                    GameData.Unavailable -> Text("No external storage: the game folder is unavailable.")
                }
            }
        }
    }

    @Composable
    private fun GroupCard(title: String, rs: List<Setting>) {
        Card(modifier = Modifier.fillMaxWidth()) {
            Text(title, style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.padding(start = 16.dp, top = 16.dp, bottom = 4.dp))
            rs.forEachIndexed { i, r ->
                if (i > 0) HorizontalDivider(Modifier.padding(horizontal = 16.dp))
                SettingRow(r)
            }
            Spacer(Modifier.size(8.dp))
        }
    }

    @Composable
    private fun DeveloperCard(open: Boolean, toggle: () -> Unit, content: @Composable () -> Unit) {
        Card(modifier = Modifier.fillMaxWidth()) {
            ListItem(
                headlineContent = { Text("Developer") },
                supportingContent = { Text("Cheats and tools; they apply to the next start only and are not saved.") },
                trailingContent = {
                    Icon(if (open) Icons.Default.KeyboardArrowUp else Icons.Default.KeyboardArrowDown, null)
                },
                colors = ListItemDefaults.colors(containerColor = CardDefaults.cardColors().containerColor),
                modifier = Modifier.clickable(onClick = toggle),
            )
            AnimatedVisibility(open) {
                Column(Modifier.padding(bottom = 8.dp)) { content() }
            }
        }
    }

    @OptIn(ExperimentalMaterial3Api::class)
    @Composable
    private fun SettingRow(r: Setting) {
        val clear = ListItemDefaults.colors(containerColor = CardDefaults.cardColors().containerColor)
        when (val k = r.e.kind) {
            Kind.Bool -> ListItem(
                headlineContent = { Text(r.e.label) },
                supportingContent = { Text(r.e.help) },
                trailingContent = { Switch(checked = r.value == "on", onCheckedChange = { change(r, if (it) "on" else "off") }) },
                colors = clear,
                modifier = Modifier.clickable { change(r, if (r.value == "on") "off" else "on") },
            )
            is Kind.IntRange if r.e.default.isNotEmpty() && k.max - k.min <= 400 -> Column(
                Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(r.e.label, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
                    Text(r.value, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                }
                Slider(
                    value = r.value.toFloatOrNull() ?: r.e.default.toFloat(),
                    onValueChange = { change(r, Math.round(it).toString()) },
                    valueRange = k.min.toFloat()..k.max.toFloat(),
                    // Tick marks only for short ranges; values are rounded to integers either way.
                    steps = if (k.max - k.min <= 20) (k.max - k.min - 1).toInt().coerceAtLeast(0) else 0,
                )
                Help(r.e.help)
            }
            is Kind.IntRange -> Field(r, KeyboardType.Number, if (r.e.default.isEmpty()) "not set" else r.e.default)
            is Kind.Text -> Field(r, KeyboardType.Text, k.format.ifEmpty { "not set" })
            is Kind.Choice -> {
                var open by remember { mutableStateOf(false) }
                val unset = r.e.default.isEmpty()
                Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
                    ExposedDropdownMenuBox(expanded = open, onExpandedChange = { open = it }) {
                        OutlinedTextField(
                            value = r.value.ifEmpty { "(not set)" },
                            onValueChange = {},
                            readOnly = true,
                            singleLine = true,
                            label = { Text(r.e.label) },
                            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(open) },
                            modifier = Modifier.fillMaxWidth().menuAnchor(MenuAnchorType.PrimaryNotEditable),
                        )
                        ExposedDropdownMenu(expanded = open, onDismissRequest = { open = false }) {
                            val choices = (if (unset) listOf("") else emptyList()) + k.choices
                            for (c in choices) {
                                DropdownMenuItem(
                                    text = { Text(c.ifEmpty { "(not set)" }) },
                                    onClick = { change(r, c); open = false },
                                    contentPadding = ExposedDropdownMenuDefaults.ItemContentPadding,
                                )
                            }
                        }
                    }
                    Help(r.e.help)
                }
            }
        }
    }

    @Composable
    private fun Field(r: Setting, type: KeyboardType, hint: String) {
        val err = r.error()
        OutlinedTextField(
            value = r.value,
            onValueChange = { change(r, it) },
            singleLine = true,
            label = { Text(r.e.label) },
            placeholder = { Text(hint) },
            isError = err != null,
            supportingText = { Text(err ?: r.e.help) },
            keyboardOptions = KeyboardOptions(keyboardType = type),
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
        )
    }

    @Composable
    private fun Help(text: String) {
        Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
fun Th08Theme(content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    val ctx = LocalContext.current
    val colors = when {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> if (dark) dynamicDarkColorScheme(ctx) else dynamicLightColorScheme(ctx)
        dark -> darkColorScheme()
        else -> lightColorScheme()
    }
    MaterialTheme(colorScheme = colors, content = content)
}
