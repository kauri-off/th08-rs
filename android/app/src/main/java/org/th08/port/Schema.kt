// The settings table as exported by `settings_schema` (crates/th08-tools) into assets/settings_schema.json, with
// the ja / ru texts of schema v3 picked by the `language` setting.
package org.th08.port

import android.content.Context
import android.content.res.Resources
import org.json.JSONObject
import java.util.Locale

sealed interface Kind {
    data object Bool : Kind
    data class IntRange(val min: Long, val max: Long) : Kind
    data class Choice(val choices: List<String>) : Kind
    data class Text(val format: String) : Kind
}

// Launcher languages (settings.rs `language`); English is the table's own text.
val LANGS = listOf("ja", "en", "ru")

// The `language` value resolved: "auto" (or unknown) = the device locale's language if ja / ru, else en.
fun resolveLang(pref: String, device: Locale = Locale.getDefault()): String =
    pref.takeIf { it in LANGS } ?: device.language.takeIf { it == "ja" || it == "ru" } ?: "en"

data class Group(val id: String, val titles: Map<String, String>) {
    fun title(lang: String) = titles[lang] ?: titles.getValue("en")
}

// The display texts of an entry in one language (schema v3: the English fields, or the entry's "ja" / "ru" object).
data class EntryText(
    val label: String,
    // Short row text; `help` is the long text behind the info button.
    val summary: String,
    val help: String,
    // Number suffix ("%", " dp") and display words by value text ("" = unset), from settings.rs `unit` / `words`.
    val unit: String,
    val words: Map<String, String>,
)

data class Entry(
    val key: String,
    val texts: Map<String, EntryText>,
    val kind: Kind,
    val default: String,
    val group: String,
    // Shown only in the Developer card (settings.rs `Audience::Dev`).
    val developer: Boolean,
    // The settings table's platform list names "android".
    val android: Boolean,
    // Saved to port.cfg; false = launch.cfg, the next start only.
    val persist: Boolean,
) {
    fun text(lang: String) = texts[lang] ?: texts.getValue("en")
}

private fun JSONObject.entryText(): EntryText {
    val ws = getJSONArray("words")
    return EntryText(
        label = getString("label"),
        summary = getString("summary"),
        help = getString("help"),
        unit = getString("unit"),
        words = (0 until ws.length()).associate { i ->
            ws.getJSONObject(i).let { w -> w.getString("value") to w.getString("label") }
        },
    )
}

class Schema(val groups: List<Group>, val entries: List<Entry>) {
    companion object {
        fun load(ctx: Context): Schema {
            val root = JSONObject(ctx.assets.open("settings_schema.json").bufferedReader().use { it.readText() })
            // A language missing from the file falls back to English (Group.title / Entry.text).
            val translated = LANGS.filter { it != "en" }
            val gs = root.getJSONArray("groups")
            val groups = (0 until gs.length()).map {
                val g = gs.getJSONObject(it)
                Group(g.getString("id"), mapOf("en" to g.getString("title")) + translated.mapNotNull { l ->
                    g.optJSONObject(l)?.let { t -> l to t.getString("title") }
                })
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
                    texts = mapOf("en" to e.entryText()) + translated.mapNotNull { l ->
                        e.optJSONObject(l)?.let { t -> l to t.entryText() }
                    },
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

// Display text of a value in `lang`: the table's word, else On/Off, number + unit, the raw text; "" = "Not set".
fun Entry.display(v: String, lang: String, res: Resources): String {
    val t = text(lang)
    t.words[v]?.let { return it }
    return when {
        kind == Kind.Bool -> res.getString(if (v == "on") R.string.on else R.string.off)
        v.isEmpty() -> res.getString(R.string.not_set)
        kind is Kind.IntRange -> v + t.unit
        else -> v
    }
}

// The values a choice dialog lists: enum choices, or every value of an int range that has a word for each value.
fun Entry.options(): List<String>? {
    val unset = if (default.isEmpty()) listOf("") else emptyList()
    val words = text("en").words
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
fun intOk(kind: Kind.IntRange, v: String): Boolean {
    if (v.isEmpty()) return true
    val n = v.toDoubleOrNull() ?: return false
    return n >= kind.min && n <= kind.max
}

// The message for a value `intOk` rejects, in the launcher's language.
fun intError(kind: Kind.IntRange, v: String, res: Resources): String? = when {
    intOk(kind, v) -> null
    v.toDoubleOrNull() == null -> res.getString(R.string.expected_number)
    else -> res.getString(R.string.expected_range, kind.min, kind.max)
}
