// The current time in Unix seconds for elapsed times and "vor 2 Min": reading `clock.now` in a
// component or $derived updates it every second while something on screen uses it.

import { createSubscriber } from 'svelte/reactivity';

const subscribe = createSubscriber((update) => {
	const tick = setInterval(update, 1000);
	return () => clearInterval(tick);
});

export const clock = {
	get now(): number {
		subscribe();
		return Date.now() / 1000;
	}
};
