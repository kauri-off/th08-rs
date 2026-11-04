// Translation packs on the settings screen: the Rust importer (libth08, translations.rs `bridge`) behind one JSON call,
// and the state the screen shows.
package org.th08.port

import org.json.JSONObject

object Packs {
    init {
        System.loadLibrary("th08")
    }

    @JvmStatic
    external fun call(request: String): String
}

data class InstalledPack(val language: String, val title: String, val revision: String, val coverage: Double?,
                         val managed: Boolean)

data class ServerPack(val code: String, val title: String, val estimate: Double, val base: List<String>,
                      val revision: String)

sealed interface PackOutcome {
    data class Installed(val lang: String, val percent: Double) : PackOutcome
    data class Removed(val lang: String) : PackOutcome
    data class Failed(val lang: String, val error: String) : PackOutcome
}

data class PackState(
    // The running job: a language code, "" for the list, null when idle.
    val busy: String? = null,
    val done: Int = 0,
    val total: Int = 0,
    val building: Boolean = false,
    val line: String = "",
    val server: List<ServerPack>? = null,
    val outcome: PackOutcome? = null,
    val installed: List<InstalledPack> = emptyList(),
) {
    companion object {
        fun parse(s: String): PackState {
            val o = JSONObject(s)
            if (o.has("error")) return PackState(outcome = PackOutcome.Failed("", o.getString("error")))
            val inst = o.getJSONArray("installed")
            val server = o.optJSONArray("server")
            val out = o.optJSONObject("outcome")
            return PackState(
                busy = if (o.isNull("busy")) null else o.getString("busy"),
                done = o.getInt("done"),
                total = o.getInt("total"),
                building = o.getBoolean("building"),
                line = o.getString("line"),
                server = server?.let { a ->
                    (0 until a.length()).map { i ->
                        val p = a.getJSONObject(i)
                        val b = p.getJSONArray("base")
                        ServerPack(p.getString("code"), p.getString("title"), p.getDouble("estimate"),
                            (0 until b.length()).map(b::getString), p.getString("revision"))
                    }
                },
                outcome = out?.let {
                    when (it.getString("kind")) {
                        "installed" -> PackOutcome.Installed(it.getString("lang"), it.getDouble("percent"))
                        "removed" -> PackOutcome.Removed(it.getString("lang"))
                        else -> PackOutcome.Failed(it.getString("lang"), it.getString("error"))
                    }
                },
                installed = (0 until inst.length()).map { i ->
                    val p = inst.getJSONObject(i)
                    InstalledPack(p.getString("language"), p.getString("title"), p.getString("revision"),
                        if (p.isNull("coverage")) null else p.getDouble("coverage"), p.getBoolean("managed"))
                },
            )
        }
    }
}
