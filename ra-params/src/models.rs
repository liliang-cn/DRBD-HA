use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct ResourceAgent {
    #[serde(rename = "@name")]
    pub name: String,
    
    #[serde(rename = "@version")]
    pub version_attr: Option<String>,

    #[serde(rename = "version")]
    pub version_elem: Option<String>,

    #[serde(default)]
    pub longdesc: LocalizedText,
    #[serde(default)]
    pub shortdesc: LocalizedText,
    #[serde(default)]
    pub parameters: Parameters,
    #[serde(default)]
    pub actions: Actions,
}

impl ResourceAgent {
    pub fn version(&self) -> String {
        self.version_attr
            .clone()
            .or_else(|| self.version_elem.clone())
            .unwrap_or_else(|| "0.0".to_string())
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use quick_xml::de::from_str;
    use std::fs;

    #[test]
    fn test_parse_rabbitmq() {
        let xml_path = "../ra-params/samples/rabbitmq-server-ha.xml";
        let xml_content = fs::read_to_string(xml_path).expect("Failed to read XML file");
        let result: Result<ResourceAgent, _> = from_str(&xml_content);
        
        match result {
            Ok(ra) => {
                println!("Successfully parsed: {:?}", ra.name);
                println!("Version: {}", ra.version());
                assert_eq!(ra.version(), "1.0");
            },
            Err(e) => panic!("Failed to parse: {}", e),
        }
    }

    #[test]
    fn test_parse_ocivip() {
        let xml_path = "../ra-params/samples/ocivip.xml";
        let xml_content = fs::read_to_string(xml_path).expect("Failed to read XML file");
        let result: Result<ResourceAgent, _> = from_str(&xml_content);
        
        match result {
            Ok(ra) => {
                println!("Successfully parsed: {:?}", ra.name);
                println!("Version: {}", ra.version());
                assert_eq!(ra.version(), "1.0");
            },
            Err(e) => panic!("Failed to parse: {}", e),
        }
    }
}