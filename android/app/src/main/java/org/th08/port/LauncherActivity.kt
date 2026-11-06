// Android settings screen (MATERIAL2): the Compose screen of LauncherUi.kt built from the settings table, Play starts
// the Rust NativeActivity in the ":game" process. Player rows are saved to port.cfg on change (as the egui launcher's
// `Model::changes`) when the table marks them `persist`; the other rows go to launch.cfg for the next start only.
// Texts follow the `language` row (docs/arch/I18N.md §3.3).
package org.th08.port

import android.app.ActivityManager
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
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
    private var data by mutableStateOf<GameData>(GameData.Missing)
    private val importer = GameImporter(this)
    private var picking: Setting? = null
    private var packs by mutableStateOf(PackState())
    private var saves by mutableStateOf(SaveState())
    private var exportDoc: (String) -> Unit = {}
    private var importDoc: () -> Unit = {}
    private var importCfg = false
    private val lastErrorFile get() = File(filesDir, "last_error.txt")
    private var lastError by mutableStateOf<String?>(null)
    private var update by mutableStateOf<UpdateInfo?>(null)

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
        Thread {
            val failed = runCatching { copyBundledPacks() }.onFailure { Log.e("th08", "bundled packs: $it") }
                .getOrDefault(emptyList())
            runOnUiThread {
                packCall("status")
                failed.firstOrNull()?.let { (id, e) -> packs = packs.copy(outcome = PackOutcome.Failed(id, copyError(e))) }
            }
        }.start()
        AppUpdate.onStart(this)
        checkUpdate()
        // A job started before a configuration change keeps running in this process.
        importer.poll()
        setContent {
            Th08Theme {
                LifecycleResumeEffect(Unit) {
                    data = checkData()
                    saves = saves.copy(gameRunning = gameRunning())
                    lastError = runCatching { lastErrorFile.readText() }.getOrNull()?.takeIf { it.isNotBlank() }
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
                        val busy = packs.busy ?: ""
                        packs = withContext(Dispatchers.Default) { runCatching { PackState.parse(Packs.call(request("status"))) } }
                            .getOrElse { PackState(outcome = PackOutcome.Failed(busy, it.message ?: it.toString())) }
                    }
                }
                LaunchedEffect(importer.state.busy) {
                    if (!importer.state.busy) return@LaunchedEffect
                    while (importer.state.busy) {
                        delay(250)
                        withContext(Dispatchers.Default) { importer.poll() }
                    }
                    importer.logOutcome()
                    data = checkData()
                }
                val pickGame = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
                    importer.start(uris)
                }
                openGamePicker = { pickGame.launch(arrayOf("*/*")) }
                val pick = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
                    val r = picking ?: return@rememberLauncherForActivityResult
                    picking = null
                    uri?.let { copyPicked(it) }?.let { change(r, it.path) }
                }
                pickFile = { r -> picking = r; pick.launch(arrayOf("*/*")) }
                val allow = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
                    AppUpdate.permissionResult(this)
                }
                openInstallSettings = {
                    allow.launch(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:$packageName")))
                }
                // The `language` row picks the texts and switches the screen live; the game reads the same value from
                // port.cfg (auto: its own Locale.getDefault(), i18n.rs `os_locale`).
                val lang = currentLang()
                val res = remember(lang) { localized(this, lang) }
                CompositionLocalProvider(LocalLang provides lang, LocalStrings provides res, LocalPacks provides packs,
                    LocalSaves provides saves, LocalImport provides importer.state, LocalLastError provides lastError,
                    LocalUpdate provides UpdateUi(update, AppUpdate.step, inAppUpdate)) {
                    LauncherScreen(schema.groups, rows, data, this)
                }
            }
        }
    }

    private var pickFile: (Setting) -> Unit = {}
    private var openGamePicker: () -> Unit = {}
    private var openInstallSettings: () -> Unit = {}
    private val inAppUpdate get() = inAppUpdateAllowed(BuildConfig.CHANNEL, File(filesDir, "update_test.json").isFile)

    // As android_main: imported files, then bundled assets, then the old <external files>/game (adb push target).
    private fun checkData(): GameData {
        val game = importDir(this)
        if (hasFile(game, "th08.pak", "th08.dat")) return GameData.Imported(game, hasFile(game, "thbgm.pak", "thbgm.dat"))
        if (assets.list("")?.any { it == "th08.pak" || it == "th08.dat" } == true) return GameData.Bundled
        val old = getExternalFilesDir(null)?.let { File(it, "game") }
        if (old != null && hasFile(old, "th08.pak", "th08.dat")) return GameData.Folder(old)
        return GameData.Missing
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

    override fun importGame() = openGamePicker()

    override fun cancelImport() = importer.cancel()

    // remove_dir_all of ~75 MB: off the UI thread.
    override fun removeGame() {
        Thread {
            val st = importer.removeState()
            runOnUiThread {
                importer.state = st
                data = checkData()
            }
        }.start()
    }

    private fun request(op: String, code: String = ""): String = JSONObject().apply {
        put("op", op)
        put("config", filesDir.path)
        put("code", code)
        data.folder()?.let { put("dir", it.path) }
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
    // Returns the packs whose copy failed; their partial copy is deleted.
    private fun copyBundledPacks(): List<Pair<String, Throwable>> {
        val failed = mutableListOf<Pair<String, Throwable>>()
        val ids = assets.list("packs") ?: return failed
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
            if (copy) {
                // A failed copy (full disk) leaves the id unseen, so the next start repairs it.
                val r = runCatching { out.deleteRecursively(); copyAssetDir("packs/$id", out) }
                val e = r.exceptionOrNull()
                if (e != null) {
                    Log.e("th08", "bundled pack $id not copied: $e")
                    // A half-copied pack would pass as installed (its pack.toml may be there): remove it.
                    out.deleteRecursively()
                    failed.add(id to e)
                    continue
                }
            }
            seen.add(id)
        }
        if (marker.readLinesOrNull() != seen.sorted()) {
            runCatching { PortCfg.writeAtomic(marker, seen.sorted().joinToString("\n", postfix = "\n")) }
                .onFailure { Log.e("th08", "pack marker not saved: $it") }
        }
        return failed
    }

    private fun copyError(e: Throwable): String {
        val m = e.message ?: e.toString()
        return if ("ENOSPC" in m || "No space" in m) getString(R.string.import_no_space) else m
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

    // Once per launcher start, off the UI thread; silent when off, offline or nothing newer.
    private fun checkUpdate() {
        if (rows.firstOrNull { it.e.key == "update_check" }?.value == "off") return
        val req = JSONObject().put("config", filesDir.path).toString()
        Thread {
            val info = runCatching { parseUpdateReply(UpdateCheck.call(req)) }.getOrNull()
            if (info != null) runOnUiThread { update = info }
        }.start()
    }

    override fun openUrl(url: String) {
        runCatching { startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))) }
            .onFailure { Log.w("th08", "cannot open $url: $it") }
    }

    // Launcher only, never with the game process alive: the install replaces the app and ends its processes.
    override fun updateStart() {
        val u = update ?: return
        if (!inAppUpdate) return
        if (gameRunning()) return run { AppUpdate.fail(R.string.update_game_running) }
        AppUpdate.start(this, u, https = BuildConfig.CHANNEL == "release")
    }

    override fun updateCancel() = AppUpdate.cancel(this)

    override fun updatePermission() = openInstallSettings()

    override fun updateDismiss() = AppUpdate.dismiss()

    override fun shareError() {
        val text = lastError ?: return
        val send = Intent(Intent.ACTION_SEND).apply {
            type = "text/plain"
            putExtra(Intent.EXTRA_SUBJECT, localized(this@LauncherActivity, currentLang()).getString(R.string.last_failed_subject))
            putExtra(Intent.EXTRA_TEXT, text)
        }
        startActivity(Intent.createChooser(send, null))
    }

    override fun dismissError() {
        lastErrorFile.delete()
        lastError = null
    }

    private fun currentLang() = resolveLang(rows.firstOrNull { it.e.key == "language" }?.value ?: "auto")

    override fun play() {
        dismissError()
        if (AppUpdate.busy || AppUpdate.step == UpdateStep.NeedPermission) AppUpdate.cancel(this)
        val once = rows
            .filter { !it.e.persist && it.value != it.initial && it.value.isNotEmpty() && it.ok() }
            .map { it.e.key to it.value }
        PortCfg.writeLaunch(File(filesDir, "launch.cfg"), once)
        startActivity(Intent(this, GameActivity::class.java))
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
