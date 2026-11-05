package app.dictate

import android.Manifest
import android.accessibilityservice.AccessibilityServiceInfo
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import android.view.accessibility.AccessibilityManager
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

    /** The floating button's accessibility service is on. */
    fun buttonServiceOn(context: Context): Boolean =
        context.getSystemService(AccessibilityManager::class.java)
            ?.getEnabledAccessibilityServiceList(AccessibilityServiceInfo.FEEDBACK_ALL_MASK)
            ?.any { it.resolveInfo.serviceInfo.packageName == context.packageName } == true

    fun openAccessibilitySettings(context: Context) {
        context.startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }

    /** App info, where Android 13+ hides "Allow restricted settings" (⋮) for apps not from a store. */
    fun openAppInfo(context: Context) {
        context.startActivity(
            Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", context.packageName, null))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
    }

    /**
     * The input method another keyboard's mic key opens (AOSP keyboards, HeliBoard, FlorisBoard ask
     * the system for its "shortcut" input method: an enabled one with a voice subtype; a system app's
     * wins). Its name, or null when there is none.
     */
    fun micKeyOpens(context: Context): Pair<String, Boolean>? {
        val imm = context.getSystemService(InputMethodManager::class.java) ?: return null
        val info = imm.shortcutInputMethodsAndSubtypes.keys.firstOrNull() ?: return null
        return info.loadLabel(context.packageManager).toString() to (info.packageName == context.packageName)
    }

    /** HeliBoard's releases (the keyboard the mic-key path is made for). */
    fun openHeliBoard(context: Context) {
        context.startActivity(
            Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/Helium314/HeliBoard/releases"))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
    }

    /** The system's list of keyboards, where Dictate is turned on. */
    fun openKeyboardSettings(context: Context) {
        context.startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
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
