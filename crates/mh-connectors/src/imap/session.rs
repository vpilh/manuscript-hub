//! A minimal IMAP4rev1 client: just the read-only commands the adapter needs.
//!
//! [`Session::run`] is the only place that writes a command, and it passes
//! every command through [`check_command`] first, so nothing that could
//! change the mailbox ever reaches the server (FR06).

use std::io::{BufRead, BufReader, Read, Write};

use imap_proto::{
    AttributeValue, Capability, MailboxDatum, NameAttribute, Response, ResponseCode, Status,
};

use super::ImapError;
use crate::imap_guard::check_command;

/// Upper bound for one server response, literals included. Fetches are
/// partial (see [`super::FETCH_LIMIT`]), so real responses stay far below this.
const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

pub(crate) struct Session<S: Read + Write> {
    stream: BufReader<S>,
    next_tag: u32,
    capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Folder {
    /// The name as the server spells it (modified UTF-7 for non-ASCII names).
    pub name: String,
    /// RFC 6154 `\All`: Gmail's "All Mail", which holds every label.
    pub all_mail: bool,
    pub selectable: bool,
}

pub(crate) struct MailboxInfo {
    pub uid_validity: Option<u32>,
    pub uid_next: Option<u32>,
}

pub(crate) struct Fetched {
    pub uid: u32,
    pub internal_date: Option<String>,
    pub raw: Vec<u8>,
}

impl<S: Read + Write> Session<S> {
    /// Reads the server greeting of a fresh connection.
    pub fn greet(stream: S) -> Result<Self, ImapError> {
        let mut session = Self::resume(stream);
        let greeting = session.read_response()?;
        match Response::parse(&greeting) {
            Ok((
                _,
                Response::Data {
                    status: Status::Ok,
                    outcome,
                },
            ))
            | Ok((
                _,
                Response::Data {
                    status: Status::PreAuth,
                    outcome,
                },
            )) => {
                if let Some(ResponseCode::Capabilities(caps)) = outcome.code {
                    session.capabilities = caps.iter().map(capability_name).collect();
                }
                Ok(session)
            }
            Ok((
                _,
                Response::Data {
                    status: Status::Bye,
                    ..
                },
            )) => Err(ImapError::Closed),
            _ => Err(ImapError::Protocol("unexpected server greeting".into())),
        }
    }

    /// Continues on a stream whose greeting was already read (after STARTTLS).
    pub fn resume(stream: S) -> Self {
        Self {
            stream: BufReader::new(stream),
            next_tag: 1,
            capabilities: Vec::new(),
        }
    }

    /// Hands back the stream for the TLS upgrade. Fails if the server sent
    /// anything after its STARTTLS reply: such bytes would be injected into
    /// the session from outside the encrypted channel.
    pub fn into_stream(self) -> Result<S, ImapError> {
        if !self.stream.buffer().is_empty() {
            return Err(ImapError::Protocol(
                "server sent data before the TLS handshake".into(),
            ));
        }
        Ok(self.stream.into_inner())
    }

