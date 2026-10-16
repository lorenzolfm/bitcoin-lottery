# BitDevs Lottery

A guessing game for the start of the meetup. Everyone opens the site on their
phone, picks a name and guesses a number. When the host closes betting, the
**next block mined** decides: whoever guessed closest to that block's nonce
wins.

- **Frontend:** SvelteKit (built as a static single-page app)
- **Backend:** Rust, axum, Diesel and SQLite, in one binary that also serves the frontend
- **Chain data:** any Esplora API, mempool.space by default

## How a round works

1. **Open.** The host opens a round on `/admin`. People bet from `/`. Guesses
   are sealed: the API returns only names until betting closes, so nobody can
   place a bet just beside someone else's.
2. **Closed.** The host presses _Close betting_. The server reads the chain tip
   and records `tip + 1` as the target block. Nobody can know that block's
   nonce when betting closes.
3. **Settled.** A background task polls the explorer. Once the target block
   exists, it stores the block's hash and nonce, and the page shows the winner.

Rules: a nonce is a 32-bit number (0 to 4,294,967,295). The closest guess wins
in either direction. On a tie, the earlier bet wins. Names are unique within a
round, ignoring case. Only one round can run at a time.

The host can remove a bet (say, a name that shouldn't be on the projector)
any time before the round settles. Settled rounds are final.

## Run it

You need Rust and Node.js. The SQLite library is compiled in, so you don't
need a system one.

```sh
cd frontend && npm install && npm run build && cd ..
cd backend
ADMIN_TOKEN='pick-something-long' cargo run --release
```

Open <http://localhost:3000>. Other devices on the same network can use
`http://<your-ip>:3000`. The database is created and migrated on startup.

### Development

Run both halves with hot reload. Vite proxies `/api` to the backend:

```sh
# terminal 1
cd backend && ADMIN_TOKEN='dev-token-1234' cargo run
# terminal 2
cd frontend && npm run dev
```

Then use <http://localhost:5173>. To try a full round without waiting for
mainnet, point the backend at a test network:
`ESPLORA_URL=https://mempool.space/testnet4/api`.

### Configuration

| Variable        | Default                     | Meaning                                       |
| --------------- | --------------------------- | --------------------------------------------- |
| `ADMIN_TOKEN`   | _(required, ≥ 12 chars)_    | Password for the host page                    |
| `DATABASE_URL`  | `lottery.db`                | SQLite file                                   |
| `BIND_ADDR`     | `0.0.0.0:3000`              | Listen address                                |
| `ESPLORA_URL`   | `https://mempool.space/api` | Esplora API (point it at your own mempool)    |
| `POLL_SECONDS`  | `15`                        | How often to check for the block              |
| `STATIC_DIR`    | `../frontend/build`         | Built frontend; API-only if it doesn't exist  |
| `RUST_LOG`      | `info`                      | Log filter                                    |

## Tests

```sh
cd backend && cargo test         # game rules, storage, API, settlement
cd frontend && npm run check     # types
```

The settlement and API tests run against an in-process fake Esplora server,
so they need no network.

## Layout

```
backend/
  migrations/      schema: CHECK constraints tie each column to its phase
  src/lottery.rs   rules: phases, validation, who wins
  src/store.rs     Diesel queries; every state change is one transaction
  src/chain.rs     Esplora client
  src/settler.rs   background task that settles closed rounds
  src/api.rs       HTTP routes and JSON shapes
frontend/src/
  routes/          / (play), /rounds (history), /admin (host)
  lib/             API client, round board, number line
```

## Things to know

- **Reorgs:** the round settles on the first block the explorer reports at the
  target height. A one-block reorg after that won't change the result.
- **Explorer lag:** the target is `tip + 1` according to the explorer. If it is
  a few seconds behind, closing in that window targets a block that already
  exists. For a room of people betting by hand this is negligible.
