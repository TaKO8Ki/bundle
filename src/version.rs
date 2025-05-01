use pubgrub::Ranges;
use semver::Version as SemVersion;
use tracing::debug;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum Segment {
    Numeric(u64),
    Text(String),
    Prerelease(String),
}

impl PartialOrd for Segment {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use Segment::*;
        Some(match (self, other) {
            (Numeric(a), Numeric(b)) => a.cmp(b),
            (Numeric(_), Text(_)) => std::cmp::Ordering::Greater,
            (Numeric(_), Prerelease(_)) => std::cmp::Ordering::Greater,
            (Text(_), Numeric(_)) => std::cmp::Ordering::Less,
            (Text(a), Text(b)) => a.cmp(b),
            (Text(a), Prerelease(b)) => a.cmp(b),
            (Prerelease(_), Numeric(_)) => std::cmp::Ordering::Less,
            (Prerelease(a), Text(b)) => a.cmp(b),
            (Prerelease(a), Prerelease(b)) => a.cmp(b),
        })
    }
}

impl Ord for Segment {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl PartialOrd for RubyVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering;
        let max_len = usize::max(self.segments.len(), other.segments.len());
        for i in 0..max_len {
            let a = self.segments.get(i).unwrap_or(&Segment::Numeric(0));
            let b = other.segments.get(i).unwrap_or(&Segment::Numeric(0));
            let ord = a.cmp(b);
            if ord != Ordering::Equal {
                return Some(ord);
            }
        }
        Some(std::cmp::Ordering::Equal)
    }
}
impl Ord for RubyVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct RubyVersion {
    pub segments: Vec<Segment>,
    platform_segment: Option<Segment>,
}

impl std::fmt::Display for RubyVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut text = String::new();
        for (i, seg) in self.segments.iter().enumerate() {
            match seg {
                Segment::Numeric(n) => {
                    if i > 0 {
                        text.push('.');
                    }
                    text.push_str(&n.to_string())
                }
                Segment::Text(s) => {
                    if i > 0 {
                        text.push('.');
                    }
                    text.push_str(s)
                }
                Segment::Prerelease(_) => (),
            }
        }
        if let Some(Segment::Prerelease(platform)) = &self.platform_segment {
            text.push_str("-");
            text.push_str(&platform)
        }
        write!(f, "{}", text)
    }
}

impl RubyVersion {
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        RubyVersion {
            segments: vec![
                Segment::Numeric(major),
                Segment::Numeric(minor),
                Segment::Numeric(patch),
            ],
            platform_segment: None,
        }
    }

    fn base_version(&self) -> String {
        self.to_string().splitn(2, '-').next().unwrap().to_string()
    }

    fn has_suffix(&self) -> bool {
        self.to_string().contains('-')
    }

    pub fn bump(&self) -> Self {
        let raw = self.to_string();
        let mut segments: Vec<String> = raw.split('.').map(|s| s.to_string()).collect();

        // Step 1-2: remove trailing non-numeric segments (prerelease identifiers)
        while segments
            .last()
            .map(|s| !s.chars().all(|c| c.is_ascii_digit()))
            .unwrap_or(false)
        {
            segments.pop();
        }

        // Step 3: drop one more segment if we still have ≥2 (matching Ruby behaviour)
        if segments.len() > 1 {
            segments.pop();
        }

        // Step 4: increment last numeric segment, or default to 1
        if let Some(last) = segments.pop() {
            let next_num = last.parse::<u64>().unwrap_or(0) + 1;
            segments.push(next_num.to_string());
        } else {
            segments.push("1".to_string());
        }

        // Step 5: join back & parse
        let bumped = segments.join(".");
        RubyVersion::parse(&bumped)
    }

    pub fn parse(text: &str) -> Self {
        let text = text.split('+').next().unwrap();
        let mut main_and_pre = text.splitn(2, '-');
        let main = main_and_pre.next().unwrap();
        let pre = main_and_pre.next();
        let mut segments = Vec::new();
        for part in main.split('.') {
            let mut digits = String::new();
            let mut letters = String::new();
            for c in part.chars() {
                if c.is_ascii_digit() && letters.is_empty() {
                    digits.push(c);
                } else {
                    letters.push(c);
                }
            }
            if !digits.is_empty() {
                let n = digits.parse().unwrap_or(0);
                segments.push(Segment::Numeric(n));
            }
            if !letters.is_empty() {
                segments.push(Segment::Text(letters));
            }
        }

        RubyVersion {
            segments,
            platform_segment: pre.map(|pre| Segment::Prerelease(pre.to_string())),
        }
    }
}

