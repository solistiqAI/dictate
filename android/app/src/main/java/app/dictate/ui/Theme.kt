package app.dictate.ui

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext

private val Light = lightColorScheme(
    primary = Color(0xFF2F6BDB),
    secondary = Color(0xFF4F5B66),
    background = Color(0xFFF7F9FA),
    surface = Color(0xFFF7F9FA),
)

private val Dark = darkColorScheme(
    primary = Color(0xFF9DBBFF),
    secondary = Color(0xFFBAC4CE),
    background = Color(0xFF15191C),
    surface = Color(0xFF15191C),
)

/** The phone's own colours (Android 12+), else Dictate's; [dynamic] off for screenshots. */
@Composable
fun DictateTheme(
    dark: Boolean = isSystemInDarkTheme(),
    dynamic: Boolean = true,
    content: @Composable () -> Unit,
) {
    val scheme = when {
        dynamic && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S ->
            if (dark) dynamicDarkColorScheme(LocalContext.current) else dynamicLightColorScheme(LocalContext.current)
        dark -> Dark
        else -> Light
    }
    MaterialTheme(colorScheme = scheme, content = content)
}
