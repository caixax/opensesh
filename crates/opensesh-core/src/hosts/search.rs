//! Fuzzy search, filters and sorting of the host list (PLAN §9: under 16 ms for 1000 hosts).
//!
//! The query is matched with nucleo's matcher against one line per host (name, address, user,
//! tags and group), so several words can match different fields; a match in the name counts
//! extra. Without a query the list is sorted by the chosen order.

use std::cmp::Ordering;
use std::collections::HashMap;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use super::recent::RecentList;
use super::{Group, Host, HostsFile, Protocol};

/// Added to the score of a host whose name or address is exactly the query.
const EXACT_BONUS: u32 = 100_000;

/// Added when the query appears as typed in the host's line.
const SUBSTRING_BONUS: u32 = 10_000;

/// Which hosts are listed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Scope {
    /// Every host.
    #[default]
    All,
    /// Favorites.
    Favorites,
    /// Hosts connected to recently, the newest first.
    Recent,
    /// A group and its subgroups.
    Group(String),
    /// Saved hosts in no group.
    Ungrouped,
    /// Hosts linked from `~/.ssh/config`.
    Linked,
}

/// Order without a query.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Sort {
    /// By name (numbers in names in numeric order).
    #[default]
    Name,
    /// By address.
    Address,
    /// The most recently used first; never used ones last, by name.
    LastUsed,
    /// By group path, then name.
    Group,
}

impl Sort {
    /// The sort named `text` (`name`, `address`, `recent`, `group`), else by name.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text {
            "address" => Self::Address,
            "recent" => Self::LastUsed,
            "group" => Self::Group,
            _ => Self::Name,
        }
    }
}

/// A search.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// Fuzzy text; empty lists everything in scope.
    pub text: String,
    /// Which hosts.
    pub scope: Scope,
    /// Only this protocol.
    pub protocol: Option<Protocol>,
    /// Only hosts with this tag (ignoring case).
    pub tag: Option<String>,
    /// Order without text.
    pub sort: Sort,
}

/// Runs searches, reusing the matcher's memory.
pub struct Searcher {
    matcher: Matcher,
    buffer: Vec<char>,
    line: String,
}

impl Default for Searcher {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Searcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Searcher")
    }
}

impl Searcher {
    /// A searcher with nucleo's default scoring.
    #[must_use]
    pub fn new() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
            buffer: Vec::new(),
            line: String::new(),
        }
    }

    /// The indexes (into `file.hosts`) of the hosts that match, best first.
    pub fn search(&mut self, file: &HostsFile, recent: &RecentList, query: &Query) -> Vec<usize> {
        let paths = group_paths(&file.groups, file);
        let scope_groups = match &query.scope {
            Scope::Group(id) => Some(file.subtree(id)),
            _ => None,
        };
        let recent_rank: HashMap<&str, usize> = recent
            .entries()
            .iter()
            .enumerate()
            .filter(|(_, entry)| !entry.host.is_empty())
            .map(|(rank, entry)| (entry.host.as_str(), rank))
            .collect();
        let tag = query.tag.as_deref().map(str::to_lowercase);
        let in_scope = |host: &Host| -> bool {
            let scope = match &query.scope {
                Scope::All => true,
                Scope::Favorites => host.favorite,
                Scope::Recent => recent_rank.contains_key(host.id.as_str()),
                Scope::Group(_) => host.group.as_ref().is_some_and(|group| {
                    scope_groups.as_ref().is_some_and(|set| set.contains(group))
                }),
                Scope::Ungrouped => host.group.is_none() && !host.is_linked(),
                Scope::Linked => host.is_linked(),
            };
            scope
                && query
                    .protocol
                    .is_none_or(|protocol| host.protocol == protocol)
                && tag
                    .as_ref()
                    .is_none_or(|tag| host.tags.iter().any(|own| own.to_lowercase() == *tag))
        };
        let text = query.text.trim();
        let mut found: Vec<(usize, u32)> = Vec::new();
        if text.is_empty() {
            found.extend(
                file.hosts
                    .iter()
                    .enumerate()
                    .filter(|(_, host)| in_scope(host))
                    .map(|(index, _)| (index, 0)),
            );
        } else {
            let pattern = Pattern::parse(text, CaseMatching::Ignore, Normalization::Smart);
            let lower = text.to_lowercase();
            for (index, host) in file.hosts.iter().enumerate() {
                if !in_scope(host) {
                    continue;
                }
                self.line.clear();
                self.line.push_str(&host.name);
                self.line.push(' ');
                self.line.push_str(&host.address);
                if let Some(user) = &host.user {
                    self.line.push(' ');
                    self.line.push_str(user);
                }
                for tag in &host.tags {
                    self.line.push(' ');
                    self.line.push_str(tag);
                }
                if let Some(path) = host.group.as_ref().and_then(|group| paths.get(group)) {
                    self.line.push(' ');
                    self.line.push_str(path);
                }
                let Some(score) = pattern.score(
                    Utf32Str::new(&self.line, &mut self.buffer),
                    &mut self.matcher,
                ) else {
                    continue;
                };
                let name_bonus = pattern
                    .score(
                        Utf32Str::new(&host.name, &mut self.buffer),
                        &mut self.matcher,
                    )
                    .unwrap_or(0);
                // The text as typed beats a looser fuzzy match: exactly a name or address
                // first, then text found as it is.
                let exact = if host.name.eq_ignore_ascii_case(text)
                    || host.address.eq_ignore_ascii_case(text)
                {
                    EXACT_BONUS
                } else if self.line.to_lowercase().contains(&lower) {
                    SUBSTRING_BONUS
                } else {
                    0
                };
                found.push((index, score + name_bonus + exact));
            }
        }
        let hosts = &file.hosts;
        let by_name = |a: usize, b: usize| natural_cmp(&hosts[a].name, &hosts[b].name);
        if !text.is_empty() {
            found.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| by_name(a.0, b.0)));
        } else if query.scope == Scope::Recent || query.sort == Sort::LastUsed {
            let rank = |index: usize| {
                recent_rank
                    .get(hosts[index].id.as_str())
                    .copied()
                    .unwrap_or(usize::MAX)
            };
            found.sort_by(|a, b| rank(a.0).cmp(&rank(b.0)).then_with(|| by_name(a.0, b.0)));
        } else {
            match query.sort {
                Sort::Name | Sort::LastUsed => found.sort_by(|a, b| by_name(a.0, b.0)),
                Sort::Address => found.sort_by(|a, b| {
                    natural_cmp(&hosts[a.0].address, &hosts[b.0].address)
                        .then_with(|| by_name(a.0, b.0))
                }),
                Sort::Group => {
                    let path = |index: usize| {
                        hosts[index]
                            .group
                            .as_ref()
                            .and_then(|group| paths.get(group))
                            .map_or("", String::as_str)
                    };
                    found.sort_by(|a, b| {
                        natural_cmp(path(a.0), path(b.0)).then_with(|| by_name(a.0, b.0))
                    });
                }
            }
        }
        found.into_iter().map(|(index, _)| index).collect()
    }
}

