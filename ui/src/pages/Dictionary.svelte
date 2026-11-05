<script lang="ts">
	import Group from '../components/Group.svelte';
	import Icon from '../components/Icon.svelte';
	import { on, send } from '../lib/bridge.svelte.ts';
	import { t } from '../lib/i18n.svelte.ts';
	import type { State } from '../lib/protocol.ts';

	let { data }: { data: State } = $props();

	let newWord = $state('');
	let query = $state('');
	let heard = $state('');
	let meant = $state('');
	let sample = $state('Wir deckläuen morgen bei Solistik, kannst du den Matchdruckers mergen?');
	let result = $state('');

	const sources = $derived({
		default: t('dictionary.source.default'),
		manual: t('dictionary.source.manual'),
		learned: t('dictionary.source.learned')
	});

	const words = $derived(
		data.words.filter((w) => w.word.toLowerCase().includes(query.trim().toLowerCase()))
	);

	function addWord(event: SubmitEvent) {
		event.preventDefault();
		const word = newWord.trim();
		if (!word || data.words.some((w) => w.word === word)) return;
		send({ type: 'words', words: [...data.words.map((w) => w.word), word] });
		newWord = '';
	}

	function removeWord(word: string) {
		send({ type: 'words', words: data.words.map((w) => w.word).filter((w) => w !== word) });
	}

	function addRule(event: SubmitEvent) {
		event.preventDefault();
		if (!heard.trim() || !meant.trim()) return;
		send({
			type: 'replacements',
			replacements: [...data.replacements, { heard: heard.trim(), meant: meant.trim() }]
		});
		heard = '';
		meant = '';
	}

	function removeRule(index: number) {
		send({ type: 'replacements', replacements: data.replacements.filter((_, i) => i !== index) });
	}

	$effect(() =>
		on((event) => {
			if (event.type === 'tried' && event.text === sample) result = event.result;
		})
	);

	// Re-run the test when the sentence or the dictionary changes.
	$effect(() => {
		void data.words;
		void data.replacements;
		send({ type: 'try', text: sample });
	});
</script>

<header class="page">
	<h1>{t('dictionary.title')}</h1>
	<p class="muted">{t('dictionary.intro')}</p>
</header>

<Group title={t('dictionary.try')}>
	<div class="try">
		<input
			class="field"
			bind:value={sample}
			aria-label={t('dictionary.try.label')}
			placeholder={t('dictionary.try.placeholder')}
		/>
		<div class="result" aria-live="polite">
			<Icon name="arrow" size={14} />
			<span>{result}</span>
		</div>
	</div>
</Group>

<Group title={t('dictionary.words', { n: data.words.length })}>
	{#snippet actions()}
		<input
			class="field small"
			type="search"
			placeholder={t('common.search')}
			bind:value={query}
			aria-label={t('dictionary.words.search')}
		/>
	{/snippet}
	<form class="add" onsubmit={addWord}>
		<input
			class="field"
			placeholder={t('dictionary.new_word.placeholder')}
			bind:value={newWord}
			aria-label={t('dictionary.new_word')}
		/>
		<button class="btn" type="submit" disabled={!newWord.trim()}>{t('common.add')}</button>
	</form>
	<div class="chips">
		{#each words as word (word.word)}
			<span class="chip" data-source={word.source} title={sources[word.source]}>
				{word.word}
				{#if word.source !== 'default'}<small>{sources[word.source]}</small>{/if}
				<button type="button" aria-label={t('dictionary.remove_word', { word: word.word })} onclick={() => removeWord(word.word)}>
					<Icon name="x" size={11} />
				</button>
			</span>
		{/each}
	</div>
</Group>

<Group
	title={t('dictionary.replacements', { n: data.replacements.length })}
	footer={t('dictionary.replacements.footer')}
>
	{#each data.replacements as rule, i (`${rule.heard}→${rule.meant}`)}
		<div class="rule">
			<span class="heard">{rule.heard}</span>
			<Icon name="arrow" size={13} />
			<span class="meant">{rule.meant}</span>
			<button type="button" class="remove" aria-label={t('dictionary.remove_rule')} onclick={() => removeRule(i)}>
				<Icon name="x" size={12} />
			</button>
		</div>
	{/each}
	<form class="rule new" onsubmit={addRule}>
		<input class="field" placeholder={t('dictionary.heard.placeholder')} bind:value={heard} aria-label={t('dictionary.heard')} />
		<Icon name="arrow" size={13} />
		<input class="field" placeholder={t('dictionary.meant.placeholder')} bind:value={meant} aria-label={t('dictionary.meant')} />
		<button class="btn" type="submit" disabled={!heard.trim() || !meant.trim()}>{t('common.add')}</button>
	</form>
</Group>

<style>
	.page {
		margin: 4px 0 20px;
	}
	h1 {
		font-size: var(--title-size);
		font-weight: 650;
		margin: 0 0 4px;
	}
	.page p {
		margin: 0;
		max-width: 620px;
		font-size: 12.5px;
	}
	.try {
		padding: 12px 14px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.try .field {
		width: 100%;
	}
	.result {
		display: flex;
		gap: 8px;
		align-items: baseline;
		font-weight: 500;
	}
	.result :global(svg) {
		color: var(--accent);
		flex: none;
		transform: translateY(2px);
	}
	.small {
		height: 24px;
		width: 160px;
	}
	.add {
		display: flex;
		gap: 8px;
		padding: 12px 14px;
	}
	.add .field {
		flex: 1;
	}
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		padding: 12px 14px 14px;
	}
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		height: 26px;
		padding: 0 4px 0 10px;
		border-radius: 13px;
		background: var(--surface-2);
	}
	.chip[data-source='learned'] {
		background: var(--accent-soft);
	}
	.chip small {
		font-size: 10.5px;
		color: var(--muted);
	}
	.chip[data-source='learned'] small {
		color: var(--accent);
	}
	.chip button,
	.remove {
		width: 20px;
		height: 20px;
		display: grid;
		place-items: center;
		border: 0;
		border-radius: 50%;
		background: transparent;
		color: var(--muted);
	}
	.chip button:hover,
	.remove:hover {
		background: color-mix(in srgb, var(--text) 10%, transparent);
		color: var(--text);
	}
	.rule {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 10px 8px 14px;
		min-height: 40px;
	}
	.rule :global(svg) {
		color: var(--muted);
		flex: none;
	}
	.heard {
		color: var(--muted);
		min-width: 160px;
	}
	.meant {
		flex: 1;
		font-weight: 500;
	}
	.rule.new .field {
		flex: 1;
	}
</style>
