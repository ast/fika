//! hamlib rigctld client over its plain TCP line protocol.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::rig::Rig;

pub struct Rigctld {
    addr: String,
    conn: Option<BufReader<TcpStream>>,
}

impl Rigctld {
    pub fn new(addr: &str) -> Self {
        Self {
            addr: addr.to_string(),
            conn: None,
        }
    }

    fn connect(&mut self) -> Result<()> {
        if self.conn.is_some() {
            return Ok(());
        }
        let sock = self
            .addr
            .to_socket_addrs()
            .with_context(|| format!("resolve {}", self.addr))?
            .next()
            .context("no address")?;
        let stream = TcpStream::connect_timeout(&sock, Duration::from_millis(500))
            .with_context(|| format!("connect rigctld at {}", self.addr))?;
        stream.set_read_timeout(Some(Duration::from_millis(1000)))?;
        stream.set_write_timeout(Some(Duration::from_millis(500)))?;
        self.conn = Some(BufReader::new(stream));
        Ok(())
    }

    /// Send one command and return the reply lines (until RPRT or one value).
    fn command(&mut self, cmd: &str, expect_lines: usize) -> Result<Vec<String>> {
        self.connect()?;
        let result = (|| -> Result<Vec<String>> {
            let conn = self.conn.as_mut().unwrap();
            conn.get_mut().write_all(format!("{cmd}\n").as_bytes())?;
            let mut lines = Vec::new();
            for _ in 0..expect_lines {
                let mut line = String::new();
                if conn.read_line(&mut line)? == 0 {
                    bail!("rigctld closed the connection");
                }
                lines.push(line.trim().to_string());
            }
            Ok(lines)
        })();
        if result.is_err() {
            self.conn = None;
        }
        result
    }

    fn check_rprt(lines: &[String], what: &str) -> Result<()> {
        match lines.first().map(String::as_str) {
            Some("RPRT 0") => Ok(()),
            other => bail!("{what}: rigctld replied {other:?}"),
        }
    }
}

impl Rig for Rigctld {
    fn name(&self) -> String {
        format!("rigctld {}", self.addr)
    }

    fn ptt(&mut self, on: bool) -> Result<()> {
        let lines = self.command(if on { "T 1" } else { "T 0" }, 1)?;
        Self::check_rprt(&lines, "PTT")
    }

    fn frequency(&mut self) -> Result<Option<u64>> {
        let lines = self.command("f", 1)?;
        let f = lines.first().and_then(|l| l.parse::<u64>().ok());
        Ok(f)
    }

    fn set_data_mode(&mut self) -> Result<()> {
        let lines = self.command("M PKTUSB 3000", 1)?;
        Self::check_rprt(&lines, "set mode")
    }

    fn connected(&self) -> bool {
        self.conn.is_some()
    }
}
