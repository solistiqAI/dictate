package app.dictate.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Share
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.InputChip
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import app.dictate.R
import app.dictate.StartWith
import app.dictate.Transcript
import app.dictate.formatSize
import java.text.DateFormat
import java.util.Date
import java.util.Locale
import kotlin.math.roundToInt

/** What the app screen's buttons do; the activity fills it in, previews leave it empty. */
class MainActions(
    val allowMic: () -> Unit = {},
    val download: () -> Unit = {},
    val cancelDownload: () -> Unit = {},
    val enableKeyboard: () -> Unit = {},
    val startWith: (StartWith) -> Unit = {},
    val openAccessibility: () -> Unit = {},
    val openAppInfo: () -> Unit = {},
    val getHeliBoard: () -> Unit = {},
    val copy: (String) -> Unit = {},
    val share: (String) -> Unit = {},
    val delete: (Transcript) -> Unit = {},
    val clear: () -> Unit = {},
    val addWord: (String) -> Unit = {},
    val removeWord: (String) -> Unit = {},
    val voiceNotes: (Boolean) -> Unit = {},
    val allowNotifications: () -> Unit = {},
    val openNotificationAccess: () -> Unit = {},
    val openAllFiles: () -> Unit = {},
)

/** The app screen: setup, a field to try it, recent transcripts, the dictionary. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun MainScreen(state: MainState, actions: MainActions = MainActions()) {
    Scaffold { padding ->
        LazyColumn(
            // Edge to edge: the list makes room for the keyboard itself (the try field, new words).
            modifier = Modifier.fillMaxSize().padding(padding).consumeWindowInsets(padding).imePadding(),
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item { Header() }
            item { Section(stringResource(R.string.setup_title)) }
            item { MicStep(state.micGranted, actions) }
            item { ModelStep(state.model, actions) }
            item { Section(stringResource(R.string.start_title)) }
            item { StartChoice(state.startWith, actions) }
            if (state.startWith.button) item { ButtonStep(state.buttonOn, actions) }
            if (state.startWith.keyboard) item { VoiceStep(state.keyboardEnabled, state.micKeyOpens, actions) }
            item { Section(stringResource(R.string.try_title)) }
            item { TryField() }
            item { VoiceNotesStep(state.voiceNotes, actions) }
            item {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Section(stringResource(R.string.recent_title), Modifier.weight(1f))
                    if (state.transcripts.isNotEmpty()) {
                        TextButton(onClick = actions.clear) { Text(stringResource(R.string.recent_clear)) }
                    }
                }
            }
            if (state.transcripts.isEmpty()) {
                item { Muted(stringResource(R.string.recent_empty)) }
            }
            items(state.transcripts, key = { "${it.time}-${it.text.hashCode()}" }) { TranscriptCard(it, actions) }
            item { Section(stringResource(R.string.dictionary_title)) }
            item { Dictionary(state.words, actions) }
            state.timing?.let { (audio, work) ->
                item {
                    Muted(
                        stringResource(
                            R.string.timing,
                            String.format(Locale.getDefault(), "%.1f", audio),
                            String.format(Locale.getDefault(), "%.2f", work),
                        ),
                    )
                }
            }
        }
    }
}

@Composable
private fun Header() {
    Column(Modifier.padding(top = 8.dp, bottom = 4.dp)) {
        Text(
            stringResource(R.string.app_name),
            style = MaterialTheme.typography.headlineMedium,
            fontWeight = FontWeight.SemiBold,
        )
        Spacer(Modifier.padding(2.dp))
        Muted(stringResource(R.string.tagline))
    }
}

@Composable
private fun Section(title: String, modifier: Modifier = Modifier) {
    Text(
        title,
        style = MaterialTheme.typography.titleMedium,
        fontWeight = FontWeight.SemiBold,
        modifier = modifier.padding(top = 8.dp),
    )
}

@Composable
private fun Muted(text: String) {
    Text(text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

/** One setup step: done (a check) or what to do (a button). */
@Composable
private fun Step(
    title: String,
    done: Boolean,
    text: String,
    content: @Composable () -> Unit = {},
) {
    Card(
        colors = CardDefaults.cardColors(
            containerColor = if (done) {
                MaterialTheme.colorScheme.surfaceContainerLow
            } else {
                MaterialTheme.colorScheme.surfaceContainerHigh
            },
        ),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(title, style = MaterialTheme.typography.titleSmall, modifier = Modifier.weight(1f))
                if (done) {
                    Icon(
                        Icons.Default.Check,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.primary,
                    )
                }
            }
            Muted(text)
            content()
        }
    }
}

