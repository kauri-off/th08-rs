// The settings table as exported by `settings_schema` (crates/th08-tools) into assets/settings_schema.json.
package org.th08.port

import android.content.Context
import org.json.JSONObject

sealed interface Kind {
    data object Bool : Kind
    data class IntRange(val min: Long, val max: Long) : Kind
    data class Choice(val choices: List<String>) : Kind
    data class Text(val format: String) : Kind
}

data class Group(val id: String, val title: String, val developer: Boolean)

data class Entry(
    val key: String,
    val label: String,
    val help: String,
    val kind: Kind,
    val default: String,
    val group: String,
    val android: Boolean,
)

class Schema(val groups: List<Group>, val entries: List<Entry>) {
    companion object {
        fun load(ctx: Context): Schema {
            val root = JSONObject(ctx.assets.open("settings_schema.json").bufferedReader().use { it.readText() })
            val gs = root.getJSONArray("groups")
            val groups = (0 until gs.length()).map {
                val g = gs.getJSONObject(it)
                Group(g.getString("id"), g.getString("title"), g.getBoolean("developer"))
            }
            val es = root.getJSONArray("entries")
            val entries = (0 until es.length()).map {
                val e = es.getJSONObject(it)
                val k = e.getJSONObject("kind")
                val kind = when (k.getString("type")) {
                    "bool" -> Kind.Bool
                    "int" -> Kind.IntRange(k.getLong("min"), k.getLong("max"))
                    "enum" -> k.getJSONArray("choices").let { c -> Kind.Choice((0 until c.length()).map(c::getString)) }
                    "text" -> Kind.Text(k.getString("format"))
                    else -> Kind.Text("")
                }
                val ps = e.getJSONArray("platforms")
                Entry(
                    key = e.getString("key"),
                    label = e.getString("label"),
                    help = e.getString("help"),
                    kind = kind,
                    default = norm(kind, e.getString("default")),
                    group = e.getString("group"),
                    android = (0 until ps.length()).any { i -> ps.getString(i) == "android" },
                )
            }
            return Schema(groups, entries)
        }
    }
}

// Same value text as the Rust launcher (launcher.rs `norm`): booleans "on"/"off", others trimmed.
fun norm(kind: Kind, v: String): String {
    if (kind != Kind.Bool) return v.trim()
    return when (v.trim().lowercase()) {
        "", "1", "true", "on", "yes" -> "on"
        "0", "false", "off", "no" -> "off"
        else -> v.trim()
    }
}

// Int check as settings.rs `Entry::check` does it before the entry's own parser; "" = unset.
fun intError(kind: Kind.IntRange, v: String): String? {
    if (v.isEmpty()) return null
    val n = v.toDoubleOrNull() ?: return "expected a number"
    return if (n < kind.min || n > kind.max) "expected ${kind.min}-${kind.max}" else null
}
