package app.dictate

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import java.io.File

/**
 * The offline build's way to the speech model: copy its files from a folder the user picked
 * (e.g. `Download/parakeet-ultra-int8`, copied over USB from a computer). Each file arrives as
 * `<name>.part` and gets its name when all are there, as with the download.
 */
object ModelImport {
    /** Blocks (670 MB): not on the main thread. The names missing in the folder; empty: imported. */
    fun from(context: Context, tree: Uri): List<String> {
        val names = ModelDownload(context).parts().map { it.name }
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
        val found = HashMap<String, Uri>()
        context.contentResolver.query(
            children,
            arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID, DocumentsContract.Document.COLUMN_DISPLAY_NAME),
            null,
            null,
            null,
        )?.use { cursor ->
            while (cursor.moveToNext()) {
                val name = cursor.getString(1)
                if (name in names) found[name] = DocumentsContract.buildDocumentUriUsingTree(tree, cursor.getString(0))
            }
        }
        val missing = names.filter { it !in found }
        if (missing.isNotEmpty()) return missing
        val folder = AppFiles(context).model.also { it.mkdirs() }
        for (name in names) {
            val part = File(folder, name + ModelDownload.PART)
            val input = context.contentResolver.openInputStream(found.getValue(name))
                ?: throw IllegalStateException("cannot read $name")
            input.use { source -> part.outputStream().use { source.copyTo(it, 1 shl 20) } }
        }
        for (name in names) {
            val target = File(folder, name)
            target.delete()
            File(folder, name + ModelDownload.PART).renameTo(target)
        }
        return emptyList()
    }
}
