package app.dictate

/**
 * dictate-core through JNI (`android/rust/src/jni.rs`). Every call may block (loading the model
 * takes seconds, a long voice message longer): call from a background thread, as [Engine] does.
 * Errors come as [IllegalStateException] with the reason.
 *
 * ONNX Runtime (`libonnxruntime.so`) must be loaded before [load]; [Engine] does that.
 */
object Native {
    init {
        System.loadLibrary("dictate_android")
    }

    /** Loads the speech model in [modelDir] and the settings in [dataDir] (`config.toml`). */
    external fun load(modelDir: String, dataDir: String)

    external fun isLoaded(): Boolean

    /** Mono samples in [-1, 1] at [sampleRate] Hz to the finished text (dictionary, commands). */
    external fun transcribe(samples: FloatArray, sampleRate: Int): String

    /** After the dictionary changed. */
    external fun reloadSettings()

    /** The whole dictionary: the default words with the user's own additions and deletions. */
    external fun dictionary(dataDir: String): Array<String>

    /** Saves the whole word list (as `[dictionary] added/removed`, like the desktop app). */
    external fun setDictionary(dataDir: String, words: Array<String>)

    /** The model's folder name, e.g. "parakeet-ultra-int8". */
    external fun modelName(): String

    /** URL and file name, alternating, of each file of the model. */
    external fun modelFiles(): Array<String>

    external fun modelInstalled(modelDir: String): Boolean
}
