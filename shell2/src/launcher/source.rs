use loro::{LoroDoc, LoroText};

pub(super) const ENTRY: &str = "shell/launcher/v1";

pub(super) struct State {
    pub doc: LoroDoc,
    pub generation: u64,
}

impl State {
    pub fn replace(&mut self, doc: LoroDoc) {
        self.doc = doc;
        self.generation = self.generation.wrapping_add(1);
    }
}

pub(super) fn import(bytes: &[u8]) -> Result<LoroDoc, String> {
    let source = LoroDoc::new();
    source.import(bytes).map_err(|e| e.to_string())?;
    Ok(source)
}

pub(super) fn bundled() -> Result<LoroDoc, String> {
    let source = LoroDoc::new();
    for (path, content) in [
        ("main.lua", include_str!("main.lua")),
        ("theme.lua", include_str!("theme.lua")),
        ("editor.lua", include_str!("editor.lua")),
        ("editor_model.lua", include_str!("editor_model.lua")),
        ("editor_text.lua", include_str!("editor_text.lua")),
        (
            "cursor.lua",
            include_str!("../../../demo_apps/pointer/cursor.lua"),
        ),
    ] {
        let text = source
            .get_map("files")
            .insert_container(path, LoroText::new())
            .map_err(|e| e.to_string())?;
        text.insert(0, content).map_err(|e| e.to_string())?;
    }
    source.commit();
    Ok(source)
}
