// Per-device conveniences only. Storage can be missing (private windows,
// blocked site data), so every access tolerates failure.

export function load(key: string): string | null {
	try {
		return localStorage.getItem(key);
	} catch {
		return null;
	}
}

export function save(key: string, value: string | null): void {
	try {
		if (value === null) localStorage.removeItem(key);
		else localStorage.setItem(key, value);
	} catch {
		// Not persisted; the page still works.
	}
}
