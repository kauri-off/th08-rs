// port.cfg in filesDir, read and written like settings.rs (`parse_port_cfg`, `set_cfg_line`): `key = value` lines,
// comments, blank and unknown lines kept; plus launch.cfg, the one-start settings the game reads and deletes (lib.rs).
package org.th08.port

import java.io.File

object PortCfg {
    // Last value per key wins, as the Rust reader applies the lines in order.
    fun read(file: File): Map<String, String> {
        val text = runCatching { file.readText() }.getOrDefault("")
        val out = LinkedHashMap<String, String>()
        for (raw in text.lines()) {
            val l = raw.trim()
            if (l.isEmpty() || l.startsWith("#")) continue
            val i = l.indexOf('=')
            if (i < 0) continue
            out[l.substring(0, i).trim()] = l.substring(i + 1).trim()
        }
        return out
    }

    // `key = value` replaced in place (first match; later duplicates dropped), appended if new, removed for null.
    fun setLine(old: String, key: String, value: String?): String {
        var found = false
        val out = ArrayList<String>()
        for (l in if (old.isEmpty()) emptyList() else old.removeSuffix("\n").split("\n")) {
            val i = l.indexOf('=')
            val isKey = !l.trimStart().startsWith("#") && i >= 0 && l.substring(0, i).trim() == key
            if (!isKey) {
                out.add(l)
            } else if (value != null && !found) {
                found = true
                out.add("$key = $value")
            }
        }
        if (!found && value != null) out.add("$key = $value")
        if (out.isEmpty()) return ""
        return out.joinToString("\n") + "\n"
    }

    fun write(file: File, key: String, value: String?) {
        val old = runCatching { file.readText() }.getOrDefault("")
        val text = setLine(old, key, value)
        if (text != old) file.writeText(text)
    }

    fun writeLaunch(file: File, values: List<Pair<String, String>>) {
        file.writeText(values.joinToString("") { (k, v) -> "$k = $v\n" })
    }
}
