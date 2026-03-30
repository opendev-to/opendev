use ts_rs::TS;
use serde::Serialize;

#[derive(Serialize, TS)]
#[ts(export)]
pub struct MyStruct {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub my_field: Option<String>,
}

fn main() {}