    pub fn has_capability(&self, name: &str) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.eq_ignore_ascii_case(name))
    }

    pub fn refresh_capabilities(&mut self) -> Result<(), ImapError> {
        let responses = self.run("CAPABILITY")?;
        for response in responses {
            if let Response::Capabilities(caps) = response {
                self.capabilities = caps.iter().map(capability_name).collect();
            }
        }
        Ok(())
    }

    pub fn starttls(&mut self) -> Result<(), ImapError> {
        self.run("STARTTLS").map(drop)
    }

    pub fn login(&mut self, username: &str, password: &str) -> Result<(), ImapError> {
        if self.has_capability("LOGINDISABLED") {
            return Err(ImapError::LoginDisabled);
        }
        let command = format!("LOGIN {} {}", quote(username)?, quote(password)?);
        match self.run(&command) {
            Ok(responses) => {
                // Servers often announce new capabilities in the tagged OK.
                self.capabilities.clear();
                for response in responses {
                    if let Response::Capabilities(caps) = response {
                        self.capabilities = caps.iter().map(capability_name).collect();
                    }
                }
                Ok(())
            }
            Err(ImapError::Rejected { text, .. }) => Err(ImapError::LoginRejected(text)),
            Err(e) => Err(e),
        }
    }

    pub fn list(&mut self) -> Result<Vec<Folder>, ImapError> {
        let responses = self.run("LIST \"\" \"*\"")?;
        Ok(responses
            .into_iter()
            .filter_map(|response| match response {
                Response::MailboxData(MailboxDatum::List(data)) => Some(Folder {
                    name: data.name.into_owned(),
                    all_mail: data.name_attributes.contains(&NameAttribute::All),
                    selectable: !data.name_attributes.contains(&NameAttribute::NoSelect),
                }),
                _ => None,
            })
            .collect())
    }

    /// Opens a folder read-only. There is deliberately no `select`.
    pub fn examine(&mut self, folder: &str) -> Result<MailboxInfo, ImapError> {
        let responses = self.run(&format!("EXAMINE {}", quote(folder)?))?;
        let mut info = MailboxInfo {
            uid_validity: None,
            uid_next: None,
        };
        for response in responses {
            if let Response::Data {
                outcome:
                    imap_proto::Outcome {
                        code: Some(code), ..
                    },
                ..
            } = response
            {
                match code {
                    ResponseCode::UidValidity(v) => info.uid_validity = Some(v),
                    ResponseCode::UidNext(n) => info.uid_next = Some(n),
                    _ => {}
                }
            }
        }
        Ok(info)
    }

    pub fn uid_search(&mut self, criteria: &str) -> Result<Vec<u32>, ImapError> {
        let responses = self.run(&format!("UID SEARCH {criteria}"))?;
        let mut uids: Vec<u32> = responses
            .into_iter()
            .filter_map(|response| match response {
                Response::MailboxData(MailboxDatum::Search(uids)) => Some(uids),
                _ => None,
            })
            .flatten()
            .collect();
        uids.sort_unstable();
        uids.dedup();
        Ok(uids)
    }

    /// Fetches messages without marking them read: `items` must use `BODY.PEEK`,
    /// which [`check_command`] enforces.
    pub fn uid_fetch(&mut self, uids: &[u32], items: &str) -> Result<Vec<Fetched>, ImapError> {
        let set = uids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let responses = self.run(&format!("UID FETCH {set} ({items})"))?;
        let mut fetched = Vec::new();
        for response in responses {
            let Response::Fetch(_, attributes) = response else {
                continue;
            };
            let mut uid = None;
            let mut internal_date = None;
            let mut raw = None;
            for attribute in attributes {
                match attribute {
                    AttributeValue::Uid(u) => uid = Some(u),
                    AttributeValue::InternalDate(d) => internal_date = Some(d.into_owned()),
                    AttributeValue::BodySection { data: Some(d), .. } => raw = Some(d.into_owned()),
                    _ => {}
                }
            }
            // Unsolicited FETCH responses (flag updates) carry no body; skip them.
            if let (Some(uid), Some(raw)) = (uid, raw) {
                fetched.push(Fetched {
                    uid,
                    internal_date,
                    raw,
                });
            }
        }
        Ok(fetched)
    }

    pub fn logout(&mut self) {
        // Best effort: the connection closes either way.
        let _ = self.run("LOGOUT");
    }

    /// Sends one command and collects its untagged responses until the tagged
    /// completion. The single write path of this client.
    pub(super) fn run(&mut self, command: &str) -> Result<Vec<Response<'static>>, ImapError> {
        check_command(command).map_err(ImapError::Guard)?;
        let tag = format!("M{}", self.next_tag);
        self.next_tag += 1;
        let stream = self.stream.get_mut();
        stream.write_all(format!("{tag} {command}\r\n").as_bytes())?;
        stream.flush()?;

        let mut responses = Vec::new();
        loop {
            let raw = self.read_response()?;
            match Response::parse(&raw) {
                Ok((
                    _,
                    Response::Done {
                        tag: done,
                        status,
                        outcome,
                    },
                )) if done.0 == tag => {
                    return match status {
                        Status::Ok => {
                            if let Some(ResponseCode::Capabilities(caps)) = outcome.code {
                                responses.push(Response::Capabilities(
                                    caps.into_iter().map(Capability::into_owned).collect(),
                                ));
                            }
                            Ok(responses)
                        }
                        _ => Err(ImapError::Rejected {
                            command: verb(command),
                            text: server_text(outcome.information.as_deref()),
                        }),
                    };
                }
                Ok((
                    _,
                    Response::Data {
                        status: Status::Bye,
                        ..
                    },
                )) if verb(command) != "LOGOUT" => return Err(ImapError::Closed),
                Ok((_, response)) => responses.push(response.into_owned()),
                // Untagged data this client doesn't model is safe to skip, but
                // our own completion must never be missed.
                Err(_) if raw.starts_with(format!("{tag} ").as_bytes()) => {
                    let text = String::from_utf8_lossy(&raw[tag.len() + 1..]);
                    let ok = text.get(..3).is_some_and(|s| s.eq_ignore_ascii_case("OK "));
                    return if ok {
                        Ok(responses)
                    } else {
                        Err(ImapError::Rejected {
                            command: verb(command),
                            text: server_text(Some(text.trim())),
                        })
                    };
                }
                Err(_) => {}
            }
        }
    }

    /// Reads one complete response: a line plus any `{n}` literals it announces.
    fn read_response(&mut self) -> Result<Vec<u8>, ImapError> {
        let mut raw = Vec::new();
        loop {
            let start = raw.len();
            let read = self.stream.read_until(b'\n', &mut raw)?;
            if read == 0 {
                return Err(ImapError::Closed);
            }
            match literal_length(&raw[start..]) {
                Some(length) => {
                    if raw.len() + length > MAX_RESPONSE_BYTES {
                        return Err(ImapError::Protocol("server response too large".into()));
                    }
                    let offset = raw.len();
                    raw.resize(offset + length, 0);
                    self.stream.read_exact(&mut raw[offset..])?;
                }
                None => return Ok(raw),
            }
            if raw.len() > MAX_RESPONSE_BYTES {
                return Err(ImapError::Protocol("server response too large".into()));
            }
        }
    }
}

