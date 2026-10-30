//! Runs the adapter against an in-memory IMAP server that records every
//! command and tracks `\Seen` flags, so the tests can show a scan leaves the
//! mailbox exactly as it was.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};

use super::*;

const PASSWORD: &str = "abcd efgh ijkl mnop";

#[derive(Default)]
struct State {
    uid_validity: u32,
    /// (uid, raw message, seen)
    messages: Vec<(u32, String, bool)>,
    commands: Vec<String>,
    /// Commands the fake doesn't know, i.e. anything that slipped past the guard.
    unexpected: Vec<String>,
    input: Vec<u8>,
    output: VecDeque<u8>,
}

#[derive(Clone)]
struct FakeServer(Arc<Mutex<State>>);

impl FakeServer {
    fn new(messages: &[&str]) -> Self {
        let state = State {
            uid_validity: 7,
            messages: messages
                .iter()
                .enumerate()
                .map(|(i, raw)| (10 + i as u32 * 10, raw.replace('\n', "\r\n"), false))
                .collect(),
            ..State::default()
        };
        Self(Arc::new(Mutex::new(state)))
    }

    fn add(&self, raw: &str) {
        let mut state = self.0.lock().unwrap();
        let uid = state.messages.last().map_or(10, |m| m.0 + 10);
        state.messages.push((uid, raw.replace('\n', "\r\n"), false));
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.lock().unwrap()
    }

    fn provider(&self) -> ImapProvider {
        let mut provider = ImapProvider::new(
            Provider::Gmail,
            "Test Gmail",
            settings(),
            PASSWORD,
            "[Gmail]/All Mail",
        );
        provider.batch_size = 2;
        let server = self.clone();
        provider.connect = Box::new(move |_| {
            server
                .state()
                .output
                .extend(b"* OK [CAPABILITY IMAP4rev1 AUTH=PLAIN] fake ready\r\n");
            Session::greet(Box::new(server.clone()) as Stream)
        });
        provider
    }

    fn respond(state: &mut State, line: &str) {
        let (tag, command) = line.split_once(' ').unwrap_or((line, ""));
        state.commands.push(command.to_string());
        let upper = command.to_ascii_uppercase();
        let mut out = String::new();
        let ok = format!("{tag} OK done\r\n");
        if upper == "CAPABILITY" {
            out += "* CAPABILITY IMAP4rev1 AUTH=PLAIN\r\n";
            out += &ok;
        } else if upper.starts_with("LOGIN ") {
            if command.ends_with(&format!("\"{PASSWORD}\"")) {
                out += &ok;
            } else {
                out += &format!("{tag} NO [AUTHENTICATIONFAILED] Invalid credentials\r\n");
            }
        } else if upper.starts_with("LIST ") {
            out += "* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n";
            out += "* LIST (\\HasChildren \\Noselect) \"/\" \"[Gmail]\"\r\n";
            out += "* LIST (\\All \\HasNoChildren) \"/\" \"[Gmail]/All Mail\"\r\n";
            out += &ok;
        } else if upper.starts_with("EXAMINE ") {
            let next = state.messages.last().map_or(1, |m| m.0 + 1);
            out += &format!("* {} EXISTS\r\n", state.messages.len());
            out += &format!("* OK [UIDVALIDITY {}] UIDs valid\r\n", state.uid_validity);
            out += &format!("* OK [UIDNEXT {next}] Predicted next UID\r\n");
            out += &format!("{tag} OK [READ-ONLY] EXAMINE completed\r\n");
        } else if let Some(rest) = upper.strip_prefix("UID SEARCH UID ") {
            let from: u32 = rest.split(':').next().unwrap().parse().unwrap();
            let mut hits: Vec<u32> = state
                .messages
                .iter()
                .map(|m| m.0)
                .filter(|uid| *uid >= from)
                .collect();
            // Real servers resolve `n:*` to the last message when n is past it.
            if hits.is_empty() {
                hits.extend(state.messages.last().map(|m| m.0));
            }
            let list: Vec<String> = hits.iter().map(u32::to_string).collect();
            out += &format!("* SEARCH {}\r\n", list.join(" "));
            out += &ok;
        } else if let Some(rest) = upper.strip_prefix("UID FETCH ") {
            let (set, items) = rest.split_once(' ').unwrap();
            let marks_seen = items.contains("BODY[");
            for wanted in set.split(',') {
                let wanted: u32 = wanted.parse().unwrap();
                if let Some((seq, message)) = state
                    .messages
                    .iter_mut()
                    .enumerate()
                    .find(|(_, m)| m.0 == wanted)
                {
                    message.2 |= marks_seen;
                    out += &format!(
                        "* {} FETCH (UID {} INTERNALDATE \"29-Oct-2026 12:00:00 +0000\" \
                         BODY[]<0> {{{}}}\r\n{})\r\n",
                        seq + 1,
                        message.0,
                        message.1.len(),
                        message.1
                    );
                }
            }
            out += &ok;
        } else if upper == "LOGOUT" {
            out += "* BYE logging out\r\n";
            out += &ok;
        } else {
            state.unexpected.push(command.to_string());
            out += &ok;
        }
        state.output.extend(out.as_bytes());
    }
}

