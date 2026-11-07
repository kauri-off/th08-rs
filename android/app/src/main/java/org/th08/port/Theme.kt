// The launcher's look: the lacquer palette and serif face of the desktop launcher and port menu (src/theme.rs), always dark.
package org.th08.port

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

object Lacquer {
    val Bg = Color(0xFF0E0809)
    val Panel = Color(0xFF170C0D)
    val Edge = Color(0xFF5A4038)
    val EdgeIn = Color(0xFF33221F)
    val Rule = Color(0xCC5A4038)
    val Band = Color(0xFF2C181B)
    val Pressed = Color(0xFF3A2226)
    val Cell = Color(0xFF221214)
    val TextHi = Color(0xFFFCFCFC)
    val Text = Color(0xFFC4BCCB)
    val Help = Color(0xFF9A93A6)
    val HelpHi = Color(0xFFB6AEBF)
    val Note = Color(0xFF8C8698)
    val Dim = Color(0xFF6C6474)
    val Disabled = Color(0xFF5F5866)
    val Accent = Color(0xFFCAC2D1)
    val Glow = Color(0xFF3C0030)
    val Fill = Color(0xFF966CA8)
    val Danger = Color(0xFFECC0C3)
}

// The game's lettering: a dark drop shadow under every serif word.
val Lettering = Shadow(Color(0xE6000000), Offset(0f, 2f), 3f)

// Noto Serif CJK JP SemiBold, cut to the UI tables (assets/fonts, from i18n/fonts).
val LocalSerif = staticCompositionLocalOf<FontFamily> { FontFamily.Serif }

private val Night = darkColorScheme(
    primary = Lacquer.Accent,
    onPrimary = Lacquer.Panel,
    primaryContainer = Lacquer.Band,
    onPrimaryContainer = Lacquer.TextHi,
    inversePrimary = Lacquer.Fill,
    secondary = Lacquer.Help,
    onSecondary = Lacquer.Panel,
    secondaryContainer = Lacquer.Cell,
    onSecondaryContainer = Lacquer.Text,
    tertiary = Lacquer.Accent,
    onTertiary = Lacquer.Panel,
    tertiaryContainer = Lacquer.Pressed,
    onTertiaryContainer = Lacquer.TextHi,
    background = Lacquer.Panel,
    onBackground = Lacquer.Text,
    surface = Lacquer.Panel,
    onSurface = Lacquer.Text,
    surfaceVariant = Lacquer.Cell,
    onSurfaceVariant = Lacquer.Help,
    surfaceTint = Color.Transparent,
    surfaceDim = Lacquer.Bg,
    surfaceBright = Lacquer.Pressed,
    surfaceContainerLowest = Lacquer.Bg,
    surfaceContainerLow = Lacquer.Panel,
    surfaceContainer = Lacquer.Panel,
    surfaceContainerHigh = Lacquer.Cell,
    surfaceContainerHighest = Lacquer.Cell,
    inverseSurface = Lacquer.Text,
    inverseOnSurface = Lacquer.Panel,
    error = Lacquer.Danger,
    onError = Lacquer.Panel,
    errorContainer = Lacquer.Pressed,
    onErrorContainer = Lacquer.Danger,
    outline = Lacquer.Edge,
    outlineVariant = Lacquer.EdgeIn,
    scrim = Color.Black,
)

private fun type(serif: FontFamily): Typography {
    val s = { size: Int, w: FontWeight -> TextStyle(fontFamily = serif, fontWeight = w, fontSize = size.sp, shadow = Lettering) }
    val plain = { size: Int -> TextStyle(fontSize = size.sp, lineHeight = (size * 1.45).sp) }
    return Typography(
        displaySmall = s(30, FontWeight.SemiBold),
        headlineMedium = s(24, FontWeight.SemiBold),
        headlineSmall = s(22, FontWeight.SemiBold),
        titleLarge = s(20, FontWeight.SemiBold),
        titleMedium = s(17, FontWeight.SemiBold),
        titleSmall = s(13, FontWeight.SemiBold),
        bodyLarge = s(16, FontWeight.SemiBold),
        bodyMedium = plain(13),
        bodySmall = plain(12),
        labelLarge = s(15, FontWeight.SemiBold),
        labelMedium = plain(12),
        labelSmall = plain(11),
    )
}

@Composable
fun Th08Theme(content: @Composable () -> Unit) {
    val assets = LocalContext.current.assets
    val serif = remember {
        runCatching { FontFamily(Font("fonts/NotoSerifJP-ui.otf", assets, FontWeight.SemiBold)) }
            .getOrDefault(FontFamily.Serif)
    }
    val square = RoundedCornerShape(0.dp)
    MaterialTheme(
        colorScheme = Night,
        typography = remember(serif) { type(serif) },
        shapes = Shapes(square, square, square, square, square),
    ) {
        CompositionLocalProvider(LocalSerif provides serif, content = content)
    }
}
