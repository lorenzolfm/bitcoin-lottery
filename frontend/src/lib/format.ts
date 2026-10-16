const integer = new Intl.NumberFormat('en-US');

export function formatNumber(value: number): string {
	return integer.format(value);
}

export function formatTime(iso: string): string {
	return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

export function formatDate(iso: string): string {
	return new Date(iso).toLocaleDateString([], { year: 'numeric', month: 'short', day: 'numeric' });
}

export function shortHash(hash: string): string {
	return `${hash.slice(0, 8)}…${hash.slice(-8)}`;
}

export function blockUrl(hash: string): string {
	return `https://mempool.space/block/${hash}`;
}

/** Reads a guess typed with or without thousands separators. */
export function parseGuess(text: string): number | null {
	const digits = text.replace(/[\s,._']/g, '');
	if (!/^\d+$/.test(digits)) return null;
	const value = Number(digits);
	return Number.isSafeInteger(value) ? value : null;
}
