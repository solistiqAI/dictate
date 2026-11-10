// Every text the page shows, in German and English. `de` is the reference: `en` must have the
// same keys (the type says so) and the same {placeholders} (strings.test.ts). Pick one with
// `translate`; components use `t` from i18n.svelte.ts, which follows the app's language.
// Keep keys grouped by page or component: "general.reports", "panel.ask".

import type { Platform } from './protocol.ts';

export type Lang = 'de' | 'en';

export const de = {
	// General (Allgemein)
	'general.title': 'Allgemein',
	'general.recording': 'Während der Aufnahme',
	'general.sounds': 'Töne beim Start und Stopp',
	'general.sounds.label': 'Töne',
	'general.sounds.off': 'Aus',
	'general.sounds.quiet': 'Leise',
	'general.sounds.medium': 'Mittel',
	'general.pause_media': 'Wiedergabe pausieren',
	'general.pause_media.hint': 'Musik, Videos und Podcasts halten an und laufen danach weiter',
	'general.mute': 'Mac stummschalten',
	'general.text': 'Text',
	'general.voice_commands': 'Sprachbefehle',
	'general.voice_commands.hint':
		'„Neue Zeile“, „neuer Absatz“ und „Spiegelstrich“ setzen Umbrüche und Listenpunkte, statt geschrieben zu werden',
	'general.after_paste': 'Nach dem Einfügen',
	'general.learn': 'Aus Korrekturen lernen',
	'general.learn.hint':
		'Korrigierst du ein Wort im Textfeld, merkt Dictate es sich. Liest das Feld bis zu 30 s, nichts wird gespeichert',
	'general.improve': 'Verbessern',
	'general.reports': 'Anonyme Berichte senden',
	'general.reports.hint':
		'Einmal am Tag Zahlen an uns: wie viele Diktate, wie schnell, welche Probleme, gelernte Wörter. Nie der diktierte Text',
	'general.reports.target': 'An {target}',
	'general.ui_language': 'Sprache',
	'general.ui_language.hint': 'Fenster, Menüs und Souffleur. Ein laufender Souffleur wechselt bei seinem nächsten Start',
	'general.ui_language.auto': 'Wie das System',
	'general.ui_language.de': 'Deutsch',
	'general.ui_language.en': 'English',
	'general.bar': 'Leiste unten am Bildschirm',
	'general.bar.hint': 'Diktieren, Souffleur und Einstellungen mit einem Klick; zeigt, wenn der Souffleur läuft',
	'general.login': 'Beim Anmelden starten',
	'general.folder': 'Daten-Ordner',
	'general.folder.hint': 'Einstellungen, Verlauf, Log und Modelle',
	'general.folder.open': 'Öffnen',

	// Shared
	'common.copy': 'Kopieren',
	'common.search': 'Suchen',
	'common.add': 'Hinzufügen',
	'common.cancel': 'Abbrechen',
	'common.save': 'Speichern',
	'common.open': 'Öffnen',
	'common.stop': 'Beenden',
	'common.settings': 'Einstellungen',
	'common.not_connected': '{name} (nicht angeschlossen)',
	'common.percent': '{n} %',

	// Sidebar
	'nav.label': 'Bereiche',
	'nav.history': 'Verlauf',
	'nav.meeting': 'Souffleur',
	'nav.dictionary': 'Wörterbuch',
	'nav.shortcuts': 'Kurzbefehle',
	'nav.recognition': 'Erkennung',
	'nav.general': 'Allgemein',
	'nav.live': 'Läuft',
	'sidebar.connecting': 'Verbinde…',
	'sidebar.permission': 'Berechtigung fehlt',
	'sidebar.accessibility': 'Bedienungshilfen: ohne sie sieht Dictate die Taste nicht und kann nicht einfügen.',
	'sidebar.microphone': 'Mikrofon: ohne Zugriff hört Dictate nichts.',
	'sidebar.grant': 'Freigeben…',
	'window.no_tray': 'Kein Tray-Symbol auf diesem Desktop',
	'window.no_tray.hint':
		'Dieses Fenster ersetzt das Menü; Schließen beendet Dictate. Unter GNOME zeigt die Erweiterung „AppIndicator“ das Symbol.',

	// Key caps (Keys.svelte): modifiers as macOS draws them, words on Windows and Linux.
	'keys.shift': '⇧',
	'keys.ctrl': '⌃',
	'keys.alt': '⌥',
	'keys.cmd': '⌘',
	'keys.fn': 'fn',
	'keys.right': 'rechts {key}',
	'keys.mouse': 'Maustaste {n}',
	'keys.off': 'aus',

	// History (Verlauf)
	'history.title': 'Verlauf',
	'history.count.one': '1 Diktat, nur auf diesem Mac',
	'history.count': '{n} Diktate, nur auf diesem Mac',
	'history.empty': 'Noch nichts diktiert.',
	'history.empty.before': 'Halte',
	'history.empty.after': 'und sprich, oder tippe kurz für freihändig.',
	'history.none': 'Nichts gefunden für „{query}“.',
	'history.edit': 'Text korrigieren',
	'history.found': 'Falsch erkannt, merken für nächstes Mal:',
	'history.remember_as': 'Merken als',
	'history.kind.dictionary': 'Wörterbuch',
	'history.kind.replacement': 'Ersetzung',
	'history.hint': 'Korrigiere falsch erkannte Wörter; Dictate merkt sie sich fürs nächste Mal.',
	'history.save_learn': 'Speichern und lernen',

	// Dictionary (Wörterbuch)
	'dictionary.title': 'Wörterbuch',
	'dictionary.intro':
		'Wörter, die Parakeet nicht kennt. Ähnlich Klingendes wird zur richtigen Schreibweise, und mit „Wörterbuch bei der Erkennung“ hört das Modell schon beim Erkennen genauer hin.',
	'dictionary.source.default': 'Standard',
	'dictionary.source.manual': 'Von dir',
	'dictionary.source.learned': 'Gelernt',
	'dictionary.try': 'Ausprobieren',
	'dictionary.try.label': 'Testsatz',
	'dictionary.try.placeholder': 'Satz, wie Parakeet ihn schreiben könnte',
	'dictionary.words': 'Wörter ({n})',
	'dictionary.words.search': 'Wörter suchen',
	'dictionary.new_word': 'Neues Wort',
	'dictionary.new_word.placeholder': 'Neues Wort, z. B. Merge Request',
	'dictionary.remove_word': '{word} entfernen',
	'dictionary.replacements': 'Ersetzungen ({n})',
	'dictionary.replacements.footer':
		'Für Verhörer, die zu anders klingen: steht genau so im Text, wird es ersetzt. Gespeichert in replacements.txt.',
	'dictionary.remove_rule': 'Ersetzung entfernen',
	'dictionary.heard': 'Gehört',
	'dictionary.heard.placeholder': 'gehört',
	'dictionary.meant': 'Gemeint',
	'dictionary.meant.placeholder': 'gemeint',

	// Shortcuts (Kurzbefehle)
	'shortcuts.title': 'Kurzbefehle',
	'shortcuts.intro': 'Womit eine Aufnahme startet. Taste, Tastenkombination oder Maustaste.',
	'shortcuts.tidy': 'Nachbearbeitung',
	'shortcuts.tidy.off': 'Aus',
	'shortcuts.tidy.format': 'Formatieren',
	'shortcuts.tidy.both': 'Formatieren + Korrigieren',
	'shortcuts.mode': 'Modus',
	'shortcuts.mode.hold': 'Halten',
	'shortcuts.mode.toggle': 'Umschalten',
	'shortcuts.mode.hybrid': 'Hybrid',
	'shortcuts.capture': 'Taste, Kombination oder Maustaste drücken… (Esc bricht ab)',
	'shortcuts.footer':
		'Halten: sprechen, solange gedrückt. Umschalten: einmal tippen startet, nochmal tippen stoppt. Hybrid: kurz tippen = freihändig, ab einer halben Sekunde halten = wie Halten. Formatieren: Gemma setzt den Text vor dem Einfügen in Absätze und Listen, die Wörter bleiben deine. Formatieren + Korrigieren (Standard): im selben Durchgang fallen Füllwörter (äh, ähm) weg, und wo du dich verbesserst („um drei, äh nein, um fünf“), bleibt nur die Verbesserung; was wegfällt, ist schon beim Sprechen durchgestrichen. Übernommen wird nur, was eindeutig eine Selbstkorrektur ist. Läuft auf dem Mac, etwa eine halbe Sekunde (ohne Gemma Apple Intelligence, unter Erkennung).',
	'shortcuts.listening': 'Drücken…',
	'shortcuts.change': 'Ändern…',
	'shortcuts.remove': 'Kurzbefehl entfernen',
	'shortcuts.add': 'Kurzbefehl hinzufügen',

	// Recognition (Erkennung)
	'recognition.title': 'Erkennung',
	'recognition.intro': 'Läuft vollständig auf diesem Mac, nichts geht ins Netz.',
	'recognition.model': 'Modell',
	'recognition.downloading': 'Lädt',
	'recognition.download': 'Laden',
	'gemma_missing.title': 'Gemma fehlt: Apple Intelligence bearbeitet nach',
	'gemma_missing.hint':
		'Formatieren und Korrigieren dauern damit bei langen Diktaten mehrere Sekunden statt etwa einer halben Sekunde, und es erkennt weniger Selbstkorrekturen. Gemma 4 E4B lädt einmal 5 GB auf diesen Mac.',
	'gemma_missing.download': 'Gemma laden',
	'recognition.active': 'Aktiv',
	'recognition.tidy': 'Formatieren und Korrigieren',
	'recognition.tidy.footer':
		'Für Kurzbefehle mit „Formatieren“ oder „Formatieren + Korrigieren“: das Modell setzt den Text und schlägt vor, wo du dich verbessert hast; übernommen wird nur, was eindeutig eine Selbstkorrektur ist. Fehlt Gemma, nimmt Dictate Apple Intelligence (langsamer, korrigiert weniger).',
	'recognition.language': 'Sprache',
	'recognition.language.hint':
		'Parakeet erkennt sie selbst; Cohere braucht sie (automatisch = Deutsch), Nemotron wird damit genauer',
	'recognition.language.auto': 'Automatisch',
	'recognition.language.de': 'Deutsch',
	'recognition.language.en': 'Englisch',
	'recognition.boost': 'Wörterbuch auch beim Erkennen nutzen',
	'recognition.boost.hint':
		'Neural Engine: ein zweites kleines Modell hört den fertigen Text noch einmal nach Wörterbuch-Wörtern ab. CPU: das Modell bevorzugt sie schon beim Erkennen. Die Schreibweise wird danach immer korrigiert.',
	'recognition.boost.label': 'Wörterbuch bei der Erkennung',
	'recognition.microphone': 'Mikrofon',
	'recognition.record_with': 'Aufnehmen mit',
	'recognition.mic.missing': 'Nicht angeschlossen, solange nimmt Dictate den Standard des Mac',
	'recognition.mic.hint': 'Standard folgt Systemeinstellungen → Ton → Eingabe',
	'recognition.mic.default': 'Standard',
	'recognition.mic.granted': 'Zugriff erlaubt',
	'recognition.mic.denied': 'Kein Zugriff',
	'recognition.mic.speak': 'Sprich…',
	'recognition.mic.test': 'Testen',

	// Souffleur page
	'meeting.title': 'Souffleur',
	'meeting.intro': 'Sagt dir im Meeting ein, was du gerade nachschlagen würdest.',
	'meeting.footer':
		'Dictate schreibt live mit, was du sagst (Ich) und was der Mac abspielt (Andere, die anderen im Call). Ein Claude-Agent liest mit, schlägt selbst im CRM, in Notion, Drive, Slack, Gmail und den Repos nach und zeigt kurze Antworten in der Seitenleiste. Er ändert nichts und schickt nichts – so steht es in seinen Anweisungen.',
	'meeting.running': 'Läuft seit {time}',
	'meeting.answers.one': '1 Antwort',
	'meeting.answers': '{n} Antworten',
	'meeting.sentences.one': '1 Satz',
	'meeting.sentences': '{n} Sätze',
	'meeting.transcribed': '{answers}, {sentences} mitgeschrieben',
	'meeting.ready': 'Bereit fürs nächste Meeting',
	'meeting.ready.hint': 'Starte ihn, wenn der Call beginnt.',
	'meeting.show_panel': 'Seitenleiste zeigen',
	'meeting.start': 'Souffleur starten',
	'meeting.listen': 'Zuhören',
	'meeting.mic': 'Mikrofon',
	'meeting.mic.label': 'Mikrofon für den Souffleur',
	'meeting.mic.missing': 'Nicht angeschlossen, solange nimmt der Souffleur den Mac-Standard',
	'meeting.mic.hint': 'Eigene Wahl, unabhängig vom Mikrofon fürs Diktieren unter Erkennung',
	'meeting.mic.default': 'Mac-Standard',
	'meeting.system': 'Systemton mithören',
	'meeting.system.hint':
		'Was der Mac abspielt, also die anderen im Call. macOS fragt beim ersten Start nach „Systemaudioaufnahme“',
	'meeting.agent': 'Agent',
	'meeting.model': 'Modell',
	'meeting.model.hint': 'Sonnet ist gut und schnell, Haiku am schnellsten, Opus gründlich, aber langsamer',
	'meeting.instructions': 'Zusätzliche Anweisungen',
	'meeting.instructions.hint':
		'Kommen zu seinen Anweisungen dazu, etwa worum es im Meeting geht oder in welcher Sprache er antwortet',
	'meeting.instructions.placeholder':
		'Zum Beispiel: Kunde ist Kessler Logistik, es geht um die Abnahme des Piloten. Antworte auf Englisch.',
	'meeting.transcripts': 'Mitschriften',
	'meeting.transcripts.hint': 'Jedes Meeting als Markdown-Datei mit den Antworten, ohne Ton',

	// Souffleur panel
	'panel.phase.off': 'Agent aus',
	'panel.phase.starting': 'Startet …',
	'panel.phase.listening': 'Hört zu',
	'panel.phase.thinking': 'Denkt nach …',
	'panel.phase.problem': 'Problem',
	'panel.activity': '{activity} …',
	'panel.seeking': 'Sucht',
	'panel.ended': 'beendet',
	'panel.start': 'Starten',
	'panel.me': 'Ich',
	'panel.others': 'Andere',
	'panel.silent': 'Systemton still – Berechtigung?',
	'panel.silent.where': 'Datenschutz → Bildschirm- und Systemaudioaufnahme',
	'panel.silent.title': 'Systemeinstellungen → Datenschutz & Sicherheit → Bildschirm- und Systemaudioaufnahme',
	'panel.system_problem': 'Systemton lässt sich nicht aufnehmen, nur das Mikrofon läuft.',
	'panel.listening': 'Hört zu – meldet sich, wenn es etwas nachzuschlagen gibt.',
	'panel.ask_below': 'Du kannst auch unten fragen.',
	'panel.idle': 'Starte den Souffleur, wenn das Meeting beginnt.',
	'panel.idle.hint':
		'Er hört mit, schlägt selbst im CRM, in Notion, Drive, Slack, Gmail und den Repos nach und zeigt hier kurze Antworten. Er ändert nichts und schickt nichts.',
	'panel.ask': 'Frag den Souffleur',
	'panel.ask.placeholder': 'Frag den Souffleur …',
	'panel.ask.off': 'Fragen geht, solange er läuft',
	'panel.ask.send': 'Fragen',
	'panel.question': '„{question}“',
	'panel.highlights': 'Das Wichtigste',
	'panel.transcript': 'Mitschrift',
	'panel.transcript.none': 'Noch nichts gesagt.',
	'panel.transcript.latest': 'Neueste',
	'panel.meter.off': '{label}: aus',

	// Times (time.ts)
	'time.today': 'Heute',
	'time.yesterday': 'Gestern',
	'time.earlier': 'Früher',
	'time.date': '{weekday}, {day}. {month}',
	'time.date_year': '{date} {year}',
	'time.just_now': 'gerade eben',
	'time.minutes_ago': 'vor {n} Min',

	// Notes and status of the mock app (mock.ts), standing in for what the Rust side sends.
	'mock.ready': 'Bereit',
	'mock.panel_opened': 'Seitenleiste geöffnet',
	'mock.settings': 'Einstellungen im Hauptfenster',
	'mock.browser': 'Im Browser geöffnet',
	'mock.meetings': 'Ordner mit Mitschriften geöffnet',
	'mock.copied': 'Kopiert',
	'mock.saved': 'Gespeichert',
	'mock.saved_learned': 'Gespeichert, {n} gelernt',
	'mock.privacy': 'Systemeinstellungen geöffnet',
	'mock.folder': 'Ordner geöffnet'
};

