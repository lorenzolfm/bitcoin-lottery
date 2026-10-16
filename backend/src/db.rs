use diesel::Connection;
use diesel::connection::SimpleConnection;
use diesel::r2d2::{ConnectionManager, CustomizeConnection, Pool};
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

pub type DbPool = Pool<ConnectionManager<SqliteConnection>>;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");
const POOL_SIZE: u32 = 8;

/// Opens the pool and brings the schema up to date before anything else
/// touches the database.
pub fn connect(database_url: &str) -> Result<DbPool, Box<dyn std::error::Error + Send + Sync>> {
    // WAL is stored in the database file, so one connection switches it
    // before the pool opens its connections in parallel and they all race
    // for the lock that switch needs.
    let mut setup = SqliteConnection::establish(database_url)?;
    setup.batch_execute("PRAGMA journal_mode = WAL;")?;
    setup.run_pending_migrations(MIGRATIONS)?;
    let pool = Pool::builder()
        .max_size(POOL_SIZE)
        .connection_customizer(Box::new(ConnectionSettings))
        .build(ConnectionManager::new(database_url))?;
    // `setup` lives until here so an in-memory test database survives until
    // the pool holds it open.
    drop(setup);
    Ok(pool)
}

#[derive(Debug)]
struct ConnectionSettings;

impl CustomizeConnection<SqliteConnection, diesel::r2d2::Error> for ConnectionSettings {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        // SQLite leaves foreign keys off unless every connection asks for them.
        conn.batch_execute(
            "PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA synchronous = NORMAL;",
        )
        .map_err(diesel::r2d2::Error::QueryError)
    }
}

/// A private in-memory database per call, shared by every connection in its pool.
#[cfg(test)]
pub fn test_pool() -> DbPool {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    connect(&format!("file:lottery-test-{id}?mode=memory&cache=shared")).expect("test database")
}
