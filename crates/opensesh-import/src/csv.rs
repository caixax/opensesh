//! Hosts from a CSV file (Sprint 16): any spreadsheet's export, with the columns mapped to host
//! fields by the user (guessed from the headers first).
//!
//! The parser takes RFC 4180's form: fields separated by commas, semicolons or tabs (whichever
//! the first line has most of, outside quotes), quoted with `"` (`""` for a quote, line breaks
//! allowed inside), lines ending in CRLF or LF.

use std::path::Path;

use opensesh_core::hosts::{Host, Protocol};

use crate::common::{ImportWarning, Imported, decode_text, folder_path, split_host_port};

/// The most rows read (a bigger file is cut, with a warning).
pub const MAX_ROWS: usize = 100_000;

/// A CSV file's cells.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Table {
    /// The separator found.
    pub delimiter: char,
    /// Every row, the header row (if any) included.
    pub rows: Vec<Vec<String>>,
    /// Whether rows past [`MAX_ROWS`] were dropped.
    pub truncated: bool,
}

impl Table {
    /// The number of columns (the widest row's).
    #[must_use]
    pub fn columns(&self) -> usize {
        self.rows.iter().map(Vec::len).max().unwrap_or(0)
    }
}

/// What a column holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// Not imported.
    Ignore,
    /// The host's name (the address when empty).
    Name,
    /// Host name or IP address; `user@host:port` is taken apart.
    Address,
    /// Port.
    Port,
    /// User name.
    User,
    /// `ssh`, `rdp`, `vnc`... (empty for SSH).
    Protocol,
    /// The group, folders separated by `/` or `\`.
    Group,
    /// Tags, separated by `,`, `;` or `|`.
    Tags,
    /// Notes.
    Notes,
    /// A private key file.
    IdentityFile,
    /// Jump hosts, separated by `,` or spaces.
    Jump,
}

impl Field {
    /// Every field, in the dialog's order.
    pub const ALL: [Self; 11] = [
        Self::Ignore,
        Self::Name,
        Self::Address,
        Self::Port,
        Self::User,
        Self::Protocol,
        Self::Group,
        Self::Tags,
        Self::Notes,
        Self::IdentityFile,
        Self::Jump,
    ];

    /// Its code for the UI.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Ignore => "ignore",
            Self::Name => "name",
            Self::Address => "address",
            Self::Port => "port",
            Self::User => "user",
            Self::Protocol => "protocol",
            Self::Group => "group",
            Self::Tags => "tags",
            Self::Notes => "notes",
            Self::IdentityFile => "identity_file",
            Self::Jump => "jump",
        }
    }

    /// The field with this code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|field| field.code() == code)
    }

    /// The field a header names, if any.
    #[must_use]
    pub fn guess(header: &str) -> Self {
        let words: String = header
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase();
        match words.as_str() {
            "name" | "label" | "title" | "alias" | "session" | "sessionname" | "displayname"
            | "connection" | "connectionname" => Self::Name,
            "host" | "hostname" | "address" | "hostaddress" | "ip" | "ipaddress" | "server"
            | "servername" | "remotehost" | "fqdn" | "dns" | "target" => Self::Address,
            "port" | "portnumber" => Self::Port,
            "user" | "username" | "login" | "loginname" | "account" => Self::User,
            "protocol" | "type" | "connectiontype" | "kind" | "scheme" => Self::Protocol,
            "group" | "folder" | "path" | "category" | "groupname" | "container" => Self::Group,
            "tags" | "tag" | "labels" | "keywords" => Self::Tags,
            "notes" | "note" | "description" | "comment" | "comments" | "remarks" => Self::Notes,
            "key" | "keyfile" | "identity" | "identityfile" | "privatekey" | "privatekeyfile"
            | "sshkey" => Self::IdentityFile,
            "jump" | "jumphost" | "jumphosts" | "proxyjump" | "bastion" | "gateway" => Self::Jump,
            _ => Self::Ignore,
        }
    }
}

/// Guesses every column from the header row; a field is only given to its first column.
#[must_use]
pub fn guess_columns(headers: &[String]) -> Vec<Field> {
    let mut taken = Vec::new();
    headers
        .iter()
        .map(|header| {
            let field = Field::guess(header);
            if field == Field::Ignore || taken.contains(&field) {
                Field::Ignore
            } else {
                taken.push(field);
                field
            }
        })
        .collect()
}

/// Reads a CSV file.
///
/// # Errors
///
/// When the file can't be read.
pub fn load(path: &Path) -> std::io::Result<Table> {
    // A file the user asked to import.
    let bytes = std::fs::read(path)?;
    Ok(parse(&decode_text(&bytes)))
}

/// The cells of `text`, with the separator guessed from its first line.
#[must_use]
pub fn parse(text: &str) -> Table {
    let delimiter = guess_delimiter(text);
    let (rows, truncated) = parse_with(text, delimiter);
    Table {
        delimiter,
        rows,
        truncated,
    }
}

fn guess_delimiter(text: &str) -> char {
    let mut counts = [(',', 0usize), (';', 0), ('\t', 0)];
    let mut quoted = false;
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            '\n' | '\r' if !quoted => break,
            _ if !quoted => {
                if let Some(count) = counts.iter_mut().find(|(d, _)| *d == c) {
                    count.1 += 1;
                }
            }
            _ => {}
        }
    }
    // Ties go to the first: a comma.
    counts
        .iter()
        .fold(
            (',', 0),
            |best, &(d, n)| if n > best.1 { (d, n) } else { best },
        )
        .0
}

