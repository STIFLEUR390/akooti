use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct File {
    #[serde(default)]
    pub(crate) accounts: Vec<Account>,
}

#[derive(Deserialize)]
pub(crate) struct Account {
    #[serde(default)]
    pub(crate) vaults: Vec<Vault>,
}

#[derive(Deserialize)]
pub(crate) struct Vault {
    pub(crate) attrs: VaultAttributes,
    #[serde(default)]
    pub(crate) items: Vec<Item>,
}

#[derive(Deserialize)]
pub(crate) struct VaultAttributes {
    pub(crate) uuid: String,
    #[serde(default)]
    pub(crate) name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Item {
    pub(crate) uuid: String,
    #[serde(default)]
    pub(crate) fav_index: u32,
    pub(crate) category_uuid: String,
    pub(crate) details: Details,
    pub(crate) overview: Overview,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Details {
    #[serde(default)]
    pub(crate) login_fields: Vec<LoginField>,
    pub(crate) notes_plain: Option<String>,
    #[serde(default)]
    pub(crate) sections: Vec<Section>,
    pub(crate) password: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoginField {
    pub(crate) value: String,
    pub(crate) name: String,
    pub(crate) designation: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct Section {
    #[serde(default)]
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) fields: Vec<Field>,
}

#[derive(Deserialize)]
pub(crate) struct Field {
    #[serde(default)]
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) value: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub(crate) guarded: bool,
}

#[derive(Deserialize, Default)]
pub(crate) struct Overview {
    #[serde(default)]
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) urls: Vec<OverviewUrl>,
    pub(crate) url: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct OverviewUrl {
    #[serde(default)]
    pub(crate) url: String,
}
