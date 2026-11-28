//! Navigation icons the core Material icon set lacks.
package org.th08.port

import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import androidx.compose.ui.graphics.Color

private fun icon(name: String, block: androidx.compose.ui.graphics.vector.PathBuilder.() -> Unit) =
    ImageVector.Builder(name, 24.dp, 24.dp, 24f, 24f).path(fill = SolidColor(Color.Black), pathBuilder = block).build()

val ReplaysIcon: ImageVector by lazy {
    icon("Replays") {
        moveTo(12f, 2f); arcTo(10f, 10f, 0f, true, false, 12f, 22f); arcTo(10f, 10f, 0f, true, false, 12f, 2f); close()
        moveTo(12f, 4f); arcTo(8f, 8f, 0f, true, true, 12f, 20f); arcTo(8f, 8f, 0f, true, true, 12f, 4f); close()
        moveTo(10f, 7.5f); lineTo(10f, 16.5f); lineTo(16.5f, 12f); close()
    }
}

val StatsIcon: ImageVector by lazy {
    icon("Stats") {
        moveTo(4f, 13f); horizontalLineTo(8f); verticalLineTo(20f); horizontalLineTo(4f); close()
        moveTo(10f, 4f); horizontalLineTo(14f); verticalLineTo(20f); horizontalLineTo(10f); close()
        moveTo(16f, 9f); horizontalLineTo(20f); verticalLineTo(20f); horizontalLineTo(16f); close()
    }
}