export type Key = keyof typeof de;

export const en: Record<Key, string> = {
	'general.title': 'General',
	'general.recording': 'While recording',
	'general.sounds': 'Sounds at start and stop',
	'general.sounds.label': 'Sounds',
	'general.sounds.off': 'Off',
	'general.sounds.quiet': 'Quiet',
	'general.sounds.medium': 'Medium',
	'general.pause_media': 'Pause playback',
	'general.pause_media.hint': 'Music, videos and podcasts pause and continue afterwards',
	'general.mute': 'Mute the Mac',
	'general.text': 'Text',
	'general.voice_commands': 'Voice commands',
	'general.voice_commands.hint':
		'“New line”, “new paragraph” and “bullet point” (or “neue Zeile”, “neuer Absatz”, “Spiegelstrich”) set breaks and list items instead of being written',
	'general.after_paste': 'After pasting',
	'general.learn': 'Learn from corrections',
	'general.learn.hint':
		'Correct a word in the text field and Dictate remembers it. Reads the field for up to 30 s, nothing is stored',
	'general.improve': 'Improve',
	'general.reports': 'Send anonymous reports',
	'general.reports.hint':
		'Once a day, numbers to us: how many dictations, how fast, which problems, learned words. Never the dictated text',
	'general.reports.target': 'To {target}',
	'general.ui_language': 'Language',
	'general.ui_language.hint': 'Window, menus and Souffleur. A running Souffleur switches at its next start',
	'general.ui_language.auto': 'Like the system',
	'general.ui_language.de': 'Deutsch',
	'general.ui_language.en': 'English',
	'general.bar': 'Bar at the bottom of the screen',
	'general.bar.hint': 'Dictate, Souffleur and Settings in one click; shows when the Souffleur is running',
	'general.login': 'Start at login',
	'general.folder': 'Data folder',
	'general.folder.hint': 'Settings, history, log and models',
	'general.folder.open': 'Open',

	'common.copy': 'Copy',
	'common.search': 'Search',
	'common.add': 'Add',
	'common.cancel': 'Cancel',
	'common.save': 'Save',
	'common.open': 'Open',
	'common.stop': 'Stop',
	'common.settings': 'Settings',
	'common.not_connected': '{name} (not connected)',
	'common.percent': '{n}%',

	'nav.label': 'Sections',
	'nav.history': 'History',
	'nav.meeting': 'Souffleur',
	'nav.dictionary': 'Dictionary',
	'nav.shortcuts': 'Shortcuts',
	'nav.recognition': 'Recognition',
	'nav.general': 'General',
	'nav.live': 'Running',
	'sidebar.connecting': 'Connecting…',
	'sidebar.permission': 'Permission missing',
	'sidebar.accessibility': 'Accessibility: without it, Dictate can’t see the key and can’t paste.',
	'sidebar.microphone': 'Microphone: without access, Dictate hears nothing.',
	'sidebar.grant': 'Allow…',
	'window.no_tray': 'No tray icon on this desktop',
	'window.no_tray.hint':
		'This window stands in for the menu; closing it quits Dictate. On GNOME, the AppIndicator extension shows the icon.',

	'keys.shift': '⇧',
	'keys.ctrl': '⌃',
	'keys.alt': '⌥',
	'keys.cmd': '⌘',
	'keys.fn': 'fn',
	'keys.right': 'right {key}',
	'keys.mouse': 'Mouse button {n}',
	'keys.off': 'off',

	'history.title': 'History',
	'history.count.one': '1 dictation, only on this Mac',
	'history.count': '{n} dictations, only on this Mac',
	'history.empty': 'Nothing dictated yet.',
	'history.empty.before': 'Hold',
	'history.empty.after': 'and speak, or tap it for hands-free.',
	'history.none': 'Nothing found for “{query}”.',
	'history.edit': 'Edit text',
	'history.found': 'Misheard, remember for next time:',
	'history.remember_as': 'Remember as',
	'history.kind.dictionary': 'Dictionary',
	'history.kind.replacement': 'Replacement',
	'history.hint': 'Fix misheard words; Dictate remembers them for next time.',
	'history.save_learn': 'Save and learn',

	'dictionary.title': 'Dictionary',
	'dictionary.intro':
		'Words Parakeet doesn’t know. Anything that sounds similar becomes the right spelling, and with “Dictionary during recognition” the model already listens for them while recognizing.',
	'dictionary.source.default': 'Default',
	'dictionary.source.manual': 'Added by you',
	'dictionary.source.learned': 'Learned',
	'dictionary.try': 'Try it',
	'dictionary.try.label': 'Test sentence',
	'dictionary.try.placeholder': 'A sentence as Parakeet might write it',
	'dictionary.words': 'Words ({n})',
	'dictionary.words.search': 'Search words',
	'dictionary.new_word': 'New word',
	'dictionary.new_word.placeholder': 'New word, e.g. Merge Request',
	'dictionary.remove_word': 'Remove {word}',
	'dictionary.replacements': 'Replacements ({n})',
	'dictionary.replacements.footer':
		'For mishearings that sound too different: wherever it appears exactly like this in the text, it is replaced. Saved in replacements.txt.',
	'dictionary.remove_rule': 'Remove replacement',
	'dictionary.heard': 'Heard',
	'dictionary.heard.placeholder': 'heard',
	'dictionary.meant': 'Meant',
	'dictionary.meant.placeholder': 'meant',

	'shortcuts.title': 'Shortcuts',
	'shortcuts.intro': 'What starts a recording: a key, a key combination or a mouse button.',
	'shortcuts.tidy': 'Post-processing',
	'shortcuts.tidy.off': 'Off',
	'shortcuts.tidy.format': 'Format',
	'shortcuts.tidy.both': 'Format + Clean up',
	'shortcuts.mode': 'Mode',
	'shortcuts.mode.hold': 'Hold',
	'shortcuts.mode.toggle': 'Toggle',
	'shortcuts.mode.hybrid': 'Hybrid',
	'shortcuts.capture': 'Press a key, combination or mouse button… (Esc cancels)',
	'shortcuts.footer':
		'Hold: speak while it’s held down. Toggle: tap once to start, tap again to stop. Hybrid: a short tap = hands-free, holding for half a second or longer = like Hold. Format: Gemma lays the text out in paragraphs and lists before pasting; the words stay yours. Format + Clean up (default): in the same pass, filler words (uh, um) drop out, and where you correct yourself (“at three, uh no, at five”), only the correction stays; what drops out is struck through while you speak. Only what is clearly a self-correction is applied. Runs on the Mac, about half a second (without Gemma, Apple Intelligence, under Recognition).',
	'shortcuts.listening': 'Press…',
	'shortcuts.change': 'Change…',
	'shortcuts.remove': 'Remove shortcut',
	'shortcuts.add': 'Add shortcut',

	'recognition.title': 'Recognition',
	'recognition.intro': 'Runs entirely on this Mac, nothing goes online.',
	'recognition.model': 'Model',
	'recognition.downloading': 'Downloading',
	'recognition.download': 'Download',
	'gemma_missing.title': 'Gemma is missing: Apple Intelligence tidies the text',
	'gemma_missing.hint':
		'Format and clean up then take several seconds on a long dictation instead of about half a second, and catch fewer self-corrections. Gemma 4 E4B is a one-time 5 GB download to this Mac.',
	'gemma_missing.download': 'Get Gemma',
	'recognition.active': 'Active',
	'recognition.tidy': 'Format and clean up',
	'recognition.tidy.footer':
		'For shortcuts with “Format” or “Format + Clean up”: the model lays out the text and suggests where you corrected yourself; only what is clearly a self-correction is applied. Without Gemma, Dictate uses Apple Intelligence (slower, cleans up less).',
	'recognition.language': 'Language',
	'recognition.language.hint':
		'Parakeet detects it by itself; Cohere needs it (automatic = German), Nemotron gets more accurate with it',
	'recognition.language.auto': 'Automatic',
	'recognition.language.de': 'German',
	'recognition.language.en': 'English',
	'recognition.boost': 'Use the dictionary during recognition too',
	'recognition.boost.hint':
		'Neural Engine: a second small model listens to the finished text once more for dictionary words. CPU: the model already favors them while recognizing. The spelling is always corrected afterwards.',
	'recognition.boost.label': 'Dictionary during recognition',
	'recognition.microphone': 'Microphone',
	'recognition.record_with': 'Record with',
	'recognition.mic.missing': 'Not connected; until it is, Dictate uses the Mac’s default',
	'recognition.mic.hint': 'Default follows System Settings → Sound → Input',
	'recognition.mic.default': 'Default',
	'recognition.mic.granted': 'Access allowed',
	'recognition.mic.denied': 'No access',
	'recognition.mic.speak': 'Speak…',
	'recognition.mic.test': 'Test',

	'meeting.title': 'Souffleur',
	'meeting.intro': 'Prompts you in meetings with what you would otherwise look up.',
	'meeting.footer':
		'Dictate transcribes live what you say (Me) and what the Mac plays (Others, the other people on the call). A Claude agent reads along, looks things up itself in the CRM, Notion, Drive, Slack, Gmail and the repos, and shows short answers in the side panel. It changes nothing and sends nothing – its instructions say so.',
	'meeting.running': 'Running for {time}',
	'meeting.answers.one': '1 answer',
	'meeting.answers': '{n} answers',
	'meeting.sentences.one': '1 sentence',
	'meeting.sentences': '{n} sentences',
	'meeting.transcribed': '{answers}, {sentences} transcribed',
	'meeting.ready': 'Ready for the next meeting',
	'meeting.ready.hint': 'Start it when the call begins.',
	'meeting.show_panel': 'Show side panel',
	'meeting.start': 'Start Souffleur',
	'meeting.listen': 'Listening',
	'meeting.mic': 'Microphone',
	'meeting.mic.label': 'Microphone for the Souffleur',
	'meeting.mic.missing': 'Not connected; until it is, the Souffleur uses the Mac’s default',
	'meeting.mic.hint': 'Its own choice, separate from the dictation microphone under Recognition',
	'meeting.mic.default': 'Mac default',
	'meeting.system': 'Listen to system audio',
	'meeting.system.hint':
		'What the Mac plays, i.e. the others on the call. macOS asks for “System Audio Recording” the first time',
	'meeting.agent': 'Agent',
	'meeting.model': 'Model',
	'meeting.model.hint': 'Sonnet is good and fast, Haiku the fastest, Opus thorough but slower',
	'meeting.instructions': 'Additional instructions',
	'meeting.instructions.hint':
		'Added to its instructions, such as what the meeting is about or which language it answers in',
	'meeting.instructions.placeholder':
		'For example: the customer is Kessler Logistik, it’s about signing off the pilot. Answer in German.',
	'meeting.transcripts': 'Transcripts',
	'meeting.transcripts.hint': 'Every meeting as a Markdown file with the answers, no audio',

	'panel.phase.off': 'Agent off',
	'panel.phase.starting': 'Starting…',
	'panel.phase.listening': 'Listening',
	'panel.phase.thinking': 'Thinking…',
	'panel.phase.problem': 'Problem',
	'panel.activity': '{activity}…',
	'panel.seeking': 'Looking up',
	'panel.ended': 'ended',
	'panel.start': 'Start',
	'panel.me': 'Me',
	'panel.others': 'Others',
	'panel.silent': 'System audio silent – permission?',
	'panel.silent.where': 'Privacy → Screen & System Audio Recording',
	'panel.silent.title': 'System Settings → Privacy & Security → Screen & System Audio Recording',
	'panel.system_problem': 'Can’t record system audio, only the microphone is running.',
	'panel.listening': 'Listening – speaks up when there’s something to look up.',
	'panel.ask_below': 'You can also ask below.',
	'panel.idle': 'Start the Souffleur when the meeting begins.',
	'panel.idle.hint':
		'It listens in, looks things up itself in the CRM, Notion, Drive, Slack, Gmail and the repos, and shows short answers here. It changes nothing and sends nothing.',
	'panel.ask': 'Ask the Souffleur',
	'panel.ask.placeholder': 'Ask the Souffleur…',
	'panel.ask.off': 'You can ask while it’s running',
	'panel.ask.send': 'Ask',
	'panel.question': '“{question}”',
	'panel.highlights': 'Key facts',
	'panel.transcript': 'Transcript',
	'panel.transcript.none': 'Nothing said yet.',
	'panel.transcript.latest': 'Latest',
	'panel.meter.off': '{label}: off',

	'time.today': 'Today',
	'time.yesterday': 'Yesterday',
	'time.earlier': 'Earlier',
	'time.date': '{weekday}, {month} {day}',
	'time.date_year': '{date}, {year}',
	'time.just_now': 'just now',
	'time.minutes_ago': '{n} min ago',

	'mock.ready': 'Ready',
	'mock.panel_opened': 'Side panel opened',
	'mock.settings': 'Settings in the main window',
	'mock.browser': 'Opened in the browser',
	'mock.meetings': 'Transcripts folder opened',
	'mock.copied': 'Copied',
	'mock.saved': 'Saved',
	'mock.saved_learned': 'Saved, {n} learned',
	'mock.privacy': 'System Settings opened',
	'mock.folder': 'Folder opened'
};

