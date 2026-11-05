import { test } from 'node:test';
import assert from 'node:assert/strict';

import { renderMarkdown } from './markdown.ts';

test('escapes html', () => {
	assert.equal(renderMarkdown('<script>alert("x")</script> & co'), '<p>&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt; &amp; co</p>');
	assert.equal(renderMarkdown('<img src=x onerror=alert(1)>'), '<p>&lt;img src=x onerror=alert(1)&gt;</p>');
});

test('paragraphs and line breaks', () => {
	assert.equal(renderMarkdown('Eins\nzwei\n\nDrei'), '<p>Eins<br>zwei</p><p>Drei</p>');
	assert.equal(renderMarkdown('\n\n  \n'), '');
});

test('lists', () => {
	assert.equal(renderMarkdown('Stand:\n- Pilot läuft\n- seit **12.10.**'), '<p>Stand:</p><ul><li>Pilot läuft</li><li>seit <strong>12.10.</strong></li></ul>');
	assert.equal(renderMarkdown('1. Erst\n2. Dann'), '<ol><li>Erst</li><li>Dann</li></ol>');
	assert.equal(renderMarkdown('* Stern'), '<ul><li>Stern</li></ul>');
});

test('bold, code and headings', () => {
	assert.equal(renderMarkdown('**fett** und `a <b> **c**`'), '<p><strong>fett</strong> und <code>a &lt;b&gt; **c**</code></p>');
	assert.equal(renderMarkdown('### Titel'), '<p><strong>Titel</strong></p>');
	assert.equal(renderMarkdown('2 * 3 ** 4'), '<p>2 * 3 ** 4</p>');
});

test('links', () => {
	assert.equal(
		renderMarkdown('[Im CRM](https://crm.example.com/accounts/1)'),
		'<p><a href="https://crm.example.com/accounts/1" title="https://crm.example.com/accounts/1">Im CRM</a></p>'
	);
	assert.equal(
		renderMarkdown('**[Notion](https://www.notion.so/a)**'),
		'<p><strong><a href="https://www.notion.so/a" title="https://www.notion.so/a">Notion</a></strong></p>'
	);
	// Only http(s) and mailto become links.
	assert.equal(renderMarkdown('[klick](javascript:alert(1))'), '<p>[klick](javascript:alert(1))</p>');
	assert.equal(renderMarkdown('[klick](javascript:x)'), '<p>[klick](javascript:x)</p>');
	assert.equal(renderMarkdown('[x](https://a.de/"onclick="y)'), '<p><a href="https://a.de/&quot;onclick=&quot;y" title="https://a.de/&quot;onclick=&quot;y">x</a></p>');
});

test('bare links keep sentence marks outside', () => {
	assert.equal(
		renderMarkdown('Siehe https://github.com/example/dictate/pull/12.'),
		'<p>Siehe <a href="https://github.com/example/dictate/pull/12" title="https://github.com/example/dictate/pull/12">https://github.com/example/dictate/pull/12</a>.</p>'
	);
	assert.equal(
		renderMarkdown('(https://a.de/x)'),
		'<p>(<a href="https://a.de/x" title="https://a.de/x">https://a.de/x</a>)</p>'
	);
	assert.equal(renderMarkdown('ftp://a.de'), '<p>ftp://a.de</p>');
});
