use std::{fmt::Debug, path::PathBuf};

use axum::Router;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[cfg_attr(test, derive(Debug, PartialEq, Deserialize))]
#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(unused)]
pub enum Scope {
    Local,
    Global,
}

#[cfg_attr(test, derive(Debug, PartialEq, Deserialize))]
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ItemVolume {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mountpoint: Option<PathBuf>,
}

#[cfg_attr(test, derive(Clone))]
pub struct VolumeInfo<S> {
    pub mountpoint: Option<PathBuf>,
    pub status: S,
}

#[async_trait::async_trait]
pub trait Driver: Clone + Send + Sync + 'static {
    type Error: std::error::Error + Send + Sync + 'static;
    type Status: Serialize + Send + Sync + 'static;
    type Opts: DeserializeOwned + Debug + Send + Sync + 'static;

    async fn activate(&self) -> Result<Vec<String>, Self::Error> {
        Ok(vec!["VolumeDriver".to_string()])
    }

    async fn capabilities(&self) -> Result<Scope, Self::Error>;
    async fn path(&self, name: &str) -> Result<Option<PathBuf>, Self::Error>;
    async fn get(&self, name: &str) -> Result<VolumeInfo<Self::Status>, Self::Error>;
    async fn list(&self) -> Result<Vec<ItemVolume>, Self::Error>;
    async fn create(&self, name: &str, opts: Option<Self::Opts>) -> Result<(), Self::Error>;
    async fn remove(&self, name: &str) -> Result<(), Self::Error>;
    async fn mount(&self, name: &str, id: &str) -> Result<PathBuf, Self::Error>;
    async fn unmount(&self, name: &str, id: &str) -> Result<(), Self::Error>;

    #[allow(dead_code)]
    fn into_router(self) -> Router {
        router::create_router(self)
    }
}

mod router {

    use super::*;
    use axum::{
        Json, Router,
        extract::{Request, State},
        http::{HeaderValue, Uri, header::CONTENT_TYPE},
        middleware::{self, Next},
        response::{IntoResponse, Response},
        routing::post,
    };
    use serde::Serialize;

    macro_rules! log_request {
        ($uri:ident, $($arg:tt)+) => {
            println!("[DEBUG: {}] :: Request: {}", $uri.to_string(), format!($($arg)*))
        };
        ($uri:ident) => {
            println!("[DEBUG: {}] :: Request", $uri.to_string())
        };
    }
    macro_rules! parse_response {
        ($uri:ident, $result:ident, $($arg:tt)+) => {
            $result.map(Json).map_err(|e| {
                let err = e.to_string();
                println!("[ERROR: {}] :: Failed: {}. {}", $uri.to_string(), err, format!($($arg)*));
                DriverError { err }
            })
        };
        ($uri:ident, $result:ident) => {
            $result.map(Json).map_err(|e| {
                let err = e.to_string();
                println!("[ERROR: {}] :: Failed: {}", $uri.to_string(), err);
                DriverError { err }
            })
        };
    }

    #[cfg_attr(test, derive(Debug, PartialEq, Deserialize))]
    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct DriverError {
        pub err: String,
    }

    impl IntoResponse for DriverError {
        fn into_response(self) -> axum::response::Response {
            Json(self).into_response()
        }
    }

    type Result<T> = std::result::Result<Json<T>, DriverError>;

    #[cfg_attr(test, derive(Debug, PartialEq, Serialize))]
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct Named {
        pub name: String,
    }

    #[cfg_attr(test, derive(Debug, PartialEq, Serialize))]
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct NamedWID {
        pub name: String,
        #[serde(rename = "ID")]
        pub id: String,
    }

    #[cfg_attr(test, derive(Debug, PartialEq, Deserialize))]
    #[derive(Serialize, Clone)]
    pub struct Empty {}

    #[cfg_attr(test, derive(Debug, PartialEq, Deserialize))]
    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct ImplementsDriver {
        pub implements: Vec<String>,
    }

    #[cfg_attr(test, derive(Debug, PartialEq, Deserialize))]
    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct Capabilities {
        pub scope: Scope,
    }

