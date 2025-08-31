use super::handlers::*;
use super::shared::*;
use crate::result::Result;
use crate::state::{
    GitvolState, Repo, RepoStatus,
    test::{REPO_URL, VOLUME_NAME},
};
use axum::{Json, extract::State};
use std::path::PathBuf;
use tempfile::{Builder as TempBuilder, TempDir};
use uuid::Uuid;

impl GitvolState {
    pub async fn set_path(&self, name: &str, path: impl Into<PathBuf>) -> Result<()> {
        let mut volume = self.try_write(name).await?;
        volume.path = Some(path.into());
        Ok(())
    }

    async fn stub_with_path(path: impl Into<PathBuf>) -> Self {
        let state = Self::stub_with_create().await;
        let mut volume = state.try_write(VOLUME_NAME).await.unwrap();
        volume.path = Some(path.into());

        state
    }

    fn temp() -> (Self, TempDir) {
        let temp = TempBuilder::new().prefix("temp-gitvol-").tempdir().unwrap();
        (Self::new(temp.path().to_path_buf()), temp)
    }

    async fn temp_with_volume() -> (Self, TempDir) {
        let (state, temp_guard) = Self::temp();
        _ = state.create(VOLUME_NAME, Repo::stub()).await.unwrap();

        (state, temp_guard)
    }

    fn req(&self) -> State<Self> {
        State(self.clone())
    }
}

impl Named {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }

    fn stub() -> Self {
        Self::new(VOLUME_NAME)
    }

    fn req(&self) -> Json<Self> {
        Json(self.clone())
    }
}

impl NamedWID {
    fn new(name: &str) -> Self {
        let id = Uuid::new_v4();
        Self {
            name: name.to_string(),
            id: id.to_string(),
        }
    }

    fn stub() -> Self {
        Self::new(VOLUME_NAME)
    }

    fn to_req(self) -> Json<Self> {
        Json(self)
    }

    fn req() -> Json<Self> {
        Json(Self::stub())
    }
}

impl RawCreateRequest {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..Default::default()
        }
    }

    fn stub() -> Self {
        Self::new(VOLUME_NAME)
    }

    fn stub_with_url() -> Self {
        Self::stub().with_url(REPO_URL)
    }

    fn req() -> Json<Self> {
        Json(Self::stub())
    }

    fn to_req(self) -> Json<Self> {
        Json(self)
    }

    fn with_opts(mut self, opts: RawRepo) -> Self {
        self.opts = Some(opts);
        self
    }

    fn with_url(self, url: &str) -> Self {
        let mut opts = self.clone().opts.unwrap_or_default();
        opts.url = Some(url.to_string());
        self.with_opts(opts)
    }

    fn with_tag(self, tag: &str) -> Self {
        let mut opts = self.clone().opts.unwrap_or_default();
        opts.tag = Some(tag.to_string());
        self.with_opts(opts)
    }

    fn with_branch(self, branch: &str) -> Self {
        let mut opts = self.clone().opts.unwrap_or_default();
        opts.branch = Some(branch.to_string());
        self.with_opts(opts)
    }

    fn with_refetch(self, refetch: bool) -> Self {
        let mut opts = self.clone().opts.unwrap_or_default();
        opts.refetch = Some(refetch);
        self.with_opts(opts)
    }
}

mod oneline {
    use super::*;

    #[tokio::test]
    async fn activate_plugin_success() {
        let result = activate_plugin().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn capabilities_handler_success() {
        let result = capabilities_handler().await;
        assert!(result.is_ok())
    }
}

mod by_state_reactions {
    use tokio::fs;

    use crate::git::test::{TestRepo, is_git_dir};

    use super::*;

    #[tokio::test]
    async fn get_path_if_non_existent_volume() {
        let state = GitvolState::stub();
        let result = get_volume_path(State(state), Named::stub().req()).await;

        assert!(result.is_ok());
        let mount_point = result.unwrap();
        assert_eq!(mount_point.mountpoint, None);
    }

    #[tokio::test]
    async fn successfully_get_volume_path() {
        let path = PathBuf::from("/tmp/test_path");
        let state = GitvolState::stub_with_path(&path).await;

        let result = get_volume_path(State(state), Named::stub().req()).await;

        assert!(result.is_ok());
        let mount_point = result.unwrap();
        assert_eq!(mount_point.mountpoint, Some(path));
    }

