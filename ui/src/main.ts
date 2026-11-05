import { mount } from 'svelte';

import './app.css';
import App from './App.svelte';
import { connect } from './lib/bridge.svelte.ts';

// Light or dark like the system; ?theme=light|dark overrides it (screenshots).
const root = document.documentElement;
const forced = new URLSearchParams(location.search).get('theme');
const system = matchMedia('(prefers-color-scheme: dark)');
function applyTheme() {
	root.dataset.theme = forced ?? (system.matches ? 'dark' : 'light');
}
applyTheme();
system.addEventListener('change', applyTheme);

mount(App, { target: document.getElementById('app')! });
void connect();