    #[cfg_attr(test, derive(Debug, PartialEq, Deserialize))]
    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct CapabilitiesResponse {
        pub capabilities: Capabilities,
    }

    #[cfg_attr(test, derive(Debug, PartialEq, Deserialize))]
    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct OptionalMountpoint {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub mountpoint: Option<PathBuf>,
    }

    #[cfg_attr(test, derive(Debug, PartialEq, Deserialize))]
    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct FullVolume<S> {
        pub name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub mountpoint: Option<PathBuf>,
        pub status: S,
    }

    #[cfg_attr(test, derive(Debug, PartialEq, Deserialize))]
    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct GetResponse<S> {
        pub volume: FullVolume<S>,
    }

    #[cfg_attr(test, derive(Debug, PartialEq, Deserialize))]
    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct ListResponse {
        pub volumes: Vec<ItemVolume>,
    }

    #[cfg_attr(test, derive(Debug, PartialEq, Serialize))]
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct CreateRequest<O> {
        pub name: String,
        pub opts: Option<O>,
    }

    #[cfg_attr(test, derive(Debug, PartialEq, Deserialize))]
    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct Mountpoint {
        pub mountpoint: PathBuf,
    }

    async fn activate_handler<D: Driver>(
        uri: Uri,
        State(driver): State<D>,
    ) -> Result<ImplementsDriver> {
        log_request!(uri);
        let result = driver
            .activate()
            .await
            .map(|implements| ImplementsDriver { implements });
        parse_response!(uri, result)
    }

    async fn capabilities_handler<D: Driver>(
        uri: Uri,
        State(driver): State<D>,
    ) -> Result<CapabilitiesResponse> {
        log_request!(uri);
        let result = driver
            .capabilities()
            .await
            .map(|scope| CapabilitiesResponse {
                capabilities: Capabilities { scope },
            });
        parse_response!(uri, result)
    }

    async fn path_handler<D: Driver>(
        uri: Uri,
        State(driver): State<D>,
        Json(Named { name }): Json<Named>,
    ) -> Result<OptionalMountpoint> {
        log_request!(uri, "volume_name={}", name);
        let result = driver
            .path(&name)
            .await
            .map(|mountpoint| OptionalMountpoint { mountpoint });
        parse_response!(uri, result, "volume_name={}", name)
    }

    async fn get_handler<D: Driver>(
        uri: Uri,
        State(driver): State<D>,
        Json(Named { name }): Json<Named>,
    ) -> Result<GetResponse<D::Status>> {
        log_request!(uri, "volume_name={}", name);
        let result = driver
            .get(&name)
            .await
            .map(|VolumeInfo { mountpoint, status }| GetResponse {
                volume: FullVolume {
                    name: name.clone(),
                    mountpoint,
                    status,
                },
            });
        parse_response!(uri, result, "volume_name={}", name)
    }

    async fn list_handler<D: Driver>(uri: Uri, State(driver): State<D>) -> Result<ListResponse> {
        log_request!(uri);
        let result = driver.list().await.map(|volumes| ListResponse { volumes });
        parse_response!(uri, result)
    }

    async fn create_handler<D: Driver>(
        uri: Uri,
        State(driver): State<D>,
        Json(CreateRequest { name, opts }): Json<CreateRequest<D::Opts>>,
    ) -> Result<Empty> {
        log_request!(uri, "volume_name={}, create_options={:?}", name, opts);
        let result = driver.create(&name, opts).await.map(|_| Empty {});
        parse_response!(uri, result, "volume_name={}", name)
    }

    async fn remove_handler<D: Driver>(
        uri: Uri,
        State(driver): State<D>,
        Json(Named { name }): Json<Named>,
    ) -> Result<Empty> {
        log_request!(uri, "volume_name={}", name);
        let result = driver.remove(&name).await.map(|_| Empty {});
        parse_response!(uri, result, "volume_name={}", name)
    }

