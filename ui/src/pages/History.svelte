<script lang="ts">
	import Icon from '../components/Icon.svelte';
	import Keys from '../components/Keys.svelte';
	import Segmented from '../components/Segmented.svelte';
	import { app, on, send } from '../lib/bridge.svelte.ts';
	import { i18n, t } from '../lib/i18n.svelte.ts';
	import type { Candidate, Entry, Kind, State } from '../lib/protocol.ts';
	import { clock, groupByDay } from '../lib/time.ts';

	let { data }: { data: State } = $props();

	let query = $state('');
	let openId = $state<number | null>(null);
	let draft = $state('');
	let found = $state<(Candidate & { chosen: boolean })[]>([]);

	const filtered = $derived(
		query.trim()
			? data.history.filter((e) => e.text.toLowerCase().includes(query.trim().toLowerCase()))
			: data.history
	);
	const groups = $derived(groupByDay(filtered, new Date(), i18n.lang));
	const open = $derived(data.history.find((e) => e.id === openId) ?? null);
	const edited = $derived(open !== null && draft.trim() !== open.text.trim());
	const learning = $derived(found.filter((c) => c.chosen).length);

	const kinds = $derived<{ value: Kind; label: string }[]>([
		{ value: 'dictionary', label: t('history.kind.dictionary') },
		{ value: 'replacement', label: t('history.kind.replacement') }
	]);

	function toggle(entry: Entry) {
		if (openId === entry.id) return close();
		openId = entry.id;
		draft = entry.text;
		found = [];
	}

	// "Letztes Diktat korrigieren…" in the menu opens that dictation here.
	$effect(() => {
		const id = app.open;
		if (id === null) return;
		const entry = data.history.find((e) => e.id === id);
		app.open = null;
		query = '';
		if (entry && openId !== id) toggle(entry);
	});

	function close() {
		openId = null;
		found = [];
	}

	// The app compares the dictation with the edited text and names the misheard words.
	let timer: ReturnType<typeof setTimeout> | undefined;
	function edit() {
		clearTimeout(timer);
		if (!open) return;
		const original = open.text;
		timer = setTimeout(() => send({ type: 'find', original, edited: draft }), 250);
	}

	$effect(() =>
		on((event) => {
			if (event.type !== 'candidates' || !open || event.edited !== draft) return;
			const before = new Map(found.map((c) => [`${c.heard}→${c.meant}`, c]));
			found = event.candidates.map((c) => {
				const kept = before.get(`${c.heard}→${c.meant}`);
				return kept ? { ...c, kind: kept.kind, chosen: kept.chosen } : { ...c, chosen: true };
			});
		})
	);

	function save() {
		if (!open) return;
		const learn = found.filter((c) => c.chosen).map(({ heard, meant, kind }) => ({ heard, meant, kind }));
		send({ type: 'save_entry', id: open.id, text: draft.trim(), learn });
		close();
	}

	function keydown(event: KeyboardEvent) {
		if (event.key === 'Escape' && openId !== null) close();
		if (event.key === 'Enter' && (event.metaKey || event.ctrlKey) && edited) save();
	}
</script>

<svelte:window onkeydown={keydown} />

<header class="page">
	<div>
		<h1>{t('history.title')}</h1>
		<p class="muted">
			{data.history.length === 1 ? t('history.count.one') : t('history.count', { n: data.history.length })}
		</p>
	</div>
	<label class="search">
		<Icon name="search" size={14} />
		<input class="field" type="search" placeholder={t('common.search')} bind:value={query} />
	</label>
</header>

