import { send } from './bridge.svelte.ts';
import type { Config } from './protocol.ts';

/** Sends the config with `change` applied; the app saves it and answers with the new state. */
export function setConfig(config: Config, change: Partial<Config>) {
	send({ type: 'config', config: { ...$state.snapshot(config), ...change } });
}
