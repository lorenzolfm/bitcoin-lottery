-- A round moves open -> closed -> settled. The CHECK below ties every column to
-- the state it belongs to, so a half-settled round cannot be stored.
CREATE TABLE rounds (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 80),
    status TEXT NOT NULL CHECK (status IN ('open', 'closed', 'settled')),
    -- The first block mined after betting closed decides the round.
    target_height INTEGER CHECK (target_height > 0),
    block_hash TEXT CHECK (length(block_hash) = 64),
    nonce BIGINT CHECK (nonce BETWEEN 0 AND 4294967295),
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    closed_at TIMESTAMP,
    settled_at TIMESTAMP,
    CHECK (
        (status = 'open'
            AND target_height IS NULL AND closed_at IS NULL
            AND block_hash IS NULL AND nonce IS NULL AND settled_at IS NULL)
        OR (status = 'closed'
            AND target_height IS NOT NULL AND closed_at IS NOT NULL
            AND block_hash IS NULL AND nonce IS NULL AND settled_at IS NULL)
        OR (status = 'settled'
            AND target_height IS NOT NULL AND closed_at IS NOT NULL
            AND block_hash IS NOT NULL AND nonce IS NOT NULL AND settled_at IS NOT NULL)
    )
);

-- At most one round can be open or waiting for its block at any time.
CREATE UNIQUE INDEX rounds_single_active ON rounds ((status <> 'settled'))
    WHERE status <> 'settled';

CREATE TABLE bets (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    round_id INTEGER NOT NULL REFERENCES rounds (id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 32),
    guess BIGINT NOT NULL CHECK (guess BETWEEN 0 AND 4294967295),
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX bets_round_name ON bets (round_id, name COLLATE NOCASE);

-- Backstop for the application check: no bet may land on a round that has
-- stopped taking bets, even if a close races with a submission.
CREATE TRIGGER bets_require_open_round
BEFORE INSERT ON bets
WHEN (SELECT status FROM rounds WHERE id = NEW.round_id) IS NOT 'open'
BEGIN
    SELECT RAISE(ABORT, 'round is not open for bets');
END;