    #[tokio::test]
    async fn get_volume_if_non_existent_volume() {
        let state = GitvolState::stub();
        let result = get_volume(State(state), Named::stub().req()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn successfully_get_volume() {
        let path = PathBuf::from("/tmp/test_volume");
        let state = GitvolState::stub_with_path(&path).await;

        let result = get_volume(State(state), Named::stub().req()).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(
            response.volume,
            GetMp {
                name: VOLUME_NAME.to_string(),
                mountpoint: Some(path),
                status: MpStatus {
                    status: RepoStatus::Created
                },
            }
        );
    }

    #[tokio::test]
    async fn empty_list_volumes() {
        let state = GitvolState::stub();
        let result = list_volumes(State(state)).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.volumes.is_empty());
    }

    #[tokio::test]
    async fn non_empty_list_volumes() {
        let second_volume_name = format!("{}_2", VOLUME_NAME);
        let path = PathBuf::from("/tmp/test_volume");
        let state = GitvolState::stub_with_path(path).await;
        _ = state
            .create(&second_volume_name, Repo::stub())
            .await
            .unwrap();

        let result = list_volumes(State(state)).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.volumes.len(), 2);

        assert!(response.volumes.contains(&ListMp {
            name: VOLUME_NAME.to_string(),
            mountpoint: Some(PathBuf::from("/tmp/test_volume")),
        }));
        assert!(response.volumes.contains(&ListMp {
            name: second_volume_name,
            mountpoint: None,
        }));
    }

