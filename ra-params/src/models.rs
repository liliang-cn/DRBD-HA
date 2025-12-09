use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct ResourceAgent {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@version")]
    pub version: String,
    #[serde(default)]
    pub longdesc: LocalizedText,
    #[serde(default)]
    pub shortdesc: LocalizedText,
    #[serde(default)]
    pub parameters: Parameters,
    #[serde(default)]
    pub actions: Actions,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct LocalizedText {
    #[serde(rename = "@lang", default)]
    pub lang: String,
    #[serde(rename = "$value", default)]
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct Parameters {
    #[serde(rename = "parameter", default)]
    pub parameters: Vec<Parameter>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Parameter {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@unique", default)]
    pub unique: String,
    #[serde(rename = "@required", default)]
    pub required: String,
    #[serde(default)]
    pub longdesc: LocalizedText,
    #[serde(default)]
    pub shortdesc: LocalizedText,
    #[serde(default)]
    pub content: Content,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct Content {
    #[serde(rename = "@type", default)]
    pub type_: String,
    #[serde(rename = "@default", default)]
    pub default: String,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct Actions {
    #[serde(rename = "action", default)]
    pub actions: Vec<Action>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Action {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@timeout", default)]
    pub timeout: String,
    #[serde(rename = "@interval", default)]
    pub interval: String,
    #[serde(rename = "@depth", default)]
    pub depth: String,
}