/// The rows of `text` split on `delimiter`, and whether rows were dropped past [`MAX_ROWS`].
#[must_use]
pub fn parse_with(text: &str, delimiter: char) -> (Vec<Vec<String>>, bool) {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                }
            } else {
                cell.push(c);
            }
            continue;
        }
        match c {
            '"' if cell.trim().is_empty() => {
                cell.clear();
                quoted = true;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                row.push(std::mem::take(&mut cell));
                if row.len() > 1 || !row[0].trim().is_empty() {
                    rows.push(std::mem::take(&mut row));
                    if rows.len() == MAX_ROWS {
                        let rest = chars.any(|c| !c.is_whitespace());
                        return (rows, rest);
                    }
                } else {
                    row.clear();
                }
            }
            c if c == delimiter => row.push(std::mem::take(&mut cell)),
            c => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        if row.len() > 1 || !row[0].trim().is_empty() {
            rows.push(row);
        }
    }
    (rows, false)
}

/// The rows as hosts, `mapping` giving each column's field; `skip_header` leaves out the first
/// row. Rows without an address are skipped with a warning.
#[must_use]
pub fn to_hosts(table: &Table, mapping: &[Field], skip_header: bool, origin: &Path) -> Imported {
    let mut imported = Imported::default();
    if table.truncated {
        imported.warnings.push(ImportWarning::new(
            origin,
            format!("only the first {MAX_ROWS} rows were read"),
        ));
    }
    let first = usize::from(skip_header);
    for (index, row) in table.rows.iter().enumerate().skip(first) {
        let line = index + 1;
        let mut warn = |message: String| {
            imported.warnings.push(ImportWarning {
                file: origin.to_path_buf(),
                line,
                message,
            });
        };
        let cell = |field: Field| -> Option<&str> {
            mapping
                .iter()
                .position(|mapped| *mapped == field)
                .and_then(|column| row.get(column))
                .map(|text| text.trim())
                .filter(|text| !text.is_empty())
        };
        let Some(address) = cell(Field::Address) else {
            warn("no address; skipped".to_owned());
            continue;
        };
        let protocol = match cell(Field::Protocol) {
            None => Protocol::Ssh,
            Some(text) => match Protocol::parse(&text.to_ascii_lowercase()) {
                Some(protocol) => protocol,
                None => {
                    warn(format!("unknown protocol {text}; skipped"));
                    continue;
                }
            },
        };
        let (user, address) = match address.rsplit_once('@') {
            Some((user, rest)) if protocol.is_network() => (Some(user.to_owned()), rest),
            _ => (None, address),
        };
        let (address, port) = if protocol.is_network() {
            split_host_port(address)
        } else {
            (address.to_owned(), None)
        };
        let port = match cell(Field::Port) {
            Some(text) => match text.parse::<u16>() {
                Ok(port) => Some(port),
                Err(_) => {
                    warn(format!("the port {text} isn't a number; left out"));
                    port
                }
            },
            None => port,
        };
        let mut host = Host {
            name: cell(Field::Name).unwrap_or(&address).to_owned(),
            protocol,
            port: port.filter(|port| Some(*port) != protocol.default_port() && *port != 0),
            user: cell(Field::User).map(str::to_owned).or(user),
            address,
            notes: cell(Field::Notes).unwrap_or_default().to_owned(),
            identity_file: cell(Field::IdentityFile).map(str::to_owned),
            tags: cell(Field::Tags)
                .map(|tags| {
                    tags.split([',', ';', '|'])
                        .map(str::trim)
                        .filter(|tag| !tag.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            jump: cell(Field::Jump).map(|jumps| {
                jumps
                    .split([',', ' '])
                    .filter(|jump| !jump.is_empty())
                    .map(str::to_owned)
                    .collect()
            }),
            ..Host::default()
        };
        let group = cell(Field::Group).map(|group| {
            group
                .split(['/', '\\'])
                .flat_map(|part| folder_path(part, '/'))
                .collect::<Vec<_>>()
        });
        host.group = group.and_then(|path| imported.group(&path));
        imported.hosts.push(host);
    }
    imported
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_and_separators() {
        let table = parse("name;host\r\n\"a; \"\"quoted\"\"\";h1\r\n\r\nb;\"multi\nline\"\r\n");
        assert_eq!(table.delimiter, ';');
        assert_eq!(
            table.rows,
            [
                vec!["name", "host"],
                vec!["a; \"quoted\"", "h1"],
                vec!["b", "multi\nline"],
            ]
        );
        assert_eq!(parse("a\tb\n1\t2").delimiter, '\t');
        assert_eq!(parse("a,b\n1,2").rows, [vec!["a", "b"], vec!["1", "2"]]);
        assert_eq!(parse("only").rows, [vec!["only"]]);
        assert_eq!(parse("a,,c\n").rows, [vec!["a", "", "c"]]);
        assert_eq!(parse("").rows, Vec::<Vec<String>>::new());
    }

    #[test]
    fn headers_are_guessed_once() {
        let headers: Vec<String> = ["Host Name", "IP", "User-Name", "Folder", "Weird"]
            .iter()
            .map(|h| (*h).to_owned())
            .collect();
        assert_eq!(
            guess_columns(&headers),
            [
                Field::Address,
                Field::Ignore,
                Field::User,
                Field::Group,
                Field::Ignore
            ]
        );
        for field in Field::ALL {
            assert_eq!(Field::from_code(field.code()), Some(field));
        }
    }
}