@Composable
private fun MicStep(granted: Boolean, actions: MainActions) {
    Step(
        stringResource(R.string.setup_mic),
        granted,
        stringResource(if (granted) R.string.setup_mic_ok else R.string.setup_mic_missing),
    ) {
        if (!granted) Button(onClick = actions.allowMic) { Text(stringResource(R.string.setup_mic_allow)) }
    }
}

@Composable
private fun ModelStep(model: ModelState, actions: MainActions) {
    val title = stringResource(R.string.setup_model)
    when (model) {
        ModelState.Ready -> Step(title, true, stringResource(R.string.setup_model_ok))
        is ModelState.Missing -> Step(
            title,
            false,
            model.bytes?.let { stringResource(R.string.setup_model_missing, formatSize(it)) }
                ?: stringResource(R.string.setup_model_missing_unknown),
        ) {
            if (model.metered) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(
                        Icons.Default.Warning,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.error,
                        modifier = Modifier.size(18.dp),
                    )
                    Spacer(Modifier.width(6.dp))
                    Text(
                        stringResource(R.string.setup_model_metered),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.error,
                    )
                }
            }
            Button(onClick = actions.download) { Text(stringResource(R.string.setup_model_download)) }
        }
        is ModelState.Downloading -> Step(
            title,
            false,
            stringResource(
                R.string.setup_model_downloading,
                formatSize(model.done),
                if (model.total > 0) formatSize(model.total) else "…",
            ),
        ) {
            if (model.total > 0) {
                LinearProgressIndicator(
                    progress = { (model.done.toFloat() / model.total).coerceIn(0f, 1f) },
                    modifier = Modifier.fillMaxWidth(),
                )
            } else {
                LinearProgressIndicator(Modifier.fillMaxWidth())
            }
            OutlinedButton(onClick = actions.cancelDownload) { Text(stringResource(R.string.setup_model_cancel)) }
        }
        is ModelState.Failed -> Step(title, false, stringResource(R.string.setup_model_failed, model.reason)) {
            Button(onClick = actions.download) { Text(stringResource(R.string.setup_model_retry)) }
        }
    }
}

/** Floating button, keyboard mic key, or both: radio rows with a line each. */
@Composable
private fun StartChoice(chosen: StartWith, actions: MainActions) {
    Column(Modifier.selectableGroup()) {
        StartRow(StartWith.BUTTON, chosen, R.string.start_button, R.string.start_button_about, actions)
        StartRow(StartWith.KEYBOARD, chosen, R.string.start_keyboard, R.string.start_keyboard_about, actions)
        StartRow(StartWith.BOTH, chosen, R.string.start_both, null, actions)
    }
}

@Composable
private fun StartRow(value: StartWith, chosen: StartWith, title: Int, about: Int?, actions: MainActions) {
    Row(
        Modifier
            .fillMaxWidth()
            .selectable(selected = value == chosen, onClick = { actions.startWith(value) }, role = Role.RadioButton)
            .padding(vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        RadioButton(selected = value == chosen, onClick = null)
        Spacer(Modifier.width(12.dp))
        Column {
            Text(stringResource(title), style = MaterialTheme.typography.bodyLarge)
            if (about != null) Muted(stringResource(about))
        }
    }
}

@Composable
private fun ButtonStep(on: Boolean, actions: MainActions) {
    Step(
        stringResource(R.string.setup_button),
        on,
        stringResource(if (on) R.string.setup_button_ok else R.string.setup_button_off),
    ) {
        if (!on) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = actions.openAccessibility) { Text(stringResource(R.string.setup_button_open)) }
                OutlinedButton(onClick = actions.openAppInfo) { Text(stringResource(R.string.setup_app_info)) }
            }
        }
    }
}

