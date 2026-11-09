//! Debug builds: writes every shown text's mid-word line breaks and cut lines to cache/text_audit.json for ui_matrix.py.
package org.th08.port

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.icu.text.BreakIterator
import android.icu.util.ULocale
import android.view.View
import android.view.ViewGroup
import android.view.inspector.WindowInspector
import androidx.compose.ui.platform.ViewRootForTest
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.text.TextLayoutResult
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

class TextAuditReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        val broken = JSONArray()
        val cut = JSONArray()
        WindowInspector.getGlobalWindowViews().flatMap { composeRoots(it) }.forEach { root ->
            walk(root.semanticsOwner.unmergedRootSemanticsNode, broken, cut)
        }
        File(ctx.cacheDir, "text_audit.json").writeText(JSONObject().put("broken", broken).put("cut", cut).toString())
    }

    private fun composeRoots(v: View): List<ViewRootForTest> = when {
        v is ViewRootForTest -> listOf(v)
        v is ViewGroup -> (0 until v.childCount).flatMap { composeRoots(v.getChildAt(it)) }
        else -> emptyList()
    }

    private fun walk(n: SemanticsNode, broken: JSONArray, cut: JSONArray) {
        n.config.getOrNull(SemanticsActions.GetTextLayoutResult)?.action?.let { get ->
            val out = mutableListOf<TextLayoutResult>()
            get(out)
            val tab = n.parent?.config?.getOrNull(SemanticsProperties.Role) == Role.Tab
            out.forEach { audit(it, n.size.width, tab, broken, cut) }
        }
        n.children.forEach { walk(it, broken, cut) }
    }

    private fun audit(r: TextLayoutResult, shown: Int, tab: Boolean, broken: JSONArray, cut: JSONArray) {
        val t = r.layoutInput.text.text
        if (t.isEmpty()) return
        val last = r.lineCount - 1
        if (!tab && (r.isLineEllipsized(last) || r.getLineRight(last) - r.getLineLeft(last) > shown + 4f)) cut.put(t)
        val words = BreakIterator.getWordInstance(ULocale.ROOT).apply { setText(t) }
        for (l in 0 until r.lineCount - 1) {
            val k = r.getLineEnd(l)
            if (k in 1 until t.length && t[k - 1].isLetterOrDigit() && t[k].isLetterOrDigit() && !words.isBoundary(k) && !tooWideName(r, t, k, shown, words)) {
                broken.put(t.substring(0, k) + "|" + t.substring(k))
            }
        }
    }

    // A spell card name is game text: a single token wider than the row cannot be kept whole.
    private fun tooWideName(r: TextLayoutResult, t: String, k: Int, shown: Int, words: BreakIterator): Boolean {
        if ('「' !in t) return false
        val end = words.following(k)
        val start = words.previous()
        return (start until end).sumOf { r.getBoundingBox(it).width.toDouble() } > shown
    }
}
