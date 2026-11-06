// In-app update: the release APK into the cache, SHA256SUMS and PackageManager checks, a PackageInstaller session.
package org.th08.port

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import java.io.File
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

sealed interface UpdateStep {
    data object Idle : UpdateStep
    data class Downloading(val done: Long, val total: Long) : UpdateStep
    data object Verifying : UpdateStep
    data object NeedPermission : UpdateStep
    data object Installing : UpdateStep
    // A string resource with an optional detail (`%1$s`), shown with the browser download as fallback.
    data class Failed(val msg: Int, val detail: String = "") : UpdateStep
    data class Updated(val version: String) : UpdateStep
}

private class Refused(val msg: Int, val detail: String = "") : Exception()

private class Cancelled : Exception()

object AppUpdate {
    var step by mutableStateOf<UpdateStep>(UpdateStep.Idle)
        private set

    private val main = Handler(Looper.getMainLooper())
    @Volatile private var cancelled = false
    @Volatile private var conn: HttpURLConnection? = null
    private var worker: Thread? = null
    private var ready: File? = null
    private var target: Pair<Long, String>? = null

    private const val SUMS_MAX = 1 shl 16
    private const val PENDING = "update_pending"

    val busy get() = worker?.isAlive == true || step is UpdateStep.Installing

    private fun dir(ctx: Context) = File(ctx.cacheDir, "update")

    private fun post(s: UpdateStep) = main.post { step = s }

    // Process start: drop what an earlier run left in the cache, and report an install that happened since.
    fun onStart(ctx: Context) {
        if (busy || step != UpdateStep.Idle) return
        dir(ctx).deleteRecursively()
        installed(ctx)
    }

    // The marker written before the commit, once this build is the one it names.
    private fun installed(ctx: Context) {
        val marker = File(ctx.filesDir, PENDING)
        val lines = runCatching { marker.readLines() }.getOrNull() ?: return
        marker.delete()
        val code = lines.getOrNull(0)?.toLongOrNull() ?: return
        if (BuildConfig.VERSION_CODE.toLong() >= code) step = UpdateStep.Updated(lines.getOrNull(1) ?: BuildConfig.VERSION_NAME)
    }

    fun fail(msg: Int) {
        if (!busy) step = UpdateStep.Failed(msg)
    }

    fun dismiss() {
        if (!busy) step = UpdateStep.Idle
    }

    fun start(ctx: Context, u: UpdateInfo, https: Boolean) {
        if (busy) return
        val app = ctx.applicationContext
        val asset = u.asset
        val sums = u.sums
        if (asset == null || sums == null) return run { step = UpdateStep.Failed(R.string.update_fail_sums) }
        cancelled = false
        step = UpdateStep.Downloading(0, -1)
        worker = Thread {
            val d = dir(app)
            val part = File(d, "$asset.part")
            try {
                d.deleteRecursively()
                d.mkdirs()
                val want = expectedSum(fetchText(sums, https, SUMS_MAX), asset)
                    ?: throw Refused(R.string.update_fail_sums)
                val got = download(u.download, https, part)
                post(UpdateStep.Verifying)
                if (!got.equals(want, ignoreCase = true)) throw Refused(R.string.update_fail_hash)
                val apk = File(d, "update.apk")
                if (!part.renameTo(apk)) throw IOException("rename failed")
                val code = verifyApk(app, apk)
                main.post {
                    if (cancelled) return@post run { d.deleteRecursively(); step = UpdateStep.Idle }
                    ready = apk
                    target = code to u.version
                    if (canInstall(app)) install(app) else step = UpdateStep.NeedPermission
                }
            } catch (e: Throwable) {
                d.deleteRecursively()
                post(when {
                    cancelled || e is Cancelled -> UpdateStep.Idle
                    e is Refused -> UpdateStep.Failed(e.msg, e.detail)
                    else -> UpdateStep.Failed(R.string.update_fail_network, e.message ?: e.toString())
                })
                if (e !is Cancelled && !cancelled) Log.w("th08", "update download: $e")
            } finally {
                conn = null
            }
        }.also { it.start() }
    }

    fun cancel(ctx: Context) {
        cancelled = true
        conn?.disconnect()
        if (step == UpdateStep.NeedPermission) {
            ready = null
            dir(ctx).deleteRecursively()
            step = UpdateStep.Idle
        }
    }

    fun canInstall(ctx: Context) = ctx.packageManager.canRequestPackageInstalls()

    // Back from the "install unknown apps" settings page.
    fun permissionResult(ctx: Context) {
        if (step != UpdateStep.NeedPermission) return
        if (canInstall(ctx)) return install(ctx.applicationContext)
        ready = null
        dir(ctx).deleteRecursively()
        step = UpdateStep.Failed(R.string.update_fail_permission)
    }

    private fun open(url: String, https: Boolean): HttpURLConnection {
        if (https && !url.startsWith("https://")) throw Refused(R.string.update_fail_network, "not https: $url")
        val c = URL(url).openConnection() as HttpURLConnection
        c.connectTimeout = 15_000
        c.readTimeout = 30_000
        c.setRequestProperty("User-Agent", "th08-rs-update")
        conn = c
        if (cancelled) throw Cancelled()
        val code = c.responseCode
        if (code != 200) throw IOException("HTTP $code")
        return c
    }

