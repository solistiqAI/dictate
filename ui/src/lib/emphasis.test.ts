import { test } from 'node:test';
import assert from 'node:assert/strict';

import { emphasize } from './emphasis.ts';

test('dates, times, amounts and percentages are bold', () => {
	assert.equal(emphasize('Abnahme 14.11., 10:00'), 'Abnahme **14.11.**, **10:00**');
	assert.equal(emphasize('Nächster Termin: 14. November, 10 Uhr'), 'Nächster Termin: **14. November**, **10 Uhr**');
	assert.equal(emphasize('Preis 4.800 € pro Monat'), 'Preis **4.800 €** pro Monat');
	assert.equal(emphasize('Fehler 11,8 % statt 14,2 %'), 'Fehler **11,8 %** statt **14,2 %**');
	assert.equal(emphasize('Jonas schickt das Angebot bis Freitag'), 'Jonas schickt das Angebot bis **Freitag**');
	assert.equal(emphasize('Kickoff on November 14, 2026 at 3 pm'), 'Kickoff on **November 14, 2026** at **3 pm**');
	assert.equal(emphasize('Budget € 12 Mio.'), 'Budget **€ 12 Mio.**');
});

test('what is bold, code or a link stays as it is', () => {
	assert.equal(emphasize('Abnahme **14.11., 10:00**'), 'Abnahme **14.11., 10:00**');
	assert.equal(emphasize('[Mail vom 21.10.](https://mail.example.com/21.10.)'), '[Mail vom 21.10.](https://mail.example.com/21.10.)');
	assert.equal(emphasize('`v1.10.2` und https://x.example.com/14.11.'), '`v1.10.2` und https://x.example.com/14.11.');
});

test('plain words and numbers stay plain', () => {
	assert.equal(emphasize('Drei Lizenzen, nicht fünf'), 'Drei Lizenzen, nicht fünf');
	assert.equal(emphasize('Version 2 der Schnittstelle'), 'Version 2 der Schnittstelle');
	assert.equal(emphasize('Mailand ist nicht Mai'), 'Mailand ist nicht Mai');
});