fn group_paths(groups: &[Group], file: &HostsFile) -> HashMap<String, String> {
    groups
        .iter()
        .map(|group| (group.id.clone(), file.group_path(&group.id)))
        .collect()
}

/// Every tag with the number of hosts that have it, by name (ignoring case).
#[must_use]
pub fn tags(file: &HostsFile) -> Vec<(String, usize)> {
    let mut counts: HashMap<String, (String, usize)> = HashMap::new();
    for host in &file.hosts {
        for tag in &host.tags {
            counts
                .entry(tag.to_lowercase())
                .or_insert_with(|| (tag.clone(), 0))
                .1 += 1;
        }
    }
    let mut list: Vec<(String, usize)> = counts.into_values().collect();
    list.sort_by(|a, b| natural_cmp(&a.0, &b.0));
    list
}

/// Compares ignoring case, with runs of digits compared as numbers (`web-2` before `web-10`).
#[must_use]
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut left = a.chars().peekable();
    let mut right = b.chars().peekable();
    loop {
        match (left.peek().copied(), right.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>| {
                    let mut digits = String::new();
                    while let Some(c) = chars.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(c);
                        chars.next();
                    }
                    digits
                };
                let (x, y) = (take(&mut left), take(&mut right));
                let (x_trim, y_trim) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                let order = x_trim
                    .len()
                    .cmp(&y_trim.len())
                    .then_with(|| x_trim.cmp(y_trim))
                    .then_with(|| x.len().cmp(&y.len()));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                let order = x.to_lowercase().cmp(y.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                left.next();
                right.next();
            }
        }
    }
}