type Reworded = Record<Lang, Partial<Record<Key, string>>>;

// What reads the same on Windows and Linux: no Mac, no Apple model, no Neural Engine.
const PC: Reworded = {
	de: {
		'keys.shift': 'Umschalt',
		'keys.ctrl': 'Strg',
		'keys.alt': 'Alt',
		'keys.fn': 'Fn',
		'general.mute': 'Computer stummschalten',
		'history.count.one': '1 Diktat, nur auf diesem Computer',
		'history.count': '{n} Diktate, nur auf diesem Computer',
		'shortcuts.footer':
			'Halten: sprechen, solange gedrückt. Umschalten: einmal tippen startet, nochmal tippen stoppt. Hybrid: kurz tippen = freihändig, ab einer halben Sekunde halten = wie Halten. Formatieren: Gemma setzt den Text vor dem Einfügen in Absätze und Listen, die Wörter bleiben deine. Formatieren + Korrigieren (Standard): im selben Durchgang fallen Füllwörter (äh, ähm) weg, und wo du dich verbesserst („um drei, äh nein, um fünf“), bleibt nur die Verbesserung; was wegfällt, ist schon beim Sprechen durchgestrichen. Übernommen wird nur, was eindeutig eine Selbstkorrektur ist. Läuft auf diesem Computer und braucht Gemma (unter Erkennung).',
		'recognition.intro': 'Läuft vollständig auf diesem Computer, nichts geht ins Netz.',
		'gemma_missing.title': 'Gemma fehlt: der Text wird nicht nachbearbeitet',
		'gemma_missing.hint': 'Formatieren und Korrigieren brauchen Gemma 4 E4B, einmal 5 GB auf diesen Computer.',
		'recognition.tidy.footer':
			'Für Kurzbefehle mit „Formatieren“ oder „Formatieren + Korrigieren“: das Modell setzt den Text und schlägt vor, wo du dich verbessert hast; übernommen wird nur, was eindeutig eine Selbstkorrektur ist. Ohne Gemma bleibt der Text, wie er erkannt wurde.',
		'recognition.boost.hint':
			'Das Modell bevorzugt sie schon beim Erkennen. Die Schreibweise wird danach immer korrigiert.',
		'recognition.mic.missing': 'Nicht angeschlossen, solange nimmt Dictate den Standard des Systems',
		'meeting.footer':
			'Dictate schreibt live mit, was du sagst (Ich) und was der Computer abspielt (Andere, die anderen im Call). Ein Claude-Agent liest mit, schlägt selbst im CRM, in Notion, Drive, Slack, Gmail und den Repos nach und zeigt kurze Antworten in der Seitenleiste. Er ändert nichts und schickt nichts – so steht es in seinen Anweisungen.',
		'meeting.mic.missing': 'Nicht angeschlossen, solange nimmt der Souffleur den System-Standard',
		'meeting.mic.default': 'System-Standard',
		'panel.silent': 'Systemton still',
		'panel.silent.where': 'Ausgabegerät prüfen',
		'panel.silent.title': 'Aufgenommen wird, was das Standard-Ausgabegerät abspielt'
	},
	en: {
		'keys.shift': 'Shift',
		'keys.ctrl': 'Ctrl',
		'keys.alt': 'Alt',
		'keys.fn': 'Fn',
		'general.mute': 'Mute the computer',
		'history.count.one': '1 dictation, only on this computer',
		'history.count': '{n} dictations, only on this computer',
		'shortcuts.footer':
			'Hold: speak while it’s held down. Toggle: tap once to start, tap again to stop. Hybrid: a short tap = hands-free, holding for half a second or longer = like Hold. Format: Gemma lays the text out in paragraphs and lists before pasting; the words stay yours. Format + Clean up (default): in the same pass, filler words (uh, um) drop out, and where you correct yourself (“at three, uh no, at five”), only the correction stays; what drops out is struck through while you speak. Only what is clearly a self-correction is applied. Runs on this computer and needs Gemma (under Recognition).',
		'recognition.intro': 'Runs entirely on this computer, nothing goes online.',
		'gemma_missing.title': 'Gemma is missing: the text is not tidied',
		'gemma_missing.hint': 'Format and clean up need Gemma 4 E4B, a one-time 5 GB download to this computer.',
		'recognition.tidy.footer':
			'For shortcuts with “Format” or “Format + Clean up”: the model lays out the text and suggests where you corrected yourself; only what is clearly a self-correction is applied. Without Gemma, the text stays as it was recognized.',
		'recognition.boost.hint':
			'The model already favors them while recognizing. The spelling is always corrected afterwards.',
		'recognition.mic.missing': 'Not connected; until it is, Dictate uses the system default',
		'meeting.footer':
			'Dictate transcribes live what you say (Me) and what the computer plays (Others, the other people on the call). A Claude agent reads along, looks things up itself in the CRM, Notion, Drive, Slack, Gmail and the repos, and shows short answers in the side panel. It changes nothing and sends nothing – its instructions say so.',
		'meeting.mic.missing': 'Not connected; until it is, the Souffleur uses the system default',
		'meeting.mic.default': 'System default',
		'panel.silent': 'System sound silent',
		'panel.silent.where': 'check the output device',
		'panel.silent.title': 'What the default output device plays is recorded'
	}
};

