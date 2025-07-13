use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    pub url: String,
    pub branch: Option<String>,
    pub updatable: bool,
}

#[derive(Debug, Clone)]
pub struct GitvolState;
