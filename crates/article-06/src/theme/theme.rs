use include_dir::{Dir, include_dir};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;
use super::scheme::JsonSchemes;

static THEME_PATH: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../assets/theme/");
static THEME_MAP: LazyLock<HashMap<String, MaterialTheme>> = LazyLock::new(|| {
    THEME_PATH
        .files()
        .filter_map(|file| {
            file.path()
                .to_str()
                .map(|s| s.replace(".json", ""))
                .and_then(|file_name| {
                    file.contents_utf8().and_then(|content| {
                        load_theme(content).ok().map(|mut theme| {
                            let name = theme.name.take().unwrap_or(file_name);
                            (name, theme)
                        })
                    })
                })
        }).collect()
});

fn load_theme(content: &str) -> serde_json::Result<MaterialTheme> {
    serde_json::from_str::<MaterialTheme>(content)
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MaterialTheme {
    pub name: Option<String>,
    pub sort: Option<u8>,
    pub schemes: JsonSchemes,
}

pub fn load_theme_map() -> HashMap<String, MaterialTheme> {
    THEME_MAP.clone()
}