fn parse_semver(text: &str) -> anyhow::Result<SemVersion> {
    let parts: Vec<&str> = text.split('.').collect();
    let (major, minor, patch_str) = match parts.as_slice() {
        [maj] => (maj.to_string(), "0".to_string(), "0".to_string()),
        [maj, min] => (maj.to_string(), min.to_string(), "0".to_string()),
        [maj, min, pat] => (maj.to_string(), min.to_string(), pat.to_string()),
        _ => return Err(anyhow::anyhow!("Invalid semver string: {}", text)),
    };
    SemVersion::parse(&format!("{}.{}.{}", major, minor, patch_str))
        .map_err(|e| anyhow::anyhow!("Failed to parse semver string: {}. Error: {}", text, e))
}

pub fn parse_req(text: &str, separator: &str) -> Ranges<RubyVersion> {
    let mut set = Ranges::full();

    if text.trim() == "*" {
        return Ranges::full();
    }
    debug!("Parsing version requirement: {}", text);
    for part in text.split(separator) {
        let s = part.trim();
        if s == "*" {
            continue;
        }

        let (op, ver_str) = if s.starts_with("~>") {
            ("~>", s.trim_start_matches("~>").trim())
        } else if s.starts_with('^') {
            ("^", s.trim_start_matches('^').trim())
        } else if s.starts_with(">=") {
            (">=", s.trim_start_matches(">=").trim())
        } else if s.starts_with("<=") {
            ("<=", s.trim_start_matches("<=").trim())
        } else if s.starts_with('>') {
            (">", s.trim_start_matches('>').trim())
        } else if s.starts_with('<') {
            ("<", s.trim_start_matches('<').trim())
        } else if s.starts_with("!=") {
            ("!=", s.trim_start_matches("!=").trim())
        } else {
            ("=", s.trim_start_matches('=').trim())
        };
        let rv = RubyVersion::parse(ver_str);

        let rng = match op {
            "=" => Ranges::singleton(rv.clone()),
            ">" => Ranges::strictly_higher_than(rv.clone()),
            ">=" => Ranges::higher_than(rv.clone()),
            "<" => Ranges::strictly_lower_than(rv.clone()),
            "<=" => Ranges::lower_than(rv.clone()),
            "!=" => {
                let lower = Ranges::strictly_lower_than(rv.clone());
                let upper = Ranges::strictly_higher_than(rv.clone());
                lower.union(&upper)
            }
            "~>" => {
                // pessimistic operator: >= rv, < next breaking version
                let mut next = rv.clone();
                if next.segments.len() > 2 {
                    next = next.bump();
                } else {
                    // single-segment case: bump major
                    // if next.segments.len() == 0 {
                    //     println!("Parsed version: {}", text);
                    // }

                    if let Segment::Numeric(maj) = &mut next.segments[0] {
                        *maj += 1;
                    }
                    // keep only the major segment
                    next.segments.truncate(1);
                }
                Ranges::between(rv.clone(), next)
            }
            "^" => {
                // caret semver: ^x.y.z => < next breaking change
                let mut next = rv.clone();
                match next.segments.get_mut(0) {
                    Some(Segment::Numeric(maj)) if *maj > 0 => *maj += 1,
                    _ => {
                        // major=0: bump minor
                        if next.segments.len() > 1 {
                            if let Segment::Numeric(min) = &mut next.segments[1] {
                                *min += 1;
                            }
                        }
                    }
                }
                Ranges::intersection(
                    &Ranges::higher_than(rv.clone()),
                    &Ranges::strictly_lower_than(next),
                )
            }
            _ => Ranges::full(),
        };
        debug!("Parsed range: {:?}", rng);
        set = set.intersection(&rng);
    }
    set
}

#[cfg(test)]
mod tests {
    use crate::version::{RubyVersion, Segment, parse_req};
    use pubgrub::Ranges;