/** Windows: the Windows key for ⌘ (hook.rs), Settings instead of System Settings. */
export const WINDOWS: Reworded = {
	de: {
		...PC.de,
		'keys.cmd': 'Win',
		'sidebar.accessibility': 'Tastatur: ohne Zugriff sieht Dictate die Taste nicht und kann nicht einfügen.',
		'recognition.mic.hint': 'Standard folgt Einstellungen → System → Sound → Eingabe',
		'meeting.system.hint': 'Was der PC abspielt, also die anderen im Call'
	},
	en: {
		...PC.en,
		'keys.cmd': 'Win',
		'sidebar.accessibility': 'Keyboard: without access, Dictate can’t see the key and can’t paste.',
		'recognition.mic.hint': 'Default follows Settings → System → Sound → Input',
		'meeting.system.hint': 'What the PC plays, i.e. the others on the call'
	}
};

/** Linux: Super for ⌘ (linux.rs), the input group instead of Accessibility (docs/linux.md). */
export const LINUX: Reworded = {
	de: {
		...PC.de,
		'keys.cmd': 'Super',
		'sidebar.accessibility':
			'Tastatur: Dictate kann die Tasten nicht lesen. Unter Wayland: sudo usermod -aG input $USER, dann neu anmelden.',
		'recognition.mic.hint': 'Standard folgt den Toneinstellungen des Desktops',
		'meeting.system.hint': 'Was der Computer abspielt (Monitor des Standard-Ausgangs), also die anderen im Call'
	},
	en: {
		...PC.en,
		'keys.cmd': 'Super',
		'sidebar.accessibility':
			'Keyboard: Dictate can’t read the keys. On Wayland: sudo usermod -aG input $USER, then log in again.',
		'recognition.mic.hint': 'Default follows the desktop’s sound settings',
		'meeting.system.hint': 'What the computer plays (the default output’s monitor), i.e. the others on the call'
	}
};

const TABLES: Record<Lang, Record<Key, string>> = { de, en };
const REWORDED: Record<Platform, Reworded | null> = { macos: null, windows: WINDOWS, linux: LINUX };

/** The text for `key` in `lang` on `platform`, with `{name}` replaced by `params.name`. */
export function translate(
	lang: Lang,
	key: Key,
	params?: Record<string, string | number>,
	platform: Platform = 'macos'
): string {
	const text = REWORDED[platform]?.[lang][key] ?? TABLES[lang][key] ?? de[key] ?? key;
	if (!params) return text;
	return text.replace(/\{(\w+)\}/g, (all, name: string) => (name in params ? String(params[name]) : all));
}

/** The locale for Intl / toLocaleString. */
export function locale(lang: Lang): string {
	return lang === 'en' ? 'en-US' : 'de-DE';
}
