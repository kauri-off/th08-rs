// Launcher update check: the Rust side (libth08, update.rs) asks GitHub at most once a day in the release channel.
package org.th08.port

import org.json.JSONObject

object UpdateCheck {
    init {
        System.loadLibrary("th08")
    }

    @JvmStatic
    external fun call(request: String): String
}

data class UpdateInfo(val version: String, val page: String, val download: String)

// `{}` (no newer release, off, offline) or garbage → null.
fun parseUpdateReply(s: String): UpdateInfo? = runCatching {
    val o = JSONObject(s)
    if (!o.has("version")) null else UpdateInfo(o.getString("version"), o.getString("page"), o.getString("download"))
}.getOrNull()