@Composable
private fun VoiceStep(enabled: Boolean, opens: Pair<String, Boolean>?, actions: MainActions) {
    val title = stringResource(R.string.setup_voice)
    when {
        !enabled -> Step(title, false, stringResource(R.string.setup_voice_off)) {
            Button(onClick = actions.enableKeyboard) { Text(stringResource(R.string.setup_keyboard_enable)) }
        }
        opens == null -> Step(title, false, stringResource(R.string.setup_voice_none)) {
            OutlinedButton(onClick = actions.getHeliBoard) { Text(stringResource(R.string.setup_get_heliboard)) }
        }
        !opens.second -> Step(title, false, stringResource(R.string.setup_voice_other, opens.first)) {
            Button(onClick = actions.enableKeyboard) { Text(stringResource(R.string.setup_keyboard_enable)) }
        }
        else -> Step(title, true, stringResource(R.string.setup_voice_ok))
    }
}

@Composable
private fun TryField() {
    var text by rememberSaveable { mutableStateOf("") }
    OutlinedTextField(
        value = text,
        onValueChange = { text = it },
        placeholder = { Text(stringResource(R.string.try_hint)) },
        minLines = 3,
        modifier = Modifier.fillMaxWidth(),
    )
}

/** Incoming WhatsApp voice messages as notifications with the text; sharing works without it. */
@Composable
private fun VoiceNotesStep(state: VoiceNotesState, actions: MainActions) {
    val title = stringResource(R.string.messages_title)
    when {
        !state.on -> Step(title, false, stringResource(R.string.voice_notes_off)) {
            Button(onClick = { actions.voiceNotes(true) }) { Text(stringResource(R.string.voice_notes_turn_on)) }
            Muted(stringResource(R.string.messages_hint))
        }
        !state.ready -> Step(title, false, stringResource(R.string.voice_notes_needs)) {
            if (!state.notifications) {
                Button(onClick = actions.allowNotifications) { Text(stringResource(R.string.voice_notes_allow_notifications)) }
            }
            if (!state.listener) {
                Button(onClick = actions.openNotificationAccess) { Text(stringResource(R.string.voice_notes_notification_access)) }
            }
            if (!state.files) {
                Button(onClick = actions.openAllFiles) { Text(stringResource(R.string.voice_notes_all_files)) }
            }
            Muted(stringResource(R.string.voice_notes_restricted))
            TextButton(onClick = { actions.voiceNotes(false) }) { Text(stringResource(R.string.voice_notes_turn_off)) }
        }
        else -> Step(title, true, stringResource(R.string.voice_notes_ok)) {
            TextButton(onClick = { actions.voiceNotes(false) }) { Text(stringResource(R.string.voice_notes_turn_off)) }
        }
    }
}

@Composable
private fun Hint(title: String, text: String) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(title, style = MaterialTheme.typography.titleSmall)
            Muted(text)
        }
    }
}

@Composable
private fun TranscriptCard(transcript: Transcript, actions: MainActions) {
    val source = stringResource(
        if (transcript.source == Transcript.Source.KEYBOARD) R.string.source_keyboard else R.string.source_message,
    )
    val time = DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(transcript.time))
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(start = 16.dp, top = 12.dp, end = 4.dp, bottom = 4.dp)) {
            Text(
                "$source · $time",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            SelectionContainer(Modifier.padding(top = 4.dp, end = 12.dp)) {
                Text(transcript.text, style = MaterialTheme.typography.bodyLarge)
            }
            TextActions(transcript.text, actions.copy, actions.share) {
                IconButton(onClick = { actions.delete(transcript) }) {
                    Icon(Icons.Default.Delete, contentDescription = stringResource(R.string.delete))
                }
            }
        }
    }
}