    #[test]
    fn test_ruby_parse() {
        let rv = RubyVersion::parse("1.7.0");
        assert_eq!(rv.segments.len(), 3);
        assert_eq!(rv.segments[0], Segment::Numeric(1));
        assert_eq!(rv.segments[1], Segment::Numeric(7));
        assert_eq!(rv.segments[2], Segment::Numeric(0));
        assert_eq!(rv.to_string(), "1.7.0");

        let rv = RubyVersion::parse("3.3.7.3");
        assert_eq!(rv.segments.len(), 4);
        assert_eq!(rv.segments[0], Segment::Numeric(3));
        assert_eq!(rv.segments[1], Segment::Numeric(3));
        assert_eq!(rv.segments[2], Segment::Numeric(7));
        assert_eq!(rv.segments[3], Segment::Numeric(3));
        assert_eq!(rv.to_string(), "3.3.7.3");

        let rv = RubyVersion::parse("1.18.7-aarch64-linux-gnu");
        assert_eq!(rv.segments.len(), 3);
        assert_eq!(rv.segments[0], Segment::Numeric(1));
        assert_eq!(rv.segments[1], Segment::Numeric(18));
        assert_eq!(rv.segments[2], Segment::Numeric(7));
        assert_eq!(
            rv.platform_segment,
            Some(Segment::Prerelease("aarch64-linux-gnu".to_string()))
        );
        assert_eq!(rv.to_string(), "1.18.7-aarch64-linux-gnu");

        let rv = RubyVersion::parse("2.15.0.rc1-x86-linux-gnu");
        assert_eq!(rv.segments.len(), 4);
        assert_eq!(rv.segments[0], Segment::Numeric(2));
        assert_eq!(rv.segments[1], Segment::Numeric(15));
        assert_eq!(rv.segments[2], Segment::Numeric(0));
        assert_eq!(rv.segments[3], Segment::Text("rc1".to_string()));
        assert_eq!(
            rv.platform_segment,
            Some(Segment::Prerelease("x86-linux-gnu".to_string()))
        );
        assert_eq!(rv.to_string(), "2.15.0.rc1-x86-linux-gnu")
    }

    fn rv(v: &str) -> RubyVersion {
        RubyVersion::parse(v)
    }

