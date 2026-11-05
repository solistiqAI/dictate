package app.dictate

import android.Manifest
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.provider.Settings
import android.view.inputmethod.InputMethodManager

/** The checks and jumps the app screen offers. */
object Setup {
    fun micGranted(context: Context): Boolean =
        context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    fun keyboardEnabled(context: Context): Boolean =
        context.getSystemService(InputMethodManager::class.java)
            ?.enabledInputMethodList?.any { it.packageName == context.packageName } == true

    fun keyboardChosen(context: Context): Boolean =
        Settings.Secure.getString(context.contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD)
            ?.startsWith(context.packageName + "/") == true

    /** The system's list of keyboards, where Dictate is turned on. */
    fun openKeyboardSettings(context: Context) {
        context.startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }

    fun pickKeyboard(context: Context) {
        context.getSystemService(InputMethodManager::class.java)?.showInputMethodPicker()
    }

    fun copy(context: Context, text: String) {
        context.getSystemService(ClipboardManager::class.java)
            ?.setPrimaryClip(ClipData.newPlainText(context.getString(R.string.app_name), text))
    }

    fun share(context: Context, text: String) {
        val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text)
        context.startActivity(Intent.createChooser(send, null).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }
}
