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
        val layouts = JSONArray().takeIf { intent.getBooleanExtra("geometry", false) }
        WindowInspector.getGlobalWindowViews().flatMap { composeRoots(it) }.forEach { root ->
            walk(root.semanticsOwner.unmergedRootSemanticsNode, broken, cut, layouts)
        }
        val result = JSONObject().put("broken", broken).put("cut", cut)
        if (layouts != null) result.put("layouts", layouts)
        File(ctx.cacheDir, "text_audit.json").writeText(result.toString())
    }

    private fun composeRoots(v: View): List<ViewRootForTest> = when {
        v is ViewRootForTest -> listOf(v)
        v is ViewGroup -> (0 until v.childCount).flatMap { composeRoots(v.getChildAt(it)) }
        else -> emptyList()
    }

    private fun walk(n: SemanticsNode, broken: JSONArray, cut: JSONArray, layouts: JSONArray?) {
        n.config.getOrNull(SemanticsActions.GetTextLayoutResult)?.action?.let { get ->
            val out = mutableListOf<TextLayoutResult>()
            get(out)
            val tab = n.parent?.config?.getOrNull(SemanticsProperties.Role) == Role.Tab
            out.forEach { audit(it, n, tab, broken, cut, layouts) }
        }
        n.children.forEach { walk(it, broken, cut, layouts) }
    }

    private fun audit(r: TextLayoutResult, n: SemanticsNode, tab: Boolean, broken: JSONArray, cut: JSONArray, layouts: JSONArray?) {
        val t = r.layoutInput.text.text
        if (t.isEmpty() || r.lineCount == 0) return
        val shown = minOf(n.size.width, r.size.width)
        val last = r.lineCount - 1
        val intrinsic = r.multiParagraph.intrinsics.maxIntrinsicWidth
        val clipped = r.multiParagraph.didExceedMaxLines || r.multiParagraph.height > r.size.height + 1f ||
            (!r.layoutInput.softWrap && intrinsic > shown + 1f) ||
            r.getLineEnd(last) < t.trimEnd().length ||
            (0..last).any { r.isLineEllipsized(it) || r.getLineRight(it) - r.getLineLeft(it) > shown + 2f }
        if (!tab && clipped) cut.put(t)
        if (layouts != null) {
            val origin = n.positionInWindow
            val lines = JSONArray()
            for (line in 0..last) {
                lines.put(JSONObject()
                    .put("text", clean(t.substring(r.getLineStart(line), r.getLineEnd(line, visibleEnd = true))))
                    .put("right", origin.x + r.getLineRight(line))
                    .put("baseline", origin.y + r.getLineBaseline(line)))
            }
            layouts.put(JSONObject().put("text", clean(t)).put("width", shown).put("intrinsic", intrinsic)
                .put("clipped", clipped).put("lines", lines))
        }
        val raw = clean(t)
        val locale = if (raw == t) ULocale.ROOT else ULocale.JAPANESE
        val words = BreakIterator.getWordInstance(locale).apply { setText(raw) }
        for (l in 0 until r.lineCount - 1) {
            val k = clean(t.take(r.getLineEnd(l))).length
            if (k in 1 until raw.length && raw[k - 1].isLetterOrDigit() && raw[k].isLetterOrDigit() && !words.isBoundary(k) &&
                (raw != t || !tooWideName(r, raw, k, shown, words))) {
                broken.put(raw.substring(0, k) + "|" + raw.substring(k))
            }
        }
    }

    private fun clean(t: String) = t.filterNot { it == '\u200b' || it == '\u2060' }

    // A spell card name is game text: a single token wider than the row cannot be kept whole.
    private fun tooWideName(r: TextLayoutResult, t: String, k: Int, shown: Int, words: BreakIterator): Boolean {
        if ('「' !in t) return false
        val end = words.following(k)
        val start = words.previous()
        return (start until end).sumOf { r.getBoundingBox(it).width.toDouble() } > shown
    }
}