    private fun fetchText(url: String, https: Boolean, max: Int): String {
        val c = open(url, https)
        val out = java.io.ByteArrayOutputStream()
        c.inputStream.use { i ->
            val buf = ByteArray(8192)
            while (true) {
                val n = i.read(buf)
                if (n < 0) break
                out.write(buf, 0, n)
                if (out.size() > max) throw IOException("$url larger than $max bytes")
            }
        }
        return out.toString("UTF-8")
    }

    // Streams `url` into `out` with progress; the file's SHA-256 as hex.
    private fun download(url: String, https: Boolean, out: File): String {
        val c = open(url, https)
        val total = c.contentLengthLong
        val md = MessageDigest.getInstance("SHA-256")
        var done = 0L
        var shown = 0L
        c.inputStream.use { i ->
            out.outputStream().use { o ->
                val buf = ByteArray(1 shl 16)
                while (true) {
                    if (cancelled) throw Cancelled()
                    val n = i.read(buf)
                    if (n < 0) break
                    o.write(buf, 0, n)
                    md.update(buf, 0, n)
                    done += n
                    if (done - shown >= 1 shl 18) {
                        shown = done
                        post(UpdateStep.Downloading(done, total))
                    }
                }
            }
        }
        if (cancelled) throw Cancelled()
        if (total >= 0 && done != total) throw IOException("short download: $done of $total bytes")
        return hex(md.digest())
    }

    @Suppress("DEPRECATION")
    private fun signers(p: PackageInfo?, lineage: Boolean): Set<String> {
        p ?: return emptySet()
        val certs = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            val s = p.signingInfo ?: return emptySet()
            if (s.hasMultipleSigners() || !lineage) s.apkContentsSigners else s.signingCertificateHistory
        } else {
            p.signatures
        }
        return certs.orEmpty().map { sha256Hex(it.toByteArray()) }.toSet()
    }

    @Suppress("DEPRECATION")
    private fun versionCode(p: PackageInfo) =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) p.longVersionCode else p.versionCode.toLong()

    // The archive is this package, newer, same key; its versionCode.
    @Suppress("DEPRECATION")
    private fun verifyApk(ctx: Context, apk: File): Long {
        val pm = ctx.packageManager
        val flags = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) PackageManager.GET_SIGNING_CERTIFICATES
            else PackageManager.GET_SIGNATURES
        val mine = pm.getPackageInfo(ctx.packageName, flags)
        val theirs = pm.getPackageArchiveInfo(apk.path, flags) ?: throw Refused(R.string.update_fail_package)
        val code = versionCode(theirs)
        when (checkApk(ctx.packageName, versionCode(mine), signers(mine, false),
            theirs.packageName, code, signers(theirs, true))) {
            ApkProblem.PACKAGE -> throw Refused(R.string.update_fail_package)
            ApkProblem.VERSION -> throw Refused(R.string.update_fail_version)
            ApkProblem.SIGNER -> throw Refused(R.string.update_fail_signer)
            null -> return code
        }
    }

    private fun install(ctx: Context) {
        val apk = ready ?: return run { step = UpdateStep.Idle }
        val (code, version) = target ?: return
        ready = null
        step = UpdateStep.Installing
        Thread {
            try {
                val pi = ctx.packageManager.packageInstaller
                val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
                    setAppPackageName(ctx.packageName)
                    setSize(apk.length())
                }
                val id = pi.createSession(params)
                pi.openSession(id).use { s ->
                    s.openWrite("base.apk", 0, apk.length()).use { o ->
                        apk.inputStream().use { it.copyTo(o) }
                        s.fsync(o)
                    }
                    // Read at the next start: the installer ends this process when it replaces the app.
                    PortCfg.writeAtomic(File(ctx.filesDir, PENDING), "$code\n$version\n")
                    val flags = PendingIntent.FLAG_UPDATE_CURRENT or
                        (if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) PendingIntent.FLAG_MUTABLE else 0)
                    val result = PendingIntent.getBroadcast(ctx, 0,
                        Intent(ctx, UpdateReceiver::class.java).setPackage(ctx.packageName), flags)
                    s.commit(result.intentSender)
                }
            } catch (e: Throwable) {
                Log.w("th08", "update install: $e")
                File(ctx.filesDir, PENDING).delete()
                post(UpdateStep.Failed(R.string.update_fail_install, e.message ?: e.toString()))
            } finally {
                dir(ctx).deleteRecursively()
            }
        }.start()
    }

    internal fun result(ctx: Context, intent: Intent) {
        val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)
        val msg = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE).orEmpty()
        Log.i("th08", "update install status $status $msg")
        if (status == PackageInstaller.STATUS_PENDING_USER_ACTION) {
            @Suppress("DEPRECATION")
            val confirm = if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
                else intent.getParcelableExtra(Intent.EXTRA_INTENT)
            confirm ?: return run { step = UpdateStep.Failed(R.string.update_fail_install, msg) }
            runCatching { ctx.startActivity(confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
                .onFailure { step = UpdateStep.Failed(R.string.update_fail_install, it.toString()) }
            return
        }
        // Success usually reaches the new build's fresh process (the old one was ended), so it reads the marker.
        if (status == PackageInstaller.STATUS_SUCCESS) return installed(ctx)
        File(ctx.filesDir, PENDING).delete()
        step = when (status) {
            PackageInstaller.STATUS_FAILURE_ABORTED -> UpdateStep.Failed(R.string.update_cancelled)
            else -> UpdateStep.Failed(R.string.update_fail_install, msg.ifEmpty { "status $status" })
        }
    }
}

// The PackageInstaller session's status broadcasts.
class UpdateReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) = AppUpdate.result(ctx, intent)
}