    #[test]
    fn gt_operator() {
        let r: Ranges<RubyVersion> = parse_req(">3.0", ",");
        assert!(!r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(3),
                Segment::Numeric(0),
                Segment::Numeric(0)
            ],
            platform_segment: None
        }));
        assert!(r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(3),
                Segment::Numeric(0),
                Segment::Numeric(1)
            ],
            platform_segment: None
        }));
        assert!(!r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(2),
                Segment::Numeric(9),
                Segment::Numeric(9)
            ],
            platform_segment: None
        }));
    }

    #[test]
    fn ge_operator() {
        let r: Ranges<RubyVersion> = parse_req(">=1.2.3", ",");
        assert!(r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(1),
                Segment::Numeric(2),
                Segment::Numeric(3)
            ],
            platform_segment: None
        }));
        assert!(r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(1),
                Segment::Numeric(2),
                Segment::Numeric(4)
            ],
            platform_segment: None
        }));
        assert!(!r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(1),
                Segment::Numeric(2),
                Segment::Numeric(2)
            ],
            platform_segment: None
        }));
    }

    #[test]
    fn lt_le_operators() {
        let lt: Ranges<RubyVersion> = parse_req("<2.0", ",");
        assert!(!lt.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(2),
                Segment::Numeric(0),
                Segment::Numeric(0)
            ],
            platform_segment: None
        }));
        assert!(lt.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(1),
                Segment::Numeric(9),
                Segment::Numeric(9)
            ],
            platform_segment: None
        }));

        let le: Ranges<RubyVersion> = parse_req("<=2.0", ",");
        assert!(le.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(2),
                Segment::Numeric(0),
                Segment::Numeric(0)
            ],
            platform_segment: None
        }));
        assert!(!le.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(2),
                Segment::Numeric(0),
                Segment::Numeric(1)
            ],
            platform_segment: None
        }));
    }

    #[test]
    fn eq_operator() {
        let r: Ranges<RubyVersion> = parse_req("=1.4.5", ",");
        assert!(r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(1),
                Segment::Numeric(4),
                Segment::Numeric(5)
            ],
            platform_segment: None
        }));
        assert!(!r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(1),
                Segment::Numeric(4),
                Segment::Numeric(6)
            ],
            platform_segment: None
        }));
    }

    #[test]
    fn wildcard() {
        let r: Ranges<RubyVersion> = parse_req("*", ",");
        assert!(r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(0),
                Segment::Numeric(0),
                Segment::Numeric(0)
            ],
            platform_segment: None
        }));
        assert!(r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(999),
                Segment::Numeric(9),
                Segment::Numeric(9)
            ],
            platform_segment: None
        }));
    }

    #[test]
    fn pessimistic_operator() {
        let r: Ranges<RubyVersion> = parse_req("~>1.5", ",");
        assert!(r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(1),
                Segment::Numeric(5),
                Segment::Numeric(0)
            ],
            platform_segment: None
        }));
        assert!(r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(1),
                Segment::Numeric(9),
                Segment::Numeric(9)
            ],
            platform_segment: None
        }));
        assert!(!r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(2),
                Segment::Numeric(0),
                Segment::Numeric(0)
            ],
            platform_segment: None
        }));
    }

    // #[test]
    // fn pessimistic_operator_invalid_semver() {
    //     let r: Ranges<RubyVersion> = parse_req("~>0.0.6.beta.2", ",");
    //     assert!(r.contains(&RubyVersion {
    //         segments: vec![
    //             Segment::Numeric(0),
    //             Segment::Numeric(0),
    //             Segment::Numeric(6),
    //             Segment::Text("beta".to_string()),
    //             Segment::Numeric(2)
    //         ],
    //         platform_segment: None
    //     }));
    //     assert!(!r.contains(&RubyVersion {
    //         segments: vec![
    //             Segment::Numeric(0),
    //             Segment::Numeric(0),
    //             Segment::Numeric(7)
    //         ],
    //         platform_segment: None
    //     }));
    // }

    #[test]
    fn not_equal_operator() {
        let r: Ranges<RubyVersion> = parse_req("!=2.1.3", ",");
        assert!(!r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(2),
                Segment::Numeric(1),
                Segment::Numeric(3)
            ],
            platform_segment: None
        }));
        assert!(r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(2),
                Segment::Numeric(5),
                Segment::Numeric(0)
            ],
            platform_segment: None
        }));
        assert!(r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(3),
                Segment::Numeric(0),
                Segment::Numeric(0)
            ],
            platform_segment: None
        }));
    }

    #[test]
    fn multiple_version_req() {
        let r: Ranges<RubyVersion> = parse_req(">2.0&<=3.0", "&");
        assert!(r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(2),
                Segment::Numeric(1),
                Segment::Numeric(3)
            ],
            platform_segment: None
        }));
        assert!(r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(2),
                Segment::Numeric(5),
                Segment::Numeric(0)
            ],
            platform_segment: None
        }));
        assert!(r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(3),
                Segment::Numeric(0),
                Segment::Numeric(0)
            ],
            platform_segment: None
        }));
        assert!(!r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(3),
                Segment::Numeric(0),
                Segment::Numeric(1)
            ],
            platform_segment: None
        }));
    }

    #[test]
    fn multiple_version_req_with_comma() {
        let r: Ranges<RubyVersion> = parse_req(">=2.0,<3.0", ",");
        assert!(r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(2),
                Segment::Numeric(0),
                Segment::Numeric(0)
            ],
            platform_segment: None
        }));
        assert!(r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(2),
                Segment::Numeric(1),
                Segment::Numeric(3)
            ],
            platform_segment: None
        }));
        assert!(r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(2),
                Segment::Numeric(5),
                Segment::Numeric(0)
            ],
            platform_segment: None
        }));
        assert!(!r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(3),
                Segment::Numeric(0),
                Segment::Numeric(0)
            ],
            platform_segment: None
        }));
        assert!(!r.contains(&RubyVersion {
            segments: vec![
                Segment::Numeric(3),
                Segment::Numeric(0),
                Segment::Numeric(1)
            ],
            platform_segment: None
        }));
    }

    #[test]
    fn test_bump() {
        let rv = RubyVersion::parse("1.2.3");
        let bumped = rv.bump();
        assert_eq!(bumped.to_string(), "1.3");

        let rv = RubyVersion::parse("0.9.11");
        let bumped = rv.bump();
        assert_eq!(bumped.to_string(), "0.10");

        let rv = RubyVersion::parse("3.0.0.rc12");
        let bumped = rv.bump();
        assert_eq!(bumped.to_string(), "3.1");
    }
}