impl Write for FakeServer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut state = self.state();
        state.input.extend_from_slice(buf);
        while let Some(end) = state.input.windows(2).position(|w| w == b"\r\n") {
            let line: Vec<u8> = state.input.drain(..end + 2).collect();
            let line = String::from_utf8(line).unwrap();
            Self::respond(&mut state, line.trim_end());
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Read for FakeServer {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut state = self.state();
        let n = buf.len().min(state.output.len());
        for (slot, byte) in buf.iter_mut().zip(state.output.drain(..n)) {
            *slot = byte;
        }
        Ok(n)
    }
}

fn settings() -> ImapSettings {
    ImapSettings {
        host: "imap.example.org".into(),
        port: 993,
        security: Security::Tls,
        username: "researcher@example.org".into(),
        folders: vec!["[Gmail]/All Mail".into()],
        backfill_days: 365,
    }
}

fn eml(n: u32) -> String {
    format!(
        "Message-ID: <{n}@example.org>\n\
         From: \"Journal of Synthetic Studies\" <editor@jss.example.org>\n\
         Subject: Decision on manuscript JSS-2026-{n:04}\n\
         Date: Thu, 29 Oct 2026 12:00:00 +0000\n\
         \n\
         Dear author, your manuscript requires major revision.\n"
    )
}

fn sync_all(provider: &mut ImapProvider, mut cursor: Option<SyncCursor>) -> (usize, SyncCursor) {
    let mut seen = 0;
    loop {
        let batch = provider.fetch_changes(cursor.as_ref()).unwrap();
        seen += batch.messages.len();
        cursor = Some(batch.next_cursor);
        if !batch.more {
            return (seen, cursor.unwrap());
        }
    }
}

#[test]
fn scan_leaves_the_mailbox_untouched() {
    let messages: Vec<String> = (1..=5).map(eml).collect();
    let refs: Vec<&str> = messages.iter().map(String::as_str).collect();
    let server = FakeServer::new(&refs);
    let mut provider = server.provider();

    let (count, cursor) = sync_all(&mut provider, None);
    drop(provider);

    assert_eq!(count, 5);
    assert_eq!(
        cursor,
        SyncCursor::Imap {
            folder: "[Gmail]/All Mail".into(),
            uid_validity: 7,
            last_uid: 50
        }
    );
    let state = server.state();
    assert!(
        state.messages.iter().all(|m| !m.2),
        "a message was marked read"
    );
    assert!(state.unexpected.is_empty(), "{:?}", state.unexpected);
    assert!(state
        .commands
        .iter()
        .all(|c| !c.to_ascii_uppercase().starts_with("SELECT")));
    // One login for the whole scan, and a clean logout.
    assert_eq!(
        state
            .commands
            .iter()
            .filter(|c| c.starts_with("LOGIN"))
            .count(),
        1
    );
    assert_eq!(state.commands.last().map(String::as_str), Some("LOGOUT"));
}

#[test]
fn resumes_after_the_last_uid() {
    let server = FakeServer::new(&[&eml(1), &eml(2)]);
    let mut provider = server.provider();
    let (_, cursor) = sync_all(&mut provider, None);

    let (again, cursor) = sync_all(&mut provider, Some(cursor));
    assert_eq!(again, 0, "nothing new since the last scan");

    server.add(&eml(3));
    let batch = provider.fetch_changes(Some(&cursor)).unwrap();
    assert_eq!(batch.messages.len(), 1);
    assert_eq!(batch.messages[0].source.message_id, "<3@example.org>");
}

#[test]
fn new_uidvalidity_expires_the_cursor() {
    let server = FakeServer::new(&[&eml(1)]);
    let mut provider = server.provider();
    let (_, cursor) = sync_all(&mut provider, None);
    server.state().uid_validity = 8;
    assert_eq!(
        provider.fetch_changes(Some(&cursor)).unwrap_err(),
        ProviderError::CursorExpired
    );
}

