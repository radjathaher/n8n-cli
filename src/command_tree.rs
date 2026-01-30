use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[allow(dead_code)]
pub struct CommandTree {
    pub version: String,
    pub base_path: String,
    pub resources: Vec<Resource>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[allow(dead_code)]
pub struct Resource {
    pub name: String,
    pub ops: Vec<Operation>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[allow(dead_code)]
pub struct Operation {
    pub name: String,
    pub method: String,
    pub path: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub params: Vec<ParamDef>,
    pub body: Option<BodyDef>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[allow(dead_code)]
pub struct ParamDef {
    pub name: String,
    pub flag: String,
    pub location: String,
    pub required: bool,
    pub schema: SchemaDef,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[allow(dead_code)]
pub struct BodyDef {
    pub required: bool,
    pub content_type: String,
    pub schema: SchemaDef,
    pub input_fields: Vec<InputField>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[allow(dead_code)]
pub struct InputField {
    pub name: String,
    pub flag: String,
    pub required: bool,
    pub schema: SchemaDef,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[allow(dead_code)]
pub struct SchemaDef {
    pub kind: String,
    pub item: Option<Box<SchemaDef>>,
}

pub fn load_command_tree() -> CommandTree {
    let raw = include_str!("../schemas/command_tree.json");
    serde_json::from_str(raw).expect("invalid command_tree.json")
}
