export type RoundStatus = 'open' | 'closed' | 'settled';

export interface Bet {
	id: number;
	name: string;
	/** `null` while betting is open: guesses stay secret until it closes. */
	guess: number | null;
	distance: number | null;
	/** 1 is the winner. Only set once the round is settled. */
	rank: number | null;
	created_at: string;
}

export interface Round {
	id: number;
	title: string;
	status: RoundStatus;
	created_at: string;
	closed_at: string | null;
	settled_at: string | null;
	/** The first block mined after betting closed. */
	target_height: number | null;
	block: { height: number; hash: string; nonce: number } | null;
	tip_height: number | null;
	/** Submission order while unsettled; closest first once settled. */
	bets: Bet[];
}

export interface RoundSummary {
	id: number;
	title: string;
	status: RoundStatus;
	created_at: string;
	target_height: number | null;
	nonce: number | null;
	bet_count: number;
	winner: string | null;
}

export const MAX_GUESS = 4_294_967_295;
export const NAME_MAX_CHARS = 32;

export class ApiError extends Error {
	constructor(
		readonly status: number,
		message: string
	) {
		super(message);
	}
}

async function request<T>(
	method: string,
	path: string,
	body?: unknown,
	token?: string
): Promise<T> {
	const headers: Record<string, string> = {};
	if (body !== undefined) headers['content-type'] = 'application/json';
	if (token) headers.authorization = `Bearer ${token}`;
	const response = await fetch(`/api${path}`, {
		method,
		headers,
		body: body === undefined ? undefined : JSON.stringify(body)
	});
	if (!response.ok) {
		const payload = await response.json().catch(() => null);
		throw new ApiError(response.status, payload?.error ?? `request failed (${response.status})`);
	}
	return response.status === 204 ? (undefined as T) : response.json();
}

export const api = {
	currentRound: () => request<Round | null>('GET', '/rounds/current'),
	round: (id: number) => request<Round>('GET', `/rounds/${id}`),
	rounds: () => request<RoundSummary[]>('GET', '/rounds'),
	placeBet: (roundId: number, name: string, guess: number) =>
		request<Bet>('POST', `/rounds/${roundId}/bets`, { name, guess })
};

export const admin = {
	checkToken: (token: string) => request<void>('GET', '/admin/session', undefined, token),
	createRound: (token: string, title: string) =>
		request<Round>('POST', '/admin/rounds', { title }, token),
	closeRound: (token: string, id: number) =>
		request<Round>('POST', `/admin/rounds/${id}/close`, undefined, token),
	deleteBet: (token: string, id: number) =>
		request<void>('DELETE', `/admin/bets/${id}`, undefined, token)
};

export function errorMessage(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}