{#if data.history.length === 0}
	<div class="empty">
		<p>{t('history.empty')}</p>
		<p class="muted">
			{t('history.empty.before')}
			<Keys spec={data.config.triggers[0]?.key ?? 'fn'} />
			{t('history.empty.after')}
		</p>
	</div>
{:else if groups.length === 0}
	<p class="empty muted">{t('history.none', { query })}</p>
{/if}

{#each groups as group (group.label)}
	<h2 class="day">{group.label}</h2>
	<div class="box">
		{#each group.entries as entry (entry.id)}
			<div class="entry" class:open={openId === entry.id}>
				<div class="line">
					<button type="button" class="pick" onclick={() => toggle(entry)} aria-expanded={openId === entry.id}>
						<span class="time">{clock(entry.at)}</span>
						<span class="text">{entry.text}</span>
					</button>
					<button
						type="button"
						class="icon"
						title={t('common.copy')}
						aria-label={t('common.copy')}
						onclick={() => send({ type: 'copy', text: entry.text })}><Icon name="copy" size={15} /></button
					>
				</div>
				{#if openId === entry.id}
					<div class="editor">
						<!-- svelte-ignore a11y_autofocus -->
						<textarea
							rows="4"
							bind:value={draft}
							oninput={edit}
							autofocus
							aria-label={t('history.edit')}
							spellcheck="false"
						></textarea>
						{#if found.length}
							<div class="found">
								<div class="muted">{t('history.found')}</div>
								{#each found as candidate, i (`${candidate.heard}→${candidate.meant}`)}
									<div class="candidate">
										<label>
											<input type="checkbox" bind:checked={found[i].chosen} />
											<span class="heard">{candidate.heard}</span>
											<Icon name="arrow" size={13} />
											<span class="meant">{candidate.meant}</span>
										</label>
										<Segmented bind:value={found[i].kind} options={kinds} label={t('history.remember_as')} />
									</div>
								{/each}
							</div>
						{:else if !edited}
							<p class="muted hint">{t('history.hint')}</p>
						{/if}
						<div class="buttons">
							<button type="button" class="btn" onclick={() => send({ type: 'copy', text: draft })}>{t('common.copy')}</button>
							<span class="spacer"></span>
							<button type="button" class="btn" onclick={close}>{t('common.cancel')}</button>
							<button type="button" class="btn primary" disabled={!edited} onclick={save}>
								{learning ? t('history.save_learn') : t('common.save')}
							</button>
						</div>
					</div>
				{/if}
			</div>
		{/each}
	</div>
{/each}

<style>
	.page {
		display: flex;
		align-items: flex-end;
		justify-content: space-between;
		gap: 16px;
		margin: 4px 0 18px;
	}
	h1 {
		font-size: var(--title-size);
		font-weight: 650;
		margin: 0;
	}
	.page p {
		margin: 2px 0 0;
		font-size: 12px;
	}
	.search {
		position: relative;
		display: flex;
		align-items: center;
		color: var(--muted);
	}
	.search :global(svg) {
		position: absolute;
		left: 9px;
	}
	.search input {
		width: 220px;
		padding-left: 28px;
	}
	.day {
		font-size: 12px;
		font-weight: 600;
		color: var(--muted);
		margin: 18px 4px 6px;
	}
	.box {
		background: var(--surface);
		border-radius: var(--radius-group);
		box-shadow: var(--shadow-group);
		overflow: hidden;
	}
	.entry + .entry {
		border-top: 1px solid var(--separator);
	}
	.line {
		display: flex;
		align-items: flex-start;
	}
	.pick {
		flex: 1;
		min-width: 0;
		display: flex;
		gap: 14px;
		padding: 10px 6px 10px 14px;
		border: 0;
		background: transparent;
		text-align: left;
	}
	.line:hover {
		background: color-mix(in srgb, var(--text) 4%, transparent);
	}
	.entry.open .line {
		background: var(--accent-soft);
	}
	.time {
		flex: none;
		width: 38px;
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
	.text {
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	.entry.open .text {
		-webkit-line-clamp: 1;
		line-clamp: 1;
	}
	.icon {
		flex: none;
		margin: 6px 8px 0 0;
		width: 28px;
		height: 28px;
		display: grid;
		place-items: center;
		border: 0;
		border-radius: var(--radius);
		background: transparent;
		color: var(--muted);
		opacity: 0;
	}
	.line:hover .icon,
	.icon:focus-visible {
		opacity: 1;
	}
	.icon:hover {
		background: var(--surface-2);
		color: var(--text);
	}
	.editor {
		padding: 4px 14px 14px 66px;
		background: var(--accent-soft);
	}
	textarea {
		width: 100%;
		resize: vertical;
		padding: 9px 10px;
		border-radius: var(--radius);
		border: 1px solid var(--border);
		background: var(--input);
		line-height: 1.5;
		outline: none;
	}
	textarea:focus {
		border-color: var(--accent);
		box-shadow: 0 0 0 3px var(--accent-soft);
	}
	.hint {
		margin: 8px 0 0;
		font-size: 12px;
	}
	.found {
		margin-top: 10px;
		display: flex;
		flex-direction: column;
		gap: 6px;
		font-size: 12.5px;
	}
	.candidate {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 6px 8px;
		border-radius: var(--radius);
		background: var(--surface);
		box-shadow: var(--shadow-group);
	}
	.candidate label {
		display: flex;
		align-items: center;
		gap: 7px;
		min-width: 0;
	}
	.heard {
		color: var(--err);
		text-decoration: line-through;
		text-decoration-color: color-mix(in srgb, var(--err) 60%, transparent);
	}
	.meant {
		color: var(--ok);
		font-weight: 600;
	}
	.candidate :global(svg) {
		color: var(--muted);
		flex: none;
	}
	.buttons {
		display: flex;
		gap: 8px;
		margin-top: 12px;
	}
	.spacer {
		flex: 1;
	}
	.empty {
		margin: 60px auto;
		text-align: center;
	}
	.empty p {
		margin: 4px 0;
	}
</style>