    async fn mount_handler<D: Driver>(
        uri: Uri,
        State(driver): State<D>,
        Json(NamedWID { name, id }): Json<NamedWID>,
    ) -> Result<Mountpoint> {
        log_request!(uri, "volume_name={}; id={}", name, id);
        let result = driver
            .mount(&name, &id)
            .await
            .map(|mountpoint| Mountpoint { mountpoint });
        parse_response!(uri, result, "volume_name={}; id={}", name, id)
    }

    async fn unmount_handler<D: Driver>(
        uri: Uri,
        State(driver): State<D>,
        Json(NamedWID { name, id }): Json<NamedWID>,
    ) -> Result<Empty> {
        log_request!(uri, "volume_name={}; id={}", name, id);
        let result = driver.unmount(&name, &id).await.map(|_| Empty {});
        parse_response!(uri, result, "volume_name={}; id={}", name, id)
    }

    pub fn create_router<D: Driver + 'static>(driver: D) -> Router {
        Router::new()
            .route("/Plugin.Activate", post(activate_handler::<D>))
            .route(
                "/VolumeDriver.Capabilities",
                post(capabilities_handler::<D>),
            )
            .route("/VolumeDriver.Path", post(path_handler::<D>))
            .route("/VolumeDriver.Get", post(get_handler::<D>))
            .route("/VolumeDriver.List", post(list_handler::<D>))
            .route("/VolumeDriver.Create", post(create_handler::<D>))
            .route("/VolumeDriver.Remove", post(remove_handler::<D>))
            .route("/VolumeDriver.Mount", post(mount_handler::<D>))
            .route("/VolumeDriver.Unmount", post(unmount_handler::<D>))
            .layer(middleware::from_fn(transform_headers))
            .with_state(driver)
    }

    async fn transform_headers(mut request: Request, next: Next) -> Response {
        let headers = request.headers_mut();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let mut response = next.run(request).await;
        let response_headers = response.headers_mut();
        response_headers.append(
            CONTENT_TYPE,
            HeaderValue::from_static("application/vnd.docker.plugin.v1+json"),
        );

        response
    }
}

#[cfg(test)]
mod test_mocks {
    use super::router::*;
    use super::*;
    use axum_test::TestServer;
    use std::{collections::HashMap, ops::Deref, sync::Arc};
    use tokio::sync::Mutex;

    pub const VOLUME_NAME: &str = "test_volume";
    const BASE_PATH: &str = "/plugin";
    const DEFAULT_OPTS: &str = "def";
    pub const MOUNTED_STATUS: &str = "mounted";
    pub const UNMOUNTED_STATUS: &str = "unmounted";
    pub const PATH: &str = "/VolumeDriver.Path";
    pub const GET: &str = "/VolumeDriver.Get";
    pub const LIST: &str = "/VolumeDriver.List";
    pub const CREATE: &str = "/VolumeDriver.Create";
    pub const REMOVE: &str = "/VolumeDriver.Remove";
    pub const MOUNT: &str = "/VolumeDriver.Mount";
    pub const UNMOUNT: &str = "/VolumeDriver.Unmount";

    pub fn base_mp() -> PathBuf {
        PathBuf::from(BASE_PATH).join(DEFAULT_OPTS)
    }

    #[derive(Debug)]
    pub struct StrError(String);

