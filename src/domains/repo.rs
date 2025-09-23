use super::url::Url;
use serde::Deserialize;
use std::str::FromStr;
use tracing::debug;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("git URL is missing")]
    MissingUrl,

    #[error("Only one of branch, tag, or ref parameters is allowed")]
    SingleBranch,

    #[error("Parsing URL: {0}")]
    ParsingUrl(#[from] super::url::Error),
}

#[cfg_attr(test, derive(Debug, PartialEq))]
#[derive(Hash, Clone)]
pub struct Repo {
    pub url: Url,
    pub branch: Option<String>,
    pub refetch: bool,
}

#[cfg_attr(test, derive(Default, Clone))]
#[derive(Debug, Deserialize)]
pub struct RawRepo {
    pub url: Option<String>,
    pub branch: Option<String>,
    pub tag: Option<String>,
    pub refetch: Option<String>,
}

impl TryFrom<RawRepo> for Repo {
    type Error = Error;

    fn try_from(value: RawRepo) -> Result<Self, Self::Error> {
        let Some(url) = value.url else {
            return Err(Error::MissingUrl);
        };

        let url = Url::from_str(&url)?;

        if value.branch.is_some() && value.tag.is_some() {
            return Err(Error::SingleBranch);
        }

        let branch = value.branch.or(value.tag);
        let refetch = value.refetch.unwrap_or("false".to_string()) == "true";

        debug!(
            url = url.to_string(),
            branch, refetch, "Parsed repository options"
        );

        Ok(Self {
            url,
            branch,
            refetch,
        })
    }
}

#[cfg(test)]
pub mod test {
    use std::hash::{DefaultHasher, Hash, Hasher};

    use super::*;
    use rstest::rstest;

    pub const REPO_URL: &str = "https://example.com/repo.git";

    impl RawRepo {
        pub fn stub() -> Self {
            Self::from_url(REPO_URL)
        }

        pub fn from_url(url: &str) -> Self {
            Self {
                url: Some(url.to_string()),
                ..Default::default()
            }
        }
    }

    #[rstest]
    #[case(RawRepo::default())]
    #[case(RawRepo { branch: Some("test".into()), ..Default::default() })]
    #[case(RawRepo { tag: Some("test".into()), ..Default::default() })]
    #[case(RawRepo { refetch: Some("true".into()), ..Default::default() })]
    fn raw_without_url(#[case] raw: RawRepo) {
        let error = Repo::try_from(raw).unwrap_err();
        assert_eq!(error, Error::MissingUrl);
    }

    #[test]
    fn branch_and_tag_together() {
        let raw = RawRepo {
            url: Some("http://host/path-to-git-repo".into()),
            branch: Some("branch".into()),
            tag: Some("tag".into()),
            ..Default::default()
        };

        let error = Repo::try_from(raw).unwrap_err();
        assert_eq!(error, Error::SingleBranch);
    }

    #[rstest]
    #[case("://host/path-to-git-repo")]
    #[case("ssh://host:123/~user/path-to-git-repo")]
    #[case("ssh://user@host:123/~user/path-to-git-repo")]
    #[case("ftp://host/path-to-git-repo")]
    #[case("ftps://host/path-to-git-repo")]
    fn failed_url_parsing(#[case] url: &str) {
        let raw = RawRepo {
            url: Some(url.to_string()),
            ..Default::default()
        };

        let error = Repo::try_from(raw).unwrap_err();
        assert!(matches!(error, Error::ParsingUrl(_)));
    }

    #[test]
    fn use_branch() {
        let raw = RawRepo {
            url: Some("http://host/path-to-git-repo".into()),
            branch: Some("branch".into()),
            ..Default::default()
        };

        let repo = Repo::try_from(raw).unwrap();
        assert_eq!(repo.branch, Some("branch".into()));
    }

    #[test]
    fn use_tag() {
        let raw = RawRepo {
            url: Some("http://host/path-to-git-repo".into()),
            tag: Some("tag".into()),
            ..Default::default()
        };

        let repo = Repo::try_from(raw).unwrap();
        assert_eq!(repo.branch, Some("tag".into()));
    }

    #[rstest]
    #[case(None, false)]
    #[case(Some("false".to_string()), false)]
    #[case(Some("".to_string()), false)]
    #[case(Some("Tratata".to_string()), false)]
    #[case(Some("true".to_string()), true)]
    fn check_refetch(#[case] refetch: Option<String>, #[case] expect: bool) {
        let raw = RawRepo {
            url: Some("http://host/path-to-git-repo".into()),
            refetch,
            ..Default::default()
        };

        let repo = Repo::try_from(raw).unwrap();
        assert_eq!(repo.refetch, expect);
    }

    #[test]
    fn hash_consistency() {
        let raw1 = RawRepo {
            url: Some("http://host/path-to-git-repo".into()),
            ..Default::default()
        };
        let raw2 = RawRepo {
            url: Some("http://host/path-to-git-repo".into()),
            ..Default::default()
        };

        let repo1 = Repo::try_from(raw1).unwrap();
        let repo2 = Repo::try_from(raw2).unwrap();

        let mut hasher1 = DefaultHasher::new();
        repo1.hash(&mut hasher1);
        let hash1 = hasher1.finish();

        let mut hasher2 = DefaultHasher::new();
        repo2.hash(&mut hasher2);
        let hash2 = hasher2.finish();

        assert_eq!(hash1, hash2);
    }
}