    #[tokio::test]
    async fn create_volume_missing_opt() {
        let state = GitvolState::stub();
        let result = create_volume(State(state.clone()), RawCreateRequest::req()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_volume_missing_url() {
        let state = GitvolState::stub();
        let request = RawCreateRequest::stub()
            .with_opts(RawRepo::default())
            .to_req();

        let result = create_volume(State(state.clone()), request).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_handler_state_incorrect_name_error() {
        let state = GitvolState::stub();
        // empty trimmed name
        let request = RawCreateRequest::new("  ").with_url(REPO_URL).to_req();
        let result = create_volume(State(state.clone()), request).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_volume_multiple_refs() {
        let state = GitvolState::stub();
        let request = RawCreateRequest::new(VOLUME_NAME)
            .with_url(REPO_URL)
            .with_branch("branch")
            .with_tag("tag")
            .to_req();

        let result = create_volume(State(state.clone()), request).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_volume_state_duplicate_error() {
        let state = GitvolState::stub_with_create().await;
        let result = create_volume(
            State(state.clone()),
            RawCreateRequest::stub_with_url().to_req(),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn successfully_create_volume() {
        let state = GitvolState::stub();
        let result = create_volume(
            State(state.clone()),
            RawCreateRequest::stub_with_url().to_req(),
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Empty {});

        let volume = state.read(VOLUME_NAME).await.unwrap();

        assert_eq!(volume.name, VOLUME_NAME);
        assert_eq!(volume.repo, Repo::stub());
    }

    #[tokio::test]
    async fn successfully_remove_volume() {
        let (state, temp) = GitvolState::temp();
        let path = temp.path().to_path_buf();
        state.create(VOLUME_NAME, Repo::stub()).await.unwrap();

        fs::create_dir_all(&path).await.unwrap();
        fs::write(path.join("some.file"), "contents").await.unwrap();
        state.set_path(VOLUME_NAME, &path).await.unwrap();

        assert!(path.exists());
        let result = remove_volume(State(state.clone()), Named::stub().req()).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Empty {});

        let volume = state.read(VOLUME_NAME).await;
        assert!(volume.is_none());
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn failed_mount_by_non_existent_volume() {
        let state = GitvolState::stub();
        let result = mount_volume_to_container(State(state), NamedWID::req()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn successfully_mount_volume() {
        let test_repo = TestRepo::get_or_create().await.unwrap();
        let (state, _temp) = GitvolState::temp();

        _ = state
            .create(VOLUME_NAME, Repo::url(&test_repo.file))
            .await
            .unwrap();
        let request = NamedWID::stub();

        let result = mount_volume_to_container(State(state.clone()), Json(request.clone())).await;
        assert!(result.is_ok());

        let volume = state.read(VOLUME_NAME).await.unwrap().clone();
        let path = volume.path.unwrap();

        assert!(path.exists());
        assert!(!is_git_dir(&path));
        assert!(volume.containers.contains(&request.id));
    }

    #[tokio::test]
    async fn failed_unmount_by_non_existent_volume() {
        let state = GitvolState::stub();
        let result = unmount_volume_by_container(State(state), NamedWID::req()).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn successfully_unmount_volume() {
        let (state, temp) = GitvolState::temp_with_volume().await;
        let request = NamedWID::stub();
        let path = temp.path().join("volume_path");
        let mut volume = state.try_write(VOLUME_NAME).await.unwrap();
        volume.containers.insert(request.id.clone());
        volume.path = Some(path.clone());
        drop(volume);
        fs::create_dir_all(&path).await.unwrap();

        let result = unmount_volume_by_container(State(state.clone()), Json(request.clone())).await;

        assert!(result.is_ok());
        assert!(!path.exists());

        let volume = state.read(VOLUME_NAME).await.unwrap().clone();
        assert!(!volume.containers.contains(&request.id));
    }
}

mod usecase {
    use crate::git::test::{TestRepo, is_git_dir};

    use super::*;

    #[derive(Clone)]
    struct CheckVol {
        name: String,
        status: RepoStatus,
        mountpoint: Option<PathBuf>,
    }

    impl CheckVol {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                status: RepoStatus::Created,
                mountpoint: None,
            }
        }

        fn with_status(mut self, status: RepoStatus) -> Self {
            self.status = status;
            self
        }

        fn with_mp(mut self, mp: Option<PathBuf>) -> Self {
            self.mountpoint = mp;
            self
        }

        fn stub() -> Self {
            Self::new(VOLUME_NAME)
        }
    }

    async fn assert_list(state: &GitvolState, len: usize, includes: Vec<CheckVol>) {
        let ListResponse { volumes } = list_volumes(state.req()).await.unwrap();

        let msg = if len == 0 {
            format!("The list of volumes is not empty")
        } else {
            format!(
                "The length of the list of volumes is not equal to {}. Actual - {}",
                len,
                volumes.len()
            )
        };
        assert_eq!(len, volumes.len(), "{}", msg);

        for CheckVol {
            name,
            mountpoint,
            status,
        } in includes
        {
            let name: String = name.to_string();
            let list_item = volumes.iter().find(|item| item.name == name);
            assert!(
                list_item.is_some(),
                "The volume named '{}' was not found in the list.",
                name
            );
            let request = Named::new(&name).req();
            let get_response = get_volume(state.req(), request.clone()).await;

            assert!(
                get_response.is_ok(),
                "The volume named {} is missing upon getting. Error: {:?}",
                name,
                get_response.unwrap_err()
            );
            let volume_from_get = get_response.unwrap().volume;

            assert_eq!(
                MpStatus {
                    status: status.clone()
                },
                volume_from_get.status,
                "The volume status does not match the expected status. Current status: {:?}. Expected status: {:?}.",
                volume_from_get.status,
                status
            );

            let mountpoint_from_list = list_item.unwrap().mountpoint.clone();
            let mountpoint_from_get = volume_from_get.mountpoint;
            assert_eq!(
                mountpoint_from_list, mountpoint_from_get,
                "Mountpoint from list is not equal to mountpoint from get"
            );

            let mountpoint_from_path = get_volume_path(state.req(), request)
                .await
                .unwrap()
                .mountpoint;
            assert_eq!(
                mountpoint_from_get, mountpoint_from_path,
                "Mountpoint from path is not equal to mountpoint from get/list"
            );

            assert_eq!(
                mountpoint, mountpoint_from_path,
                "Mountpoint from mount is not equal to mountpoint from get/list/path"
            );
        }
    }

    async fn assert_empty_list(state: &GitvolState) {
        assert_list(state, 0, Vec::new()).await;
    }

    async fn assert_created(state: &GitvolState, list: Vec<&str>) {
        let includes = list
            .into_iter()
            .map(|name| CheckVol::new(name).with_status(RepoStatus::Created))
            .collect::<Vec<CheckVol>>();
        assert_list(state, includes.len(), includes).await;
    }

    async fn assert_created_stub(state: &GitvolState) {
        assert_created(state, vec![VOLUME_NAME]).await
    }

    #[tokio::test]
    async fn emply_before_working() {
        assert_empty_list(&GitvolState::stub()).await;
    }

    #[tokio::test]
    async fn onetime_creating_volume() {
        let state = GitvolState::stub();
        assert_empty_list(&state).await;

        _ = create_volume(state.req(), RawCreateRequest::stub_with_url().to_req())
            .await
            .unwrap();

        assert_created_stub(&state).await;

        remove_volume(state.req(), Named::stub().req())
            .await
            .unwrap();
        assert_empty_list(&state).await;
    }

    #[tokio::test]
    async fn creating_multiple_volumes() {
        let first_vol = "first";
        let second_vol = "second";

        let state = GitvolState::stub();
        assert_empty_list(&state).await;

        _ = create_volume(
            state.req(),
            RawCreateRequest::new(first_vol).with_url("/some").to_req(),
        )
        .await
        .unwrap();

        assert_created(&state, vec![first_vol]).await;

        _ = create_volume(
            state.req(),
            RawCreateRequest::new(second_vol).with_url("/some").to_req(),
        )
        .await
        .unwrap();

        assert_created(&state, vec![first_vol, second_vol]).await;

        remove_volume(state.req(), Named::new(first_vol).req())
            .await
            .unwrap();
        assert_created(&state, vec![second_vol]).await;

        remove_volume(state.req(), Named::new(second_vol).req())
            .await
            .unwrap();
        assert_empty_list(&state).await;
    }

    #[tokio::test]
    async fn onetime_creating_and_mounting_volume() {
        let test_repo = TestRepo::get_or_create().await.unwrap();
        let (state, _) = GitvolState::temp();
        assert_empty_list(&state).await;

        _ = create_volume(
            state.req(),
            RawCreateRequest::stub().with_url(&test_repo.file).to_req(),
        )
        .await
        .unwrap();

        assert_created_stub(&state).await;

        let Mp { mountpoint } = mount_volume_to_container(state.req(), NamedWID::stub().to_req())
            .await
            .unwrap();

        assert_list(
            &state,
            1,
            vec![
                CheckVol::stub()
                    .with_status(RepoStatus::Clonned)
                    .with_mp(Some(mountpoint.clone())),
            ],
        )
        .await;
        assert!(mountpoint.exists());
        assert!(!is_git_dir(&mountpoint));
        assert!(test_repo.is_master(&mountpoint));

        remove_volume(state.req(), Named::stub().req())
            .await
            .unwrap();

        assert!(!mountpoint.exists());
        assert_empty_list(&state).await;
    }

    #[tokio::test]
    async fn mount_and_unmount_pipeline() {
        let test_repo = TestRepo::get_or_create().await.unwrap();
        let named_1 = NamedWID::stub();
        let named_2 = NamedWID::stub();
        let check_vol = CheckVol::stub();

        let (state, _) = GitvolState::temp();
        assert_empty_list(&state).await;

        _ = create_volume(
            state.req(),
            RawCreateRequest::stub().with_url(&test_repo.file).to_req(),
        )
        .await
        .unwrap();

        assert_list(&state, 1, vec![check_vol.clone()]).await;

        let mp1 = mount_volume_to_container(state.req(), named_1.clone().to_req())
            .await
            .unwrap();
        assert_list(
            &state,
            1,
            vec![
                check_vol
                    .clone()
                    .with_status(RepoStatus::Clonned)
                    .with_mp(Some(mp1.mountpoint.clone())),
            ],
        )
        .await;
        assert!(mp1.mountpoint.exists());

        let mp2 = mount_volume_to_container(state.req(), named_2.clone().to_req())
            .await
            .unwrap();
        assert_eq!(mp1.mountpoint, mp2.mountpoint);
        assert_list(
            &state,
            1,
            vec![
                check_vol
                    .clone()
                    .with_status(RepoStatus::Clonned)
                    .with_mp(Some(mp2.mountpoint.clone())),
            ],
        )
        .await;
        assert!(mp1.mountpoint.exists());

        let _ = unmount_volume_by_container(state.req(), named_1.clone().to_req())
            .await
            .unwrap();
        assert_list(
            &state,
            1,
            vec![
                check_vol
                    .clone()
                    .with_status(RepoStatus::Clonned)
                    .with_mp(Some(mp2.mountpoint.clone())),
            ],
        )
        .await;
        assert!(mp1.mountpoint.exists());

        let _ = unmount_volume_by_container(state.req(), named_2.clone().to_req())
            .await
            .unwrap();
        assert_list(
            &state,
            1,
            vec![
                check_vol
                    .clone()
                    .with_status(RepoStatus::Cleared)
                    .with_mp(None),
            ],
        )
        .await;
        assert!(!mp1.mountpoint.exists());
    }

    #[tokio::test]
    async fn create_and_mount_default_branch() {
        let test_repo = TestRepo::get_or_create().await.unwrap();
        let (state, _) = GitvolState::temp();

        let _ = create_volume(
            state.req(),
            RawCreateRequest::stub().with_url(&test_repo.file).to_req(),
        )
        .await
        .unwrap();
        let Mp { mountpoint } = mount_volume_to_container(state.req(), NamedWID::stub().to_req())
            .await
            .unwrap();

        assert!(!is_git_dir(&mountpoint));
        assert!(test_repo.is_master(&mountpoint));
    }

    #[tokio::test]
    async fn create_and_mount_specific_branch() {
        let test_repo = TestRepo::get_or_create().await.unwrap();
        let (state, _) = GitvolState::temp();

        let _ = create_volume(
            state.req(),
            RawCreateRequest::stub()
                .with_url(&test_repo.file)
                .with_branch(&test_repo.develop)
                .to_req(),
        )
        .await
        .unwrap();
        let Mp { mountpoint } = mount_volume_to_container(state.req(), NamedWID::stub().to_req())
            .await
            .unwrap();

        assert!(!is_git_dir(&mountpoint));
        assert!(test_repo.is_develop(&mountpoint));
    }

    #[tokio::test]
    async fn create_and_mount_tag() {
        let test_repo = TestRepo::get_or_create().await.unwrap();
        let (state, _) = GitvolState::temp();

        let _ = create_volume(
            state.req(),
            RawCreateRequest::stub()
                .with_url(&test_repo.file)
                .with_tag(&test_repo.tag)
                .to_req(),
        )
        .await
        .unwrap();
        let Mp { mountpoint } = mount_volume_to_container(state.req(), NamedWID::stub().to_req())
            .await
            .unwrap();

        assert!(!is_git_dir(&mountpoint));
        assert!(test_repo.is_tag(&mountpoint));
    }

    #[tokio::test]
    async fn create_refetchable_and_mount_twice() {
        let test_repo = TestRepo::get_or_create().await.unwrap();
        let (state, _) = GitvolState::temp();
        let branch_name = format!("branch_{}", Uuid::new_v4().to_string());

        test_repo.setup_temp_branch(&branch_name).await.unwrap();

        let _ = create_volume(
            state.req(),
            RawCreateRequest::stub()
                .with_url(&test_repo.file)
                .with_branch(&branch_name)
                .with_refetch(true)
                .to_req(),
        )
        .await
        .unwrap();
        let Mp { mountpoint } = mount_volume_to_container(state.req(), NamedWID::stub().to_req())
            .await
            .unwrap();

        assert!(is_git_dir(&mountpoint));
        assert!(
            !TestRepo::is_temp_changed(&mountpoint, &branch_name)
                .await
                .unwrap()
        );

        test_repo.change_temp_branch(&branch_name).await.unwrap();
        let Mp {
            mountpoint: mountpoint2,
        } = mount_volume_to_container(state.req(), NamedWID::stub().to_req())
            .await
            .unwrap();

        assert_eq!(mountpoint, mountpoint2);
        assert!(
            TestRepo::is_temp_changed(&mountpoint, &branch_name)
                .await
                .unwrap()
        );
    }
}