/// A generated list of `count` hosts in nested groups, with tags and favorites: the fixture of
/// the performance checks and the smoke test (always the same for the same count).
#[must_use]
pub fn sample_hosts(count: usize) -> HostsFile {
    let regions = ["eu-west", "eu-central", "us-east", "us-west", "ap-south"];
    let roles = [
        "web", "db", "cache", "queue", "api", "worker", "bastion", "monitor",
    ];
    let mut file = HostsFile::default();
    for (index, region) in regions.iter().enumerate() {
        file.groups.push(Group {
            id: format!("G{index:02}"),
            name: (*region).to_owned(),
            ..Group::default()
        });
        for (child, env) in ["production", "staging"].iter().enumerate() {
            let mut group = Group {
                id: format!("G{index:02}{child}"),
                name: (*env).to_owned(),
                parent: Some(format!("G{index:02}")),
                ..Group::default()
            };
            group.defaults.user = Some(
                if *env == "production" {
                    "deploy"
                } else {
                    "ops"
                }
                .to_owned(),
            );
            file.groups.push(group);
        }
    }
    for n in 0..count {
        let role = roles[n % roles.len()];
        let region = n / roles.len() % regions.len();
        let env = n / (roles.len() * regions.len()) % 2;
        file.hosts.push(Host {
            id: format!("H{n:05}"),
            name: format!("{role}-{:02}.{}", n / 40 + 1, regions[region]),
            group: Some(format!("G{region:02}{env}")),
            address: format!("10.{}.{}.{}", region, n / 250 % 250, n % 250 + 1),
            tags: vec![role.to_owned(), regions[region].to_owned()],
            favorite: n % 37 == 0,
            ..Host::default()
        });
    }
    file
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(file: &HostsFile, found: &[usize]) -> Vec<String> {
        found
            .iter()
            .map(|index| file.hosts[*index].name.clone())
            .collect()
    }

    #[test]
    fn fuzzy_search_ranks_and_filters() {
        let file = sample_hosts(200);
        let recent = RecentList::default();
        let mut searcher = Searcher::new();
        let query = |text: &str| Query {
            text: text.to_owned(),
            ..Query::default()
        };
        let found = searcher.search(&file, &recent, &query("db eu-west"));
        assert!(!found.is_empty());
        assert!(
            names(&file, &found).iter().all(|name| name.contains("db")),
            "{:?}",
            names(&file, &found)
        );
        // An address matches too.
        let found = searcher.search(&file, &recent, &query("10.2.0.101"));
        assert_eq!(file.hosts[found[0]].address, "10.2.0.101");
        // Scope, protocol and tag filters.
        let favorites = searcher.search(
            &file,
            &recent,
            &Query {
                scope: Scope::Favorites,
                ..Query::default()
            },
        );
        assert!(favorites.iter().all(|index| file.hosts[*index].favorite));
        let region = searcher.search(
            &file,
            &recent,
            &Query {
                scope: Scope::Group("G01".into()),
                ..Query::default()
            },
        );
        assert!(!region.is_empty());
        assert!(region.iter().all(|index| {
            file.hosts[*index]
                .group
                .as_deref()
                .is_some_and(|g| g.starts_with("G01"))
        }));
        let tagged = searcher.search(
            &file,
            &recent,
            &Query {
                tag: Some("CACHE".into()),
                ..Query::default()
            },
        );
        assert_eq!(tagged.len(), 25);
        let rdp = searcher.search(
            &file,
            &recent,
            &Query {
                protocol: Some(Protocol::Rdp),
                ..Query::default()
            },
        );
        assert!(rdp.is_empty());
        assert!(
            searcher
                .search(&file, &recent, &query("zzzzqqq"))
                .is_empty()
        );
    }

    #[test]
    fn sorting_and_recent() {
        let file = sample_hosts(40);
        let mut recent = RecentList::default();
        recent.touch_host("H00007", 1);
        recent.touch_host("H00003", 2);
        let mut searcher = Searcher::new();
        let found = searcher.search(
            &file,
            &recent,
            &Query {
                scope: Scope::Recent,
                ..Query::default()
            },
        );
        assert_eq!(found, vec![3, 7]);
        let by_recent = searcher.search(
            &file,
            &recent,
            &Query {
                sort: Sort::LastUsed,
                ..Query::default()
            },
        );
        assert_eq!(&by_recent[..2], &[3, 7]);
        let by_name = searcher.search(&file, &recent, &Query::default());
        let sorted = names(&file, &by_name);
        let mut expected = sorted.clone();
        expected.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(sorted, expected);
        assert_eq!(natural_cmp("web-2", "web-10"), Ordering::Less);
        assert_eq!(natural_cmp("Web", "web"), Ordering::Less);
        assert_eq!(natural_cmp("a01", "a1"), Ordering::Greater);
        let tags = tags(&file);
        assert!(tags.iter().any(|(tag, count)| tag == "web" && *count == 5));
    }

    /// PLAN §9: under 16 ms for 1000 hosts. Timed here in every build; the budget is checked in
    /// release builds (`cargo test --release -p opensesh-core search`, see docs/perf.md).
    #[test]
    fn a_thousand_hosts_search_quickly() {
        let file = sample_hosts(1000);
        let recent = RecentList::default();
        let mut searcher = Searcher::new();
        let mut worst = std::time::Duration::ZERO;
        for text in [
            "w",
            "we",
            "web",
            "web eu",
            "db-1",
            "10.2",
            "stag",
            "monitor us-west",
            "zz",
        ] {
            let start = std::time::Instant::now();
            let found = searcher.search(
                &file,
                &recent,
                &Query {
                    text: text.to_owned(),
                    ..Query::default()
                },
            );
            worst = worst.max(start.elapsed());
            assert!(found.len() <= 1000);
        }
        let start = std::time::Instant::now();
        let all = searcher.search(&file, &recent, &Query::default());
        worst = worst.max(start.elapsed());
        assert_eq!(all.len(), 1000);
        println!("slowest search over 1000 hosts: {worst:?}");
        if !cfg!(debug_assertions) {
            assert!(worst.as_millis() < 16, "{worst:?}");
        }
    }
}
