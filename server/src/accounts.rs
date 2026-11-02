//! Accounts and sign-in tokens, kept in SQLite. Passwords are stored as Argon2id hashes and
//! tokens only as SHA-256 digests, so the database alone gives neither away.

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use rand::RngExt;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

pub const MIN_PASSWORD: usize = 8;
const MAX_PASSWORD: usize = 256;

#[derive(Clone, Debug, PartialEq)]
pub struct User {
    pub id: i64,
    pub name: String,
}

pub struct Accounts {
    conn: Mutex<Connection>,
    /// Checked against when the name is unknown, so a wrong name takes as long as a wrong password.
    dummy_hash: String,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn digest(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn hash_password(password: &str) -> Result<String> {
    Ok(Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(|e| anyhow::anyhow!("hashing failed: {e}"))?
        .to_string())
}

fn verify(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|parsed| {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

/// Names are what people type to sign in: letters, digits and a few separators.
pub fn check_name(name: &str) -> Result<()> {
    let ok = (1..=64).contains(&name.chars().count())
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '@'));
    if !ok {
        bail!("names are 1 to 64 letters, digits, '.', '_', '-' or '@'");
    }
    Ok(())
}

pub fn check_password(password: &str) -> Result<()> {
    let n = password.chars().count();
    if n < MIN_PASSWORD {
        bail!("passwords need at least {MIN_PASSWORD} characters");
    }
    if n > MAX_PASSWORD {
        bail!("passwords can have at most {MAX_PASSWORD} characters");
    }
    Ok(())
}

/// Account ids name the account's folder of files, so a new account must never get the id of a
/// deleted one. Databases made before ids were kept unique are rebuilt once, keeping every row.
fn never_reuse_ids(conn: &Connection) -> Result<()> {
    let sql: String = conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'users'",
        [],
        |r| r.get(0),
    )?;
    if sql.to_uppercase().contains("AUTOINCREMENT") {
        return Ok(());
    }
    // Foreign keys are off while the table is replaced, or dropping it would drop the tokens.
    conn.execute_batch(
        "PRAGMA foreign_keys = OFF;
         BEGIN;
         CREATE TABLE users_unique (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             name TEXT NOT NULL UNIQUE COLLATE NOCASE,
             hash TEXT NOT NULL,
             created INTEGER NOT NULL
         );
         INSERT INTO users_unique (id, name, hash, created) SELECT id, name, hash, created FROM users;
         DROP TABLE users;
         ALTER TABLE users_unique RENAME TO users;
         COMMIT;
         PRAGMA foreign_keys = ON;",
    )?;
    Ok(())
}

impl Accounts {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS users (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 name TEXT NOT NULL UNIQUE COLLATE NOCASE,
                 hash TEXT NOT NULL,
                 created INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS tokens (
                 hash TEXT PRIMARY KEY,
                 user INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                 created INTEGER NOT NULL,
                 used INTEGER NOT NULL
             );",
        )?;
        never_reuse_ids(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            dummy_hash: hash_password("not a real password")?,
        })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Adds an account. Hashing takes a moment on purpose; call it off the async runtime.
    pub fn add(&self, name: &str, password: &str) -> Result<User> {
        check_name(name)?;
        check_password(password)?;
        let hash = hash_password(password)?;
        let conn = self.conn();
        let taken: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM users WHERE name = ?1)",
            [name],
            |r| r.get(0),
        )?;
        if taken {
            bail!(Taken);
        }
        conn.execute(
            "INSERT INTO users (name, hash, created) VALUES (?1, ?2, ?3)",
            params![name, hash, now()],
        )?;
        Ok(User {
            id: conn.last_insert_rowid(),
            name: name.to_owned(),
        })
    }

    /// The account for a name and password, or `None` when either is wrong.
    pub fn sign_in(&self, name: &str, password: &str) -> Result<Option<User>> {
        let found: Option<(i64, String, String)> = self
            .conn()
            .query_row(
                "SELECT id, name, hash FROM users WHERE name = ?1",
                [name],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((id, name, hash)) = found else {
            verify(password, &self.dummy_hash);
            return Ok(None);
        };
        Ok(verify(password, &hash).then_some(User { id, name }))
    }

    /// Sets a new password and signs the account out everywhere.
    pub fn set_password(&self, name: &str, password: &str) -> Result<()> {
        check_password(password)?;
        let hash = hash_password(password)?;
        let conn = self.conn();
        let id: Option<i64> = conn
            .query_row("SELECT id FROM users WHERE name = ?1", [name], |r| r.get(0))
            .optional()?;
        let Some(id) = id else { bail!(NoSuchUser) };
        conn.execute(
            "UPDATE users SET hash = ?1 WHERE id = ?2",
            params![hash, id],
        )?;
        conn.execute("DELETE FROM tokens WHERE user = ?1", [id])?;
        Ok(())
    }

    /// Removes an account and its tokens; returns its id so its files can go too.
    pub fn remove(&self, name: &str) -> Result<i64> {
        let conn = self.conn();
        let id: Option<i64> = conn
            .query_row("SELECT id FROM users WHERE name = ?1", [name], |r| r.get(0))
            .optional()?;
        let Some(id) = id else { bail!(NoSuchUser) };
        conn.execute("DELETE FROM users WHERE id = ?1", [id])?;
        Ok(id)
    }

    pub fn list(&self) -> Result<Vec<(String, i64)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT name, created FROM users ORDER BY name")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn is_empty(&self) -> Result<bool> {
        Ok(self
            .conn()
            .query_row("SELECT NOT EXISTS(SELECT 1 FROM users)", [], |r| r.get(0))?)
    }

    /// A new sign-in token for the account. Only its digest is kept.
    pub fn issue_token(&self, user: &User) -> Result<String> {
        let mut bytes = [0u8; 32];
        rand::rng().fill(&mut bytes[..]);
        let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        self.conn().execute(
            "INSERT INTO tokens (hash, user, created, used) VALUES (?1, ?2, ?3, ?3)",
            params![digest(&token), user.id, now()],
        )?;
        Ok(token)
    }

    pub fn user_for_token(&self, token: &str) -> Result<Option<User>> {
        let conn = self.conn();
        let hash = digest(token);
        let user = conn
            .query_row(
                "SELECT users.id, users.name FROM tokens JOIN users ON users.id = tokens.user
                 WHERE tokens.hash = ?1",
                [&hash],
                |r| {
                    Ok(User {
                        id: r.get(0)?,
                        name: r.get(1)?,
                    })
                },
            )
            .optional()?;
        if user.is_some() {
            conn.execute(
                "UPDATE tokens SET used = ?1 WHERE hash = ?2",
                params![now(), hash],
            )?;
        }
        Ok(user)
    }

    pub fn revoke_token(&self, token: &str) -> Result<()> {
        self.conn()
            .execute("DELETE FROM tokens WHERE hash = ?1", [digest(token)])?;
        Ok(())
    }
}

