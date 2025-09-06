use std::{fmt::Display, str::FromStr};

use git_url_parse::{GitUrl, GitUrlParseError, Scheme};

const SUPPORTED_SCHEMES: &[Scheme] = &[
    Scheme::Http,
    Scheme::Https,
    #[cfg(test)]
    Scheme::File,
];

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("git url can not be empty")]
    Empty,

    #[error(transparent)]
    Parse(#[from] GitUrlParseError),

    #[error("Unsupported scheme {0}. Allowed only {allowed:?}", allowed = SUPPORTED_SCHEMES)]
    Unsupported(Scheme),
}

#[cfg_attr(test, derive(Debug, PartialEq))]
#[derive(Clone)]
pub struct Url(GitUrl);

impl FromStr for Url {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let str_url = s.trim();
        if str_url.is_empty() {
            return Err(Error::Empty);
        }

        let git_url = GitUrl::from_str(str_url)?;

        if !SUPPORTED_SCHEMES.contains(&git_url.scheme) {
            return Err(Error::Unsupported(git_url.scheme));
        }

        Ok(Self(git_url))
    }
}

impl Display for Url {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::hash::Hash for Url {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write(self.to_string().as_bytes());
    }
}

#[cfg(test)]
mod test {
    use rstest::rstest;

    use super::*;

    #[test]
    fn empty_url() {
        let err = Url::from_str("  ").unwrap_err();
        assert_eq!(err, Error::Empty);
    }

    #[test]
    fn parsing() {
        let err = Url::from_str("://host:123/path-to-git-repo").unwrap_err();
        assert!(matches!(err, Error::Parse(_)));
    }

    #[rstest]
    #[case("http://host/path-to-git-repo")]
    #[case("https://host/path-to-git-repo")]
    #[case("http://host:123/path-to-git-repo")]
    #[case("https://host:123/path-to-git-repo")]
    fn valid_url(#[case] s: &str) {
        let result = Url::from_str(s);
        assert!(result.is_ok(), "Failed parsing {s}");

        let url = result.unwrap();
        assert!(url.to_string().contains(s));
    }

    #[rstest]
    #[case("host:~user/path-to-git-repo")]
    #[case("user@host:~user/path-to-git-repo")]
    #[case("ssh://host/path-to-git-repo")]
    #[case("ssh://user@host/path-to-git-repo")]
    #[case("ssh://host:123/path-to-git-repo")]
    #[case("ssh://user@host:123/path-to-git-repo")]
    #[case("git://host/path-to-git-repo")]
    #[case("git://host:123/path-to-git-repo")]
    #[case("git://host/~user/path-to-git-repo")]
    #[case("git://host:123/~user/path-to-git-repo")]
    #[case("ssh://host/~user/path-to-git-repo")]
    #[case("ssh://user@host/~user/path-to-git-repo")]
    #[case("ssh://host:123/~user/path-to-git-repo")]
    #[case("ssh://user@host:123/~user/path-to-git-repo")]
    #[case("ftp://host/path-to-git-repo")]
    #[case("ftps://host/path-to-git-repo")]
    #[case("ftp://host:123/path-to-git-repo")]
    #[case("ftps://host:123/path-to-git-repo")]
    fn invalid_url(#[case] s: &str) {
        let result = Url::from_str(s);
        assert!(result.is_err(), "Successed parsing '{s}'");

        let error = result.unwrap_err();
        assert!(matches!(error, Error::Unsupported(_)));
    }
}
