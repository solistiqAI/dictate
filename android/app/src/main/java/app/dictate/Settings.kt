package app.dictate

import android.content.Context

/** How a recording starts: the floating button (any keyboard, Gboard too), the mic key of the user's keyboard, or both. */
enum class StartWith(val button: Boolean, val keyboard: Boolean) {
    BUTTON(true, false),
    KEYBOARD(false, true),
    BOTH(true, true),
}

/** The app's own small settings (the dictionary lives in config.toml, shared with the desktop format). */
class Settings(context: Context) {
    private val prefs = context.applicationContext.getSharedPreferences("settings", Context.MODE_PRIVATE)

    var startWith: StartWith
        get() = StartWith.entries.find { it.name == prefs.getString(START_WITH, null) } ?: StartWith.BUTTON
        set(value) = prefs.edit().putString(START_WITH, value.name).apply()

    /** The floating button's place: from the left edge, and above the keyboard (or the screen's bottom), in px. */
    var buttonX: Int
        get() = prefs.getInt(BUTTON_X, -1)
        set(value) = prefs.edit().putInt(BUTTON_X, value).apply()

    var buttonAbove: Int
        get() = prefs.getInt(BUTTON_ABOVE, -1)
        set(value) = prefs.edit().putInt(BUTTON_ABOVE, value).apply()

    companion object {
        private const val START_WITH = "start_with"
        private const val BUTTON_X = "button_x"
        private const val BUTTON_ABOVE = "button_above"
    }
}
