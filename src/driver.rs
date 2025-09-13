use std::{fmt::Debug, path::PathBuf};

// use axum::Router;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(unused)]
pub enum Scope {
    Local,
    Global,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ItemVolume {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mountpoint: Option<PathBuf>,
}

pub struct VolumeInfo<S> {
    pub mountpoint: Option<PathBuf>,
    pub status: S,
}

#[async_trait::async_trait]
pub trait Driver: Clone + Send + Sync + 'static {
    type Error: std::error::Error;
    type Status: Serialize;
    type Opts: DeserializeOwned + Debug + Send;

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

    // fn into_router(self) -> Router {
    //     router::create_router(self)
    // }
}

mod router {
    #![allow(warnings)]
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
            $result.map(|r| Json(r)).map_err(|e| {
                let err = e.to_string();

                println!("[ERROR: {}] :: Failed: {}. {}", $uri.to_string(), err, format!($($arg)*));
                DriverError { err }
            })
        };
        ($uri:ident, $result:ident) => {
            $result.map(|r| Json(r)).map_err(|e| {
                let err = e.to_string();

                println!("[ERROR: {}] :: Failed: {}", $uri.to_string(), err);
                DriverError { err }
            })
        };
    }

    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    struct DriverError {
        err: String,
    }

    impl IntoResponse for DriverError {
        fn into_response(self) -> axum::response::Response {
            Json(self).into_response()
        }
    }

    type Result<T> = std::result::Result<Json<T>, DriverError>;

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct Named {
        pub name: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub(super) struct NamedWID {
        pub(super) name: String,
        #[serde(rename = "ID")]
        pub(super) id: String,
    }

    #[derive(Serialize, Clone)]
    struct Empty {}

    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    struct ImplementsDriver {
        implements: Vec<String>,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    struct Capabilities {
        scope: Scope,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    struct CapabilitiesResponse {
        capabilities: Capabilities,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    struct OptionalMountpoint {
        #[serde(skip_serializing_if = "Option::is_none")]
        mountpoint: Option<PathBuf>,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    struct FullVolume<S> {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        mountpoint: Option<PathBuf>,
        status: S,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    struct GetResponse<S> {
        volume: FullVolume<S>,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    struct ListResponse {
        volumes: Vec<ItemVolume>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct CreateRequest<O> {
        name: String,
        opts: Option<O>,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    struct Mountpoint {
        mountpoint: PathBuf,
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
            .path(&name.clone())
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
        headers.append(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let mut response = next.run(request).await;
        let response_headers = response.headers_mut();
        response_headers.append(
            CONTENT_TYPE,
            HeaderValue::from_static("application/vnd.docker.plugin.v1+json"),
        );

        response
    }
}
