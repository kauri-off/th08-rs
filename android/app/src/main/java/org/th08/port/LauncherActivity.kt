// Android settings screen (MATERIAL2): the Compose screen of LauncherUi.kt built from the settings table, Play starts
// the Rust NativeActivity in the ":game" process. Player rows are saved to port.cfg on change (as the egui launcher's
// `Model::changes`) when the table marks them `persist`; the other rows go to launch.cfg for the next start only.
// Texts follow the `language` row (docs/arch/I18N.md §3.3).
package org.th08.port

import android.app.NativeActivity
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.OpenableColumns
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.LifecycleResumeEffect
import java.io.File

class LauncherActivity : ComponentActivity(), LauncherActions {
    private val cfgFile get() = File(filesDir, "port.cfg")

    private lateinit var schema: Schema
    private lateinit var rows: List<Setting>
    private var data by mutableStateOf<GameData>(GameData.Unavailable)
    private var picking: Setting? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        schema = Schema.load(this)
        val cfg = PortCfg.read(cfgFile)
        rows = schema.entries.filter { it.android }.map { e ->
            val saved = cfg[e.key]?.let { norm(e.kind, it) }?.takeIf { it.isNotEmpty() }
            Setting(e, saved ?: e.default)
        }
        setContent {
            Th08Theme {
                LifecycleResumeEffect(Unit) {
                    data = checkData()
                    onPauseOrDispose { }
                }
                val pick = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
                    val r = picking ?: return@rememberLauncherForActivityResult
                    picking = null
                    uri?.let { copyPicked(it) }?.let { change(r, it.path) }
                }
                pickFile = { r -> picking = r; pick.launch(arrayOf("*/*")) }
                // The `language` row picks the texts and switches the screen live; the game reads the same value from
                // port.cfg (auto: its own Locale.getDefault(), i18n.rs `os_locale`).
                val lang = resolveLang(rows.firstOrNull { it.e.key == "language" }?.value ?: "auto")
                val res = remember(lang) { localized(this, lang) }
                CompositionLocalProvider(LocalLang provides lang, LocalStrings provides res) {
                    LauncherScreen(schema.groups, rows, data, this)
                }
            }
        }
    }

    private var pickFile: (Setting) -> Unit = {}

    // Bundled assets win, as in android_main; else <external files>/game (adb push target).
    private fun checkData(): GameData {
        if (assets.list("")?.any { it == "th08.pak" || it == "th08.dat" } == true) return GameData.Bundled
        val dir = getExternalFilesDir(null)?.let { File(it, "game") } ?: return GameData.Unavailable
        dir.mkdirs()
        val found = dir.listFiles()?.any { f -> f.isFile && listOf("th08.pak", "th08.dat").any { f.name.equals(it, ignoreCase = true) } } == true
        return GameData.Folder(dir, found)
    }

    // A picked document copied to filesDir/picked/<name>, a path the native game can open.
    private fun copyPicked(uri: Uri): File? = runCatching {
        val name = contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst()) it.getString(0) else null
        }?.replace('/', '_') ?: "picked.rpy"
        val out = File(File(filesDir, "picked").also { it.mkdirs() }, name)
        contentResolver.openInputStream(uri)!!.use { i -> out.outputStream().use { i.copyTo(it) } }
        out
    }.getOrNull()

    override fun change(r: Setting, v: String) {
        r.value = v
        if (!r.e.persist || !r.ok()) return
        PortCfg.write(cfgFile, r.e.key, v.takeIf { it.isNotEmpty() && it != r.e.default })
    }

    override fun reset() {
        rows.forEach { change(it, it.e.default) }
    }

    override fun recheck() {
        data = checkData()
    }

    override fun pick(r: Setting) = pickFile(r)

    override fun play() {
        val once = listOf("launcher" to "off") + rows
            .filter { !it.e.persist && it.value != it.initial && it.value.isNotEmpty() && it.ok() }
            .map { it.e.key to it.value }
        PortCfg.writeLaunch(File(filesDir, "launch.cfg"), once)
        startActivity(Intent(this, NativeActivity::class.java))
    }
}

// Dynamic colour (Android 12+) following the system light / dark mode.
@Composable
fun Th08Theme(dark: Boolean = isSystemInDarkTheme(), content: @Composable () -> Unit) {
    val ctx = LocalContext.current
    val colors = when {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> if (dark) dynamicDarkColorScheme(ctx) else dynamicLightColorScheme(ctx)
        dark -> darkColorScheme()
        else -> lightColorScheme()
    }
    MaterialTheme(colorScheme = colors, content = content)
}
