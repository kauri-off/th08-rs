// The launcher's colours: the night-sky palette of the desktop launcher and port menu (src/theme.rs), always dark.
package org.th08.port

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

private val Bg = Color(0xFF0A0B1E)
private val Surface = Color(0xFF141430)
private val Stripe = Color(0xFF1A1A3A)
private val Raised = Color(0xFF22224C)
private val RaisedHover = Color(0xFF2E2E62)
private val Outline = Color(0xFF7070AA)
private val Text = Color(0xFFECEAF8)
private val TextWeak = Color(0xFFB0AECE)
private val Accent = Color(0xFF6452DC)
private val Lavender = Color(0xFFB0A8FF)
private val OnLavender = Color(0xFF1A1448)
private val Focus = Color(0xFFF5D680)
private val Danger = Color(0xFFFF8A80)

private val Night = darkColorScheme(
    primary = Lavender,
    onPrimary = OnLavender,
    primaryContainer = Accent,
    onPrimaryContainer = Text,
    inversePrimary = Accent,
    secondary = TextWeak,
    onSecondary = Bg,
    secondaryContainer = Raised,
    onSecondaryContainer = Text,
    tertiary = Focus,
    onTertiary = Bg,
    tertiaryContainer = RaisedHover,
    onTertiaryContainer = Focus,
    background = Bg,
    onBackground = Text,
    surface = Bg,
    onSurface = Text,
    surfaceVariant = Raised,
    onSurfaceVariant = TextWeak,
    surfaceTint = Accent,
    surfaceDim = Bg,
    surfaceBright = RaisedHover,
    surfaceContainerLowest = Bg,
    surfaceContainerLow = Surface,
    surfaceContainer = Surface,
    surfaceContainerHigh = Stripe,
    surfaceContainerHighest = Raised,
    inverseSurface = Text,
    inverseOnSurface = Bg,
    error = Danger,
    onError = Bg,
    errorContainer = Color(0xFF4A1C2A),
    onErrorContainer = Color(0xFFFFD6D2),
    outline = Outline,
    outlineVariant = Color(0xFF3A3A6E),
    scrim = Color.Black,
)

@Composable
fun Th08Theme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = Night, content = content)
}
