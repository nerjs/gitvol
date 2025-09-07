use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::domains::repo::RawRepo;

use super::repo::Repo;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no options provided, git URL is required")]
    None,

    #[error("volume name can not be empty")]
    Empty,

    #[error(transparent)]
    Repo(#[from] super::repo::Error),
}

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug, Clone, Serialize)]
pub enum Status {
    Created,
    Clonned,
    Cleared,
}

#[cfg_attr(test, derive(Debug, Clone))]
pub struct Volume {
    pub name: String,
    pub path: Option<PathBuf>,
    pub repo: Repo,
    pub status: Status,
    pub containers: HashSet<String>,
}

impl TryFrom<(&str, RawRepo)> for Volume {
    type Error = Error;

    fn try_from((name, raw): (&str, RawRepo)) -> Result<Self, Self::Error> {
        let name = name.trim();

        if name.is_empty() {
            return Err(Error::Empty);
        }

        let repo = Repo::try_from(raw)?;

        Ok(Self {
            name: name.to_string(),
            repo,
            path: None,
            containers: HashSet::new(),
            status: Status::Created,
        })
    }
}

impl TryFrom<(&str, Option<RawRepo>)> for Volume {
    type Error = Error;

    fn try_from((name, maybe_raw): (&str, Option<RawRepo>)) -> Result<Self, Self::Error> {
        let Some(raw) = maybe_raw else {
            return Err(Error::None);
        };

        Self::try_from((name, raw))
    }
}

impl Volume {
    pub fn create_path_from(&mut self, base_path: &Path) -> PathBuf {
        let mut hasher = DefaultHasher::new();
        hasher.write(self.name.as_bytes());
        hasher.write(b"_");
        self.repo.hash(&mut hasher);
        let hash_part = hasher.finish();
        let path = base_path.join(hash_part.to_string());
        self.path = Some(path.clone());

        path
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::domains::repo::test::REPO_URL;
    use rstest::rstest;

    pub const VOLUME_NAME: &str = "volume_name_99";

    impl Volume {
        pub fn stub() -> Self {
            Self::try_from((VOLUME_NAME, Some(RawRepo::stub()))).unwrap()
        }
    }

    #[test]
    fn missing_options() {
        let error = Volume::try_from((VOLUME_NAME, None)).unwrap_err();
        assert_eq!(error, Error::None);
    }

    #[test]
    fn by_correct_optional() {
        let volume = Volume::try_from((VOLUME_NAME, Some(RawRepo::stub()))).unwrap();
        assert!(volume.repo.url.to_string().contains(REPO_URL));
    }

    #[test]
    fn from_correct_opt() {
        let volume = Volume::try_from((VOLUME_NAME, RawRepo::stub())).unwrap();
        assert_eq!(volume.name, VOLUME_NAME);
        assert!(volume.repo.url.to_string().contains(REPO_URL));
    }

    #[test]
    fn empty_name() {
        let error = Volume::try_from(("  ", RawRepo::stub())).unwrap_err();
        assert_eq!(error, Error::Empty);
    }

    #[rstest]
    #[case(RawRepo::default())]
    #[case(RawRepo::from_url(" "))]
    #[case(RawRepo::from_url("://example.com/repo.git"))]
    #[case(RawRepo { branch: Some("branch".into()),  tag: Some("tag".into()), ..RawRepo::stub() })]
    fn repo_parsing(#[case] raw: RawRepo) {
        let result = Volume::try_from((VOLUME_NAME, raw.clone()));
        assert!(result.is_err(), "Failed check: {:?}", raw);
        let error = result.unwrap_err();
        assert!(
            matches!(error, Error::Repo(_)),
            "Failed check error type: {:?}",
            raw
        );
    }

    #[test]
    fn create_path() {
        let mut volume = Volume::try_from((VOLUME_NAME, RawRepo::stub())).unwrap();

        assert_eq!(volume.path, None);

        let base_path = PathBuf::from("/tmp/test");
        volume.create_path_from(&base_path);

        assert!(matches!(volume.path, Some(p) if p.starts_with(base_path)));
    }

    #[test]
    fn unique_paths() {
        let opts1 = RawRepo::from_url(REPO_URL);
        let opts2 = RawRepo::from_url(REPO_URL);
        let url3 = format!("{}/some-test", REPO_URL);
        let opts3 = RawRepo::from_url(&url3);

        let mut volume1 = Volume::try_from((VOLUME_NAME, opts1)).unwrap();
        let mut volume2 = Volume::try_from((VOLUME_NAME, opts2)).unwrap();
        let mut volume3 = Volume::try_from((VOLUME_NAME, opts3)).unwrap();

        let base_path = PathBuf::from("/tmp/test");
        volume1.create_path_from(&base_path);
        volume2.create_path_from(&base_path);
        volume3.create_path_from(&base_path);

        let path1 = volume1.path.unwrap();
        let path2 = volume2.path.unwrap();
        let path3 = volume3.path.unwrap();

        assert_eq!(path1, path2);
        assert_ne!(path1, path3);
        assert_ne!(path2, path3);
    }
}
