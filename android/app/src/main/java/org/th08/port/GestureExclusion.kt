// System gesture exclusion rects of the game's NativeActivity, called from Rust (platform/gestures.rs).
package org.th08.port

import android.app.Activity
import android.graphics.Rect
import android.os.Build
import androidx.annotation.Keep

@Keep
object GestureExclusion {
    // `rects`: left, top, right, bottom per rect (window px); empty clears.
    @JvmStatic
    fun set(activity: Activity, rects: IntArray) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return
        val list = (0 until rects.size / 4).map { Rect(rects[4 * it], rects[4 * it + 1], rects[4 * it + 2], rects[4 * it + 3]) }
        activity.runOnUiThread { activity.window.decorView.systemGestureExclusionRects = list }
    }
}