#[test]
fn empty_folder_skips_ahead_to_new_mail() {
    let server = FakeServer::new(&[]);
    let mut provider = server.provider();
    let batch = provider.fetch_changes(None).unwrap();
    assert!(batch.messages.is_empty());
    assert!(!batch.more);
}

#[test]
fn wrong_password_is_a_permission_error() {
    let server = FakeServer::new(&[&eml(1)]);
    let mut provider = server.provider();
    provider.password = "wrong".into();
    let error = provider.fetch_changes(None).unwrap_err();
    assert!(
        matches!(error, ProviderError::PermissionDenied(_)),
        "{error:?}"
    );
    assert!(
        !error.to_string().contains("wrong"),
        "password leaked into the error"
    );
}

#[test]
fn guard_stops_mutating_commands_before_they_are_sent() {
    let server = FakeServer::new(&[&eml(1)]);
    let mut provider = server.provider();
    let session = provider.session().unwrap();
    for command in [
        "SELECT INBOX",
        "UID STORE 10 +FLAGS (\\Seen)",
        "UID FETCH 10 (BODY[])",
    ] {
        assert!(matches!(session.run(command), Err(ImapError::Guard(_))));
    }
    assert!(server.state().unexpected.is_empty());
    assert!(server.state().messages.iter().all(|m| !m.2));
}

#[test]
fn lists_folders_and_finds_all_mail() {
    let server = FakeServer::new(&[]);
    let mut provider = server.provider();
    let folders = provider.session().unwrap().list().unwrap();
    let all: Vec<_> = folders.iter().filter(|f| f.all_mail).collect();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "[Gmail]/All Mail");
    assert!(
        !folders
            .iter()
            .find(|f| f.name == "[Gmail]")
            .unwrap()
            .selectable
    );
}

#[test]
fn decodes_mime_and_german_charsets() {
    let raw = "Message-ID: <de-1@example.org>\n\
               From: =?ISO-8859-1?Q?Zeitschrift_f=FCr_Synthetische_Studien?= <redaktion@zss.example.org>\n\
               Subject: =?UTF-8?Q?Entscheidung_=C3=BCber_Ihr_Manuskript?=\n\
               Date: Mon, 26 Oct 2026 09:30:00 +0100\n\
               MIME-Version: 1.0\n\
               Content-Type: multipart/alternative; boundary=\"b1\"\n\
               \n\
               --b1\n\
               Content-Type: text/plain; charset=ISO-8859-1\n\
               Content-Transfer-Encoding: quoted-printable\n\
               \n\
               Ihr Manuskript ZSS-2026-077 wurde angenommen. Bitte senden Sie die Lizenz bis zum 03.11.2026 zur=FCck.\n\
               --b1\n\
               Content-Type: text/html; charset=UTF-8\n\
               \n\
               <p>HTML version</p>\n\
               --b1--\n";
    let server = FakeServer::new(&[raw]);
    let mut provider = server.provider();
    let batch = provider.fetch_changes(None).unwrap();
    let message = &batch.messages[0];
    assert_eq!(message.subject, "Entscheidung über Ihr Manuskript");
    assert_eq!(
        message.from,
        "\"Zeitschrift für Synthetische Studien\" <redaktion@zss.example.org>"
    );
    // No body in the failure message: bodies never reach logs, not even in CI.
    assert!(message.body.contains("zurück"));
    assert!(!message.body.contains("HTML version"));
    assert_eq!(message.source.received_at, 1_793_003_400);
}

#[test]
fn message_without_id_gets_a_stable_one() {
    let raw = "From: editor@jss.example.org\nSubject: Decision\n\nText\n";
    let server = FakeServer::new(&[raw]);
    let mut provider = server.provider();
    let batch = provider.fetch_changes(None).unwrap();
    assert_eq!(
        batch.messages[0].source.message_id,
        "<7.10@[Gmail]/All Mail.imap.invalid>"
    );
    // No Date header: the server's INTERNALDATE is used.
    assert_eq!(batch.messages[0].source.received_at, 1_793_275_200);
}

#[test]
fn formats_search_dates() {
    assert_eq!(imap_date(1_793_275_200), "29-Oct-2026");
    assert_eq!(imap_date(0), "1-Jan-1970");
}

/// Live TLS check without logging in: `MH_IMAP_HOST=imap.example.org cargo test -p mh-connectors live_tls -- --ignored`
#[test]
#[ignore = "needs network access"]
fn live_tls_handshake() {
    let host = std::env::var("MH_IMAP_HOST").expect("set MH_IMAP_HOST");
    let mut session = transport::connect(&ImapSettings { host, ..settings() }).unwrap();
    session.refresh_capabilities().unwrap();
    assert!(session.has_capability("IMAP4rev1"));
    session.logout();
}