    impl std::fmt::Display for StrError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            std::fmt::Display::fmt(&self.0, f)
        }
    }

    impl std::error::Error for StrError {}

    #[derive(Clone)]
    pub struct Test {
        volumes: Arc<Mutex<HashMap<String, VolumeInfo<String>>>>,
        next_error: Arc<Mutex<Option<String>>>,
    }

    impl Test {
        fn new() -> Self {
            Self {
                volumes: Arc::new(Mutex::new(HashMap::new())),
                next_error: Arc::new(Mutex::new(None)),
            }
        }

        async fn set_error(&self, msg: &str) {
            let mut next_error = self.next_error.lock().await;
            *next_error = Some(msg.to_string());
        }

        async fn check_error(&self) -> Result<(), StrError> {
            let mut next_error = self.next_error.lock().await;
            if let Some(msg) = next_error.take() {
                return Err(StrError(msg));
            }
            Ok(())
        }

        pub fn into_server() -> Server {
            let app = Self::new();
            let server = TestServer::new(app.clone().into_router()).unwrap();
            Server { app, server }
        }
    }

    #[async_trait::async_trait]
    impl Driver for Test {
        type Error = StrError;
        type Status = String;
        type Opts = String;

        async fn capabilities(&self) -> Result<Scope, Self::Error> {
            self.check_error().await?;
            Ok(Scope::Global)
        }

        async fn path(&self, name: &str) -> Result<Option<PathBuf>, Self::Error> {
            self.check_error().await?;
            let volumes = self.volumes.lock().await;
            let vol = volumes.get(name);
            Ok(vol.and_then(|v| v.mountpoint.clone()))
        }

        async fn get(&self, name: &str) -> Result<VolumeInfo<Self::Status>, Self::Error> {
            self.check_error().await?;
            let volumes = self.volumes.lock().await;
            volumes
                .get(name)
                .cloned()
                .ok_or(StrError("not found".into()))
        }

        async fn list(&self) -> Result<Vec<ItemVolume>, Self::Error> {
            self.check_error().await?;
            let volumes = self.volumes.lock().await;
            let list = volumes
                .iter()
                .map(|(k, v)| ItemVolume {
                    mountpoint: v.mountpoint.clone(),
                    name: k.clone(),
                })
                .collect::<Vec<ItemVolume>>();
            Ok(list)
        }

        async fn create(&self, name: &str, opts: Option<Self::Opts>) -> Result<(), Self::Error> {
            self.check_error().await?;
            let Some(opts) = opts else {
                return Err(StrError("empty options".into()));
            };
            let mut volumes = self.volumes.lock().await;
            volumes.insert(
                name.to_string(),
                VolumeInfo {
                    mountpoint: None,
                    status: opts,
                },
            );
            Ok(())
        }

        async fn remove(&self, name: &str) -> Result<(), Self::Error> {
            self.check_error().await?;
            let mut volumes = self.volumes.lock().await;
            volumes.remove(name);
            Ok(())
        }

        async fn mount(&self, name: &str, _id: &str) -> Result<PathBuf, Self::Error> {
            self.check_error().await?;
            let VolumeInfo { mountpoint, status } = self.get(name).await?;
            if let Some(path) = mountpoint {
                return Ok(path);
            }

            let mountpoint = PathBuf::from(BASE_PATH).join(status.clone());
            let mut volumes = self.volumes.lock().await;
            volumes.insert(
                name.to_string(),
                VolumeInfo {
                    mountpoint: Some(mountpoint.clone()),
                    status: MOUNTED_STATUS.to_string(),
                },
            );

            Ok(mountpoint)
        }

        async fn unmount(&self, name: &str, _id: &str) -> Result<(), Self::Error> {
            self.check_error().await?;
            let VolumeInfo { mountpoint, .. } = self.get(name).await?;
            if mountpoint.is_some() {
                let mut volumes = self.volumes.lock().await;
                volumes.insert(
                    name.to_string(),
                    VolumeInfo {
                        mountpoint: None,
                        status: UNMOUNTED_STATUS.to_string(),
                    },
                );
                return Ok(());
            }

            Ok(())
        }
    }

    pub struct Server {
        app: Test,
        server: TestServer,
    }

    impl Deref for Server {
        type Target = TestServer;

        fn deref(&self) -> &Self::Target {
            &self.server
        }
    }

    impl Server {
        pub async fn set_error(&self, msg: &str) {
            self.app.set_error(msg).await;
        }
    }

    impl Named {
        pub fn stub() -> Self {
            Self {
                name: VOLUME_NAME.into(),
            }
        }
    }

    impl NamedWID {
        fn stub_id(id: &str) -> Self {
            Self {
                name: VOLUME_NAME.to_string(),
                id: id.to_string(),
            }
        }

        pub fn stub() -> Self {
            Self::stub_id("id")
        }
    }

    impl ItemVolume {
        fn stub() -> Self {
            Self {
                name: VOLUME_NAME.to_string(),
                mountpoint: None,
            }
        }
    }

    impl ListResponse {
        fn new(volumes: Vec<ItemVolume>) -> Self {
            Self { volumes }
        }

        fn item(item: ItemVolume) -> Self {
            Self::new(vec![item])
        }

        pub fn stub_item() -> Self {
            Self::item(ItemVolume::stub())
        }

        pub fn empty() -> Self {
            Self::new(vec![])
        }
    }

    impl DriverError {
        pub fn new(msg: &str) -> Self {
            Self {
                err: msg.to_string(),
            }
        }
    }

    impl CreateRequest<String> {
        fn new(name: &str, opts: &str) -> Self {
            Self {
                name: name.to_string(),
                opts: Some(opts.to_string()),
            }
        }

        pub fn stub() -> Self {
            Self::new(VOLUME_NAME, DEFAULT_OPTS)
        }
    }

    impl GetResponse<String> {
        fn new(volume: FullVolume<String>) -> Self {
            Self { volume }
        }

        pub fn stub() -> Self {
            Self::new(FullVolume {
                name: VOLUME_NAME.to_string(),
                mountpoint: None,
                status: DEFAULT_OPTS.to_string(),
            })
        }

        pub fn stub_mount(mountpoint: Option<PathBuf>, status: &str) -> Self {
            Self::new(FullVolume {
                name: VOLUME_NAME.to_string(),
                mountpoint,
                status: status.to_string(),
            })
        }
    }

    impl Mountpoint {
        fn new(mountpoint: PathBuf) -> Self {
            Self { mountpoint }
        }

        pub fn stub() -> Self {
            Self::new(base_mp())
        }
    }

    impl OptionalMountpoint {
        pub fn new(mountpoint: Option<PathBuf>) -> Self {
            Self { mountpoint }
        }

        pub fn empty() -> Self {
            Self::new(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::router::*;
    use super::test_mocks::*;
    use super::*;

    mod first_requests {
        use super::*;

        #[tokio::test]
        async fn activate_plugin() {
            Test::into_server()
                .post("/Plugin.Activate")
                .await
                .assert_json(&ImplementsDriver {
                    implements: vec!["VolumeDriver".into()],
                });
        }

        #[tokio::test]
        async fn capabilities() {
            Test::into_server()
                .post("/VolumeDriver.Capabilities")
                .await
                .assert_json(&CapabilitiesResponse {
                    capabilities: Capabilities {
                        scope: Scope::Global,
                    },
                });
        }

        #[tokio::test]
        async fn empty_list() {
            Test::into_server()
                .post(LIST)
                .await
                .assert_json(&ListResponse::empty());
        }

        #[tokio::test]
        async fn list_with_error() {
            let server = Test::into_server();
            server.set_error("list error").await;
            server
                .post(LIST)
                .await
                .assert_json(&DriverError::new("list error"));
        }

        #[tokio::test]
        async fn empty_path() {
            Test::into_server()
                .post(PATH)
                .json(&Named::stub())
                .await
                .assert_json(&OptionalMountpoint::empty());
        }

        #[tokio::test]
        async fn path_with_error() {
            let server = Test::into_server();
            server.set_error("path error").await;
            server
                .post(PATH)
                .json(&Named::stub())
                .await
                .assert_json(&DriverError::new("path error"));
        }

        #[tokio::test]
        async fn empty_get() {
            Test::into_server()
                .post(GET)
                .json(&Named::stub())
                .await
                .assert_json(&DriverError::new("not found"));
        }

        #[tokio::test]
        async fn get_with_error() {
            let server = Test::into_server();
            server.set_error("get error").await;
            server
                .post(GET)
                .json(&Named::stub())
                .await
                .assert_json(&DriverError::new("get error"));
        }
    }

    #[tokio::test]
    async fn failed_created_volume_with_empty_opts() {
        Test::into_server()
            .post(CREATE)
            .json(&CreateRequest::<String> {
                name: VOLUME_NAME.into(),
                opts: None,
            })
            .await
            .assert_json(&DriverError::new("empty options"));
    }

    #[tokio::test]
    async fn failed_created_volume() {
        let server = Test::into_server();

        server.set_error("creating error").await;
        server
            .post(CREATE)
            .json(&CreateRequest::stub())
            .await
            .assert_json(&DriverError::new("creating error"));
    }

    #[tokio::test]
    async fn successfully_created_volume() {
        let server = Test::into_server();
        server
            .post(CREATE)
            .json(&CreateRequest::stub())
            .await
            .assert_json(&Empty {});

        server
            .post(LIST)
            .await
            .assert_json(&ListResponse::stub_item());
        server
            .post(GET)
            .json(&Named::stub())
            .await
            .assert_json(&GetResponse::stub());
        server
            .post(PATH)
            .json(&Named::stub())
            .await
            .assert_json(&OptionalMountpoint::empty());
    }

    #[tokio::test]
    async fn failed_remove_volume() {
        let server = Test::into_server();
        server.post(CREATE).json(&CreateRequest::stub()).await;

        server.set_error("remove error").await;
        server
            .post(REMOVE)
            .json(&Named::stub())
            .await
            .assert_json(&DriverError::new("remove error"));

        server
            .post(LIST)
            .await
            .assert_json(&ListResponse::stub_item());
    }

    #[tokio::test]
    async fn successfully_remove_volume() {
        let server = Test::into_server();
        server.post(CREATE).json(&CreateRequest::stub()).await;

        server
            .post(REMOVE)
            .json(&Named::stub())
            .await
            .assert_json(&Empty {});

        server.post(LIST).await.assert_json(&ListResponse::empty());
    }

    #[tokio::test]
    async fn failed_non_existent_mount() {
        Test::into_server()
            .post(MOUNT)
            .json(&NamedWID::stub())
            .await
            .assert_json(&DriverError::new("not found"));
    }

    #[tokio::test]
    async fn failed_mount() {
        let server = Test::into_server();
        server.set_error("mount error").await;
        server
            .post(MOUNT)
            .json(&NamedWID::stub())
            .await
            .assert_json(&DriverError::new("mount error"));
    }

    #[tokio::test]
    async fn failed_non_existent_unmount() {
        Test::into_server()
            .post(UNMOUNT)
            .json(&NamedWID::stub())
            .await
            .assert_json(&DriverError::new("not found"));
    }
    #[tokio::test]
    async fn failed_unmount() {
        let server = Test::into_server();
        server.set_error("unmount error").await;
        server
            .post(UNMOUNT)
            .json(&NamedWID::stub())
            .await
            .assert_json(&DriverError::new("unmount error"));
    }

    #[tokio::test]
    async fn successfully_mount() {
        let server = Test::into_server();
        server.post(CREATE).json(&CreateRequest::stub()).await;

        server
            .post(MOUNT)
            .json(&NamedWID::stub())
            .await
            .assert_json(&Mountpoint::stub());
        server
            .post(GET)
            .json(&Named::stub())
            .await
            .assert_json(&GetResponse::stub_mount(Some(base_mp()), MOUNTED_STATUS));
        server
            .post(PATH)
            .json(&Named::stub())
            .await
            .assert_json(&OptionalMountpoint::new(Some(base_mp())));
    }

    #[tokio::test]
    async fn successfully_unmount() {
        let server = Test::into_server();
        server.post(CREATE).json(&CreateRequest::stub()).await;

        server
            .post(MOUNT)
            .json(&NamedWID::stub())
            .await
            .assert_json(&Mountpoint::stub());
        server
            .post(UNMOUNT)
            .json(&NamedWID::stub())
            .await
            .assert_json(&Empty {});

        server
            .post(GET)
            .json(&Named::stub())
            .await
            .assert_json(&GetResponse::stub_mount(None, UNMOUNTED_STATUS));
        server
            .post(PATH)
            .json(&Named::stub())
            .await
            .assert_json(&OptionalMountpoint::new(None));
    }
}