/// `... {123}\r\n` announces a 123-byte literal after the line break.
fn literal_length(line: &[u8]) -> Option<usize> {
    let line = line
        .strip_suffix(b"\r\n")
        .or_else(|| line.strip_suffix(b"\n"))?;
    let inner = line.strip_suffix(b"}")?;
    let open = inner.iter().rposition(|&b| b == b'{')?;
    std::str::from_utf8(&inner[open + 1..]).ok()?.parse().ok()
}

/// An IMAP quoted string. Line breaks and non-ASCII text would need literals,
/// which this client doesn't send.
fn quote(value: &str) -> Result<String, ImapError> {
    if !value.chars().all(|c| c.is_ascii() && !c.is_ascii_control()) {
        return Err(ImapError::UnsupportedText);
    }
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

fn verb(command: &str) -> String {
    command
        .split_ascii_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase()
}

/// Server text is shown to the user, so keep it short and printable.
fn server_text(text: Option<&str>) -> String {
    text.unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect()
}

fn capability_name(capability: &Capability<'_>) -> String {
    match capability {
        Capability::Imap4rev1 => "IMAP4rev1".into(),
        Capability::Auth(mechanism) => format!("AUTH={mechanism}"),
        Capability::Atom(atom) => atom.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_literal_lengths() {
        assert_eq!(literal_length(b"* 1 FETCH (BODY[] {42}\r\n"), Some(42));
        assert_eq!(literal_length(b"* OK done\r\n"), None);
        assert_eq!(literal_length(b"* OK {x}\r\n"), None);
    }

    #[test]
    fn quotes_and_escapes() {
        assert_eq!(quote(r#"a"b\c"#).unwrap(), r#""a\"b\\c""#);
        assert!(matches!(quote("Entwürfe"), Err(ImapError::UnsupportedText)));
        assert!(matches!(quote("a\r\nb"), Err(ImapError::UnsupportedText)));
    }
}