/// The name is in use already.
#[derive(Debug)]
pub struct Taken;

impl std::fmt::Display for Taken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("that name is taken")
    }
}

impl std::error::Error for Taken {}

#[derive(Debug)]
pub struct NoSuchUser;

impl std::fmt::Display for NoSuchUser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no such user")
    }
}

impl std::error::Error for NoSuchUser {}

#[cfg(test)]
mod tests {
    use super::*;

    fn accounts() -> (tempfile::TempDir, Accounts) {
        let dir = tempfile::tempdir().unwrap();
        let accounts = Accounts::open(&dir.path().join("a.db")).unwrap();
        (dir, accounts)
    }

    #[test]
    fn a_deleted_accounts_id_is_never_given_again() {
        let (_dir, a) = accounts();
        a.add("alice", "correct horse").unwrap();
        let bob = a.add("bob", "correct horse").unwrap();
        assert_eq!(a.remove("bob").unwrap(), bob.id);
        assert!(a.add("carol", "correct horse").unwrap().id > bob.id);
    }

    #[test]
    fn older_databases_keep_their_accounts_and_stop_reusing_ids() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE COLLATE NOCASE,
                     hash TEXT NOT NULL, created INTEGER NOT NULL);
                 CREATE TABLE tokens (hash TEXT PRIMARY KEY,
                     user INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                     created INTEGER NOT NULL, used INTEGER NOT NULL);",
            )
            .unwrap();
        }
        let a = Accounts::open(&path).unwrap();
        let alice = a.add("alice", "correct horse").unwrap();
        let bob = a.add("bob", "correct horse").unwrap();
        let token = a.issue_token(&alice).unwrap();
        drop(a);
        // Make it old again: the table as it was, with the same rows.
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "PRAGMA foreign_keys = OFF;
                 CREATE TABLE old (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE COLLATE NOCASE,
                     hash TEXT NOT NULL, created INTEGER NOT NULL);
                 INSERT INTO old SELECT * FROM users;
                 DROP TABLE users;
                 ALTER TABLE old RENAME TO users;",
            )
            .unwrap();
        }

        let a = Accounts::open(&path).unwrap();
        assert_eq!(a.user_for_token(&token).unwrap(), Some(alice.clone()));
        assert_eq!(
            a.sign_in("bob", "correct horse").unwrap(),
            Some(bob.clone())
        );
        a.remove("bob").unwrap();
        assert!(a.add("carol", "correct horse").unwrap().id > bob.id);
        // Opening again changes nothing.
        drop(a);
        let a = Accounts::open(&path).unwrap();
        assert_eq!(a.list().unwrap().len(), 2);
    }

    #[test]
    fn sign_in_takes_the_right_password_only() {
        let (_dir, a) = accounts();
        let alice = a.add("alice", "correct horse").unwrap();
        assert_eq!(
            a.sign_in("alice", "correct horse").unwrap(),
            Some(alice.clone())
        );
        assert_eq!(a.sign_in("ALICE", "correct horse").unwrap(), Some(alice));
        assert_eq!(a.sign_in("alice", "wrong horse!").unwrap(), None);
        assert_eq!(a.sign_in("bob", "correct horse").unwrap(), None);
        assert!(a.add("Alice", "another one").unwrap_err().is::<Taken>());
        assert!(a.add("al ice", "correct horse").is_err());
        assert!(a.add("carol", "short").is_err());
    }

    #[test]
    fn tokens_are_stored_hashed_and_die_with_a_new_password() {
        let (_dir, a) = accounts();
        let alice = a.add("alice", "correct horse").unwrap();
        let token = a.issue_token(&alice).unwrap();
        let stored: String = a
            .conn()
            .query_row("SELECT hash FROM tokens", [], |r| r.get(0))
            .unwrap();
        assert_ne!(stored, token);
        assert_eq!(a.user_for_token(&token).unwrap(), Some(alice.clone()));
        a.set_password("alice", "battery staple").unwrap();
        assert_eq!(a.user_for_token(&token).unwrap(), None);

        let token = a.issue_token(&alice).unwrap();
        a.revoke_token(&token).unwrap();
        assert_eq!(a.user_for_token(&token).unwrap(), None);
    }
}
