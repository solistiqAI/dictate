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

    /**
     * The floating button's place on the screen, in px from the left and from the top. It stays
     * there whatever the keyboard does. Unset (-1, null): not moved yet.
     */
    var buttonX: Int
        get() = prefs.getInt(BUTTON_X, -1)
        set(value) = prefs.edit().putInt(BUTTON_X, value).apply()

    var buttonY: Int?
        get() = if (prefs.contains(BUTTON_Y)) prefs.getInt(BUTTON_Y, 0) else null
        set(value) = prefs.edit().apply { if (value == null) remove(BUTTON_Y) else putInt(BUTTON_Y, value) }.apply()

    /** Incoming WhatsApp voice messages to text by themselves ([VoiceNoteListener]). */
    var voiceNotes: Boolean
        get() = prefs.getBoolean(VOICE_NOTES, false)
        set(value) = prefs.edit().putBoolean(VOICE_NOTES, value).apply()

    companion object {
        private const val VOICE_NOTES = "voice_notes"
        private const val START_WITH = "start_with"
        private const val BUTTON_X = "button_x"
        private const val BUTTON_Y = "button_y"
    }
}