/** Copy and Share for a text, right-aligned, with room for more. */
@Composable
private fun TextActions(
    text: String,
    copy: (String) -> Unit,
    share: (String) -> Unit,
    more: @Composable () -> Unit = {},
) {
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
        IconButton(onClick = { copy(text) }) {
            Icon(painterResource(R.drawable.ic_copy), contentDescription = stringResource(R.string.copy))
        }
        IconButton(onClick = { share(text) }) {
            Icon(Icons.Default.Share, contentDescription = stringResource(R.string.share))
        }
        more()
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Dictionary(words: List<String>, actions: MainActions) {
    var word by rememberSaveable { mutableStateOf("") }
    val add = {
        actions.addWord(word)
        word = ""
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Muted(stringResource(R.string.dictionary_hint))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (w in words) {
                InputChip(
                    selected = false,
                    onClick = { actions.removeWord(w) },
                    label = { Text(w) },
                    trailingIcon = {
                        Icon(
                            Icons.Default.Close,
                            contentDescription = stringResource(R.string.remove_word, w),
                            modifier = Modifier.size(18.dp),
                        )
                    },
                )
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            OutlinedTextField(
                value = word,
                onValueChange = { word = it },
                placeholder = { Text(stringResource(R.string.dictionary_add)) },
                singleLine = true,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { add() }),
                modifier = Modifier.weight(1f),
            )
            IconButton(onClick = add, enabled = word.isNotBlank()) {
                Icon(Icons.Default.Add, contentDescription = stringResource(R.string.dictionary_add_button))
            }
        }
    }
}

/** What the share screen's buttons do. */
class ShareActions(
    val copy: (String) -> Unit = {},
    val share: (String) -> Unit = {},
    val openApp: () -> Unit = {},
    val close: () -> Unit = {},
)

/** Shared voice messages and their texts, one card each, in the order they came. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ShareScreen(messages: List<MessageState>, needsModel: Boolean, actions: ShareActions = ShareActions()) {
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.share_title)) },
                navigationIcon = {
                    IconButton(onClick = actions.close) {
                        Icon(Icons.Default.Close, contentDescription = null)
                    }
                },
            )
        },
    ) { padding ->
        LazyColumn(
            modifier = Modifier.fillMaxSize().padding(padding),
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            when {
                needsModel -> item {
                    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Hint(stringResource(R.string.setup_model), stringResource(R.string.share_needs_model))
                        Button(onClick = actions.openApp) { Text(stringResource(R.string.share_open_app)) }
                    }
                }
                messages.isEmpty() -> item { Muted(stringResource(R.string.share_none)) }
                else -> {
                    itemsIndexed(messages) { index, message -> MessageCard(index, message, actions) }
                    val texts = messages.filterIsInstance<MessageState.Done>().map { it.text }.filter { it.isNotBlank() }
                    if (messages.size > 1 && texts.size > 1) {
                        item {
                            Button(onClick = { actions.copy(texts.joinToString("\n\n")) }) {
                                Text(stringResource(R.string.share_copy_all))
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun MessageCard(index: Int, message: MessageState, actions: ShareActions) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(start = 16.dp, top = 12.dp, end = 4.dp, bottom = 4.dp)) {
            Text(
                stringResource(R.string.share_message, index + 1),
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Column(Modifier.padding(top = 6.dp, end = 12.dp, bottom = 8.dp)) {
                when (message) {
                    MessageState.Waiting -> Progress(stringResource(R.string.share_waiting))
                    MessageState.Decoding -> Progress(stringResource(R.string.share_decoding))
                    is MessageState.Transcribing -> Progress(
                        stringResource(R.string.share_transcribing, message.seconds.roundToInt().toString()),
                    )
                    is MessageState.Failed -> Text(
                        stringResource(R.string.share_failed, message.reason),
                        color = MaterialTheme.colorScheme.error,
                    )
                    is MessageState.Done -> if (message.text.isBlank()) {
                        Muted(stringResource(R.string.share_empty))
                    } else {
                        SelectionContainer { Text(message.text, style = MaterialTheme.typography.bodyLarge) }
                    }
                }
            }
            if (message is MessageState.Done && message.text.isNotBlank()) {
                TextActions(message.text, actions.copy, actions.share)
            }
        }
    }
}

@Composable
private fun Progress(text: String) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Muted(text)
        LinearProgressIndicator(Modifier.fillMaxWidth())
    }
}
