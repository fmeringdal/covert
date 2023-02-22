use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateNamespaceParams {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateNamespaceResponse {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ListNamespaceResponse {
    pub namespaces: Vec<ListNamespaceItemResponse>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ListNamespaceItemResponse {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DeleteNamespaceResponse {
    pub id: String,
    pub name: String,
}
