// Save backup on the settings screen: the Rust zip export / import (libth08, saves.rs `bridge`) behind one JSON call,
// with the picked document copied through a file in the app cache (Storage Access Framework, no permission).
package org.th08.port

import org.json.JSONObject

object Saves {
    init {
        System.loadLibrary("th08")
    }

    // saves.rs `MAX_ZIP`.
    const val MAX_ZIP = 256L shl 20

    @JvmStatic
    external fun call(request: String): String
}

sealed interface SaveOutcome {
    data class Exported(val files: Int) : SaveOutcome
    data class Imported(val files: Int, val backup: String?) : SaveOutcome
    data class Failed(val export: Boolean, val error: String) : SaveOutcome
}

// `gameRunning`: the :game process is alive, so its files may still change; export and import wait.
data class SaveState(
    val gameRunning: Boolean = false,
    val busy: Boolean = false,
    val outcome: SaveOutcome? = null,
)

fun parseSaveReply(s: String, export: Boolean): SaveOutcome {
    val o = JSONObject(s)
    if (o.has("error")) return SaveOutcome.Failed(export, o.getString("error"))
    return if (export) {
        SaveOutcome.Exported(o.getInt("files"))
    } else {
        SaveOutcome.Imported(o.getInt("files"), if (o.isNull("backup")) null else o.getString("backup"))
    }
}
