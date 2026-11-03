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

data class Group(val id: String, val title: String)

data class Entry(
    val key: String,
    val label: String,
    // Short row text; `help` is the long text behind the info button.
    val summary: String,
    val help: String,
    // Number suffix ("%", " dp") and display words by value text ("" = unset), from settings.rs `unit` / `words`.
    val unit: String,
    val words: Map<String, String>,
    val kind: Kind,
    val default: String,
    val group: String,
    // Shown only in the Developer card (settings.rs `Audience::Dev`).
    val developer: Boolean,
    // The settings table's platform list names "android".
    val android: Boolean,
    // Saved to port.cfg; false = launch.cfg, the next start only.
    val persist: Boolean,
)

class Schema(val groups: List<Group>, val entries: List<Entry>) {
    companion object {
        fun load(ctx: Context): Schema {
            val root = JSONObject(ctx.assets.open("settings_schema.json").bufferedReader().use { it.readText() })
            val gs = root.getJSONArray("groups")
            val groups = (0 until gs.length()).map {
                val g = gs.getJSONObject(it)
                Group(g.getString("id"), g.getString("title"))
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
                val ws = e.getJSONArray("words")
                val words = (0 until ws.length()).associate { i ->
                    ws.getJSONObject(i).let { w -> w.getString("value") to w.getString("label") }
                }
                Entry(
                    key = e.getString("key"),
                    label = e.getString("label"),
                    summary = e.getString("summary"),
                    help = e.getString("help"),
                    unit = e.getString("unit"),
                    words = words,
                    kind = kind,
                    default = norm(kind, e.getString("default")),
                    group = e.getString("group"),
                    developer = e.getString("audience") == "dev",
                    android = (0 until ps.length()).any { i -> ps.getString(i) == "android" },
                    persist = e.getBoolean("persist"),
                )
            }
            return Schema(groups, entries)
        }
    }
}

// Display text of a value: the table's word, else On/Off, number + unit, the raw text; "" = "Not set".
fun Entry.display(v: String): String {
    words[v]?.let { return it }
    return when {
        kind == Kind.Bool -> if (v == "on") "On" else "Off"
        v.isEmpty() -> "Not set"
        kind is Kind.IntRange -> v + unit
        else -> v
    }
}

// The values a choice dialog lists: enum choices, or every value of an int range that has a word for each value.
fun Entry.options(): List<String>? {
    val unset = if (default.isEmpty()) listOf("") else emptyList()
    return when (val k = kind) {
        is Kind.Choice -> unset + k.choices
        is Kind.IntRange if k.max - k.min < 8 && (k.min..k.max).all { words.containsKey(it.toString()) } ->
            unset + (k.min..k.max).map { it.toString() }
        else -> null
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
