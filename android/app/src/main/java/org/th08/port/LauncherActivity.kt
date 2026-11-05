// Android settings screen (MATERIAL2): the Compose screen of LauncherUi.kt built from the settings table, Play starts
// the Rust NativeActivity in the ":game" process. Player rows are saved to port.cfg on change (as the egui launcher's
// `Model::changes`) when the table marks them `persist`; the other rows go to launch.cfg for the next start only.
// Texts follow the `language` row (docs/arch/I18N.md §3.3).
package org.th08.port

import android.app.ActivityManager
import android.app.NativeActivity
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.util.Log
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
import androidx.compose.runtime.LaunchedEffect
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File

class LauncherActivity : ComponentActivity(), LauncherActions {
    private val cfgFile get() = File(filesDir, "port.cfg")

    private lateinit var schema: Schema
    private lateinit var rows: List<Setting>
    private val devKeys by lazy { schema.entries.filter { it.developer }.map { it.key } }
    private var data by mutableStateOf<GameData>(GameData.Unavailable)
    private var picking: Setting? = null
    private var packs by mutableStateOf(PackState())
    private var saves by mutableStateOf(SaveState())
    private var exportDoc: (String) -> Unit = {}
    private var importDoc: () -> Unit = {}
    private var importCfg = false

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        schema = Schema.load(this)
        val cfg = PortCfg.read(cfgFile)
        // Developer rows always start at their default: the game ignores them in port.cfg.
        rows = schema.entries.filter { it.android }.map { e ->
            val saved = cfg[e.key]?.takeUnless { e.developer }?.let { norm(e.kind, it) }?.takeIf { it.isNotEmpty() }
            Setting(e, saved ?: e.default)
        }
        Thread { copyBundledPacks(); runOnUiThread { packCall("status") } }.start()
        setContent {
            Th08Theme {
                LifecycleResumeEffect(Unit) {
                    data = checkData()
                    saves = saves.copy(gameRunning = gameRunning())
                    onPauseOrDispose { }
                }
                val exp = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/zip")) { uri ->
                    uri?.let(::exportTo)
                }
                val imp = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
                    uri?.let(::importFrom)
                }
                exportDoc = { name -> exp.launch(name) }
                importDoc = { imp.launch(arrayOf("application/zip", "application/x-zip-compressed", "application/octet-stream")) }
                // Poll the Rust job while one runs.
                LaunchedEffect(packs.busy) {
                    while (packs.busy != null) {
                        delay(250)
                        packs = withContext(Dispatchers.Default) { PackState.parse(Packs.call(request("status"))) }
                    }
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
                CompositionLocalProvider(LocalLang provides lang, LocalStrings provides res, LocalPacks provides packs,
                    LocalSaves provides saves) {
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

    private fun gameRunning(): Boolean = (getSystemService(ACTIVITY_SERVICE) as ActivityManager)
        .runningAppProcesses.orEmpty().any { it.processName == "$packageName:game" }

    private fun saveRequest(op: String, zip: File, extra: (JSONObject) -> Unit = {}): String = JSONObject().apply {
        put("op", op)
        put("config", filesDir.path)
        put("zip", zip.path)
        extra(this)
    }.toString()

    override fun saveExport() {
        if (gameRunning()) return run { saves = saves.copy(gameRunning = true) }
        val name = runCatching { JSONObject(Saves.call(saveRequest("name", cacheDir))).getString("name") }
            .getOrDefault("th08-saves.zip")
        exportDoc(name)
    }

    override fun saveImport(portCfg: Boolean) {
        if (gameRunning()) return run { saves = saves.copy(gameRunning = true) }
        importCfg = portCfg
        importDoc()
    }

    // The zip is built in the cache, then copied into the picked document.
    private fun exportTo(uri: Uri) = saveJob(export = true) {
        val tmp = File(cacheDir, "saves-export.zip")
        try {
            val out = parseSaveReply(Saves.call(saveRequest("export", tmp)), export = true)
            if (out is SaveOutcome.Exported) {
                contentResolver.openOutputStream(uri, "wt")!!.use { o -> tmp.inputStream().use { it.copyTo(o) } }
            }
            out
        } finally {
            tmp.delete()
        }
    }

    // The picked document is copied into the cache (at most `Saves.MAX_ZIP` bytes), then validated and imported.
    private fun importFrom(uri: Uri) = saveJob(export = false) {
        val tmp = File(cacheDir, "saves-import.zip")
        try {
            contentResolver.openInputStream(uri)!!.use { i ->
                tmp.outputStream().use { o ->
                    val buf = ByteArray(1 shl 16)
                    var total = 0L
                    while (true) {
                        val n = i.read(buf)
                        if (n < 0) break
                        total += n
                        if (total > Saves.MAX_ZIP) return@saveJob SaveOutcome.Failed(false, "larger than 256 MiB")
                        o.write(buf, 0, n)
                    }
                }
            }
            val cfg = importCfg
            val out = parseSaveReply(Saves.call(saveRequest("import", tmp) { it.put("port_cfg", cfg) }), export = false)
            if (out is SaveOutcome.Imported && cfg) runOnUiThread(::reloadPortCfg)
            out
        } finally {
            tmp.delete()
        }
    }

    private fun saveJob(export: Boolean, job: () -> SaveOutcome) {
        saves = saves.copy(busy = true, outcome = null)
        Thread {
            val out = runCatching(job).getOrElse { SaveOutcome.Failed(export, it.message ?: it.toString()) }
            runOnUiThread { saves = saves.copy(busy = false, outcome = out) }
        }.start()
    }

    // Persisted player rows take the values of a port.cfg just imported.
    private fun reloadPortCfg() {
        val cfg = PortCfg.read(cfgFile)
        rows.filter { it.e.persist && !it.e.developer }.forEach { r ->
            r.value = cfg[r.e.key]?.let { norm(r.e.kind, it) }?.takeIf { it.isNotEmpty() } ?: r.e.default
        }
    }

    override fun change(r: Setting, v: String) {
        r.value = v
        if (!r.e.persist || !r.ok()) return
        runCatching {
            PortCfg.write(cfgFile, r.e.key, v.takeIf { it.isNotEmpty() && it != r.e.default }, devKeys)
        }.onFailure { Log.e("th08", "port.cfg not saved: $it") }
    }

    override fun reset() {
        rows.forEach { change(it, it.e.default) }
    }

    override fun recheck() {
        data = checkData()
    }

    override fun pick(r: Setting) = pickFile(r)

    private fun request(op: String, code: String = ""): String = JSONObject().apply {
        put("op", op)
        put("config", filesDir.path)
        put("code", code)
        (data as? GameData.Folder)?.let { put("dir", it.dir.path) }
    }.toString()

    private fun packCall(op: String, code: String = "", extra: (JSONObject) -> Unit = {}) {
        val req = JSONObject(request(op, code)).also(extra).toString()
        packs = runCatching { PackState.parse(Packs.call(req)) }
            .getOrElse { PackState(outcome = PackOutcome.Failed(code, it.message ?: it.toString())) }
    }

    override fun packList() = packCall("list")

    override fun packRemove(code: String) = packCall("remove", code)

    // The importer reads th08.pak / th08.dat: a bundled asset by descriptor (stored, noCompress), else the folder.
    override fun packInstall(code: String) {
        val asset = listOf("th08.pak", "th08.dat").firstOrNull { n -> assets.list("")?.contains(n) == true }
        if (asset == null) return packCall("install", code)
        val afd = runCatching { assets.openFd(asset) }.getOrNull() ?: return packCall("install", code)
        val fd = afd.parcelFileDescriptor.dup().detachFd()
        packCall("install", code) { r ->
            r.put("fd", fd)
            r.put("off", afd.startOffset)
            r.put("len", afd.length)
            r.put("name", asset)
        }
        afd.close()
    }

    // assets/packs/lang_xx of a personal build into filesDir/packs: once per pack id, then again only when the APK's
    // copy is newer than the installed one (`packNewer`). The `.bundled` marker lists the ids copied once, so a pack the
    // user removed stays removed until they install it again.
    private fun copyBundledPacks() {
        val ids = assets.list("packs") ?: return
        val dst = File(filesDir, "packs").also { it.mkdirs() }
        val marker = File(dst, ".bundled")
        val seen = runCatching { marker.readLines() }.getOrDefault(emptyList())
            .mapNotNull { it.trim().substringBefore(' ').takeIf(String::isNotEmpty) }.toMutableSet()
        for (id in ids) {
            val toml = runCatching { assets.open("packs/$id/pack.toml").bufferedReader().use { it.readText() } }
                .getOrNull() ?: continue
            val out = File(dst, id)
            val have = runCatching { File(out, "pack.toml").readText() }.getOrNull()
            val copy = when {
                have != null -> packNewer(toml, have)
                else -> id !in seen
            }
            seen.add(id)
            if (!copy) continue
            out.deleteRecursively()
            copyAssetDir("packs/$id", out)
        }
        if (marker.readLinesOrNull() != seen.sorted()) {
            runCatching { PortCfg.writeAtomic(marker, seen.sorted().joinToString("\n", postfix = "\n")) }
                .onFailure { Log.e("th08", "pack marker not saved: $it") }
        }
    }

    private fun File.readLinesOrNull(): List<String>? = runCatching { readLines().filter(String::isNotBlank) }.getOrNull()

    private fun copyAssetDir(path: String, out: File) {
        val children = assets.list(path).orEmpty()
        if (children.isEmpty()) {
            out.parentFile?.mkdirs()
            assets.open(path).use { i -> out.outputStream().use { i.copyTo(it) } }
            return
        }
        children.forEach { copyAssetDir("$path/$it", File(out, it)) }
    }

    override fun play() {
        val once = rows
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
