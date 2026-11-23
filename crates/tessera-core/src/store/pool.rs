//! A small blocking SQLite connection pool (WAL mode, many readers, one writer at a time
//! enforced by SQLite's busy timeout).

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use rusqlite::Connection;

pub struct Pool {
    path: PathBuf,
    max: usize,
    state: Mutex<PoolState>,
    cond: Condvar,
}

struct PoolState {
    idle: Vec<Connection>,
    open: usize,
}

/// A connection borrowed from the pool; returned on drop.
pub struct Conn<'a> {
    pool: &'a Pool,
    conn: Option<Connection>,
}

impl Deref for Conn<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.conn.as_ref().expect("connection present until drop")
    }
}

impl DerefMut for Conn<'_> {
    fn deref_mut(&mut self) -> &mut Connection {
        self.conn.as_mut().expect("connection present until drop")
    }
}

impl Drop for Conn<'_> {
    fn drop(&mut self) {
        if let Some(c) = self.conn.take() {
            let mut st = self.pool.state.lock().unwrap_or_else(|e| e.into_inner());
            st.idle.push(c);
            drop(st);
            self.pool.cond.notify_one();
        }
    }
}

fn open_conn(path: &Path) -> rusqlite::Result<Connection> {
    let c = Connection::open(path)?;
    c.busy_timeout(Duration::from_millis(10000))?;
    c.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;
         PRAGMA cache_size=-65536;
         PRAGMA temp_store=MEMORY;
         PRAGMA mmap_size=268435456;",
    )?;
    Ok(c)
}

impl Pool {
    pub fn open(path: &Path, max: usize) -> rusqlite::Result<Pool> {
        let first = open_conn(path)?;
        Ok(Pool {
            path: path.to_path_buf(),
            max: max.max(1),
            state: Mutex::new(PoolState {
                idle: vec![first],
                open: 1,
            }),
            cond: Condvar::new(),
        })
    }

    pub fn get(&self) -> rusqlite::Result<Conn<'_>> {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(c) = st.idle.pop() {
                return Ok(Conn {
                    pool: self,
                    conn: Some(c),
                });
            }
            if st.open < self.max {
                st.open += 1;
                drop(st);
                return match open_conn(&self.path) {
                    Ok(c) => Ok(Conn {
                        pool: self,
                        conn: Some(c),
                    }),
                    Err(e) => {
                        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
                        st.open -= 1;
                        Err(e)
                    }
                };
            }
            st = self.cond.wait(st).unwrap_or_else(|e| e.into_inner());
        }
    }
}
