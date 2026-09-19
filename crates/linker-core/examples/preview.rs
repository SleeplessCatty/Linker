//! Deployment inventory. Use a COPY of the state DB: opening it can migrate metadata.
use linker_core::{state::StateDb, sync::preview_item, Result};
fn main() -> Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .expect("usage: preview <copied-state.sqlite>");
    let db = StateDb::open(std::path::Path::new(&path))?;
    let previews = db
        .list_items()?
        .iter()
        .map(|item| preview_item(&db, item))
        .collect::<Result<Vec<_>>>()?;
    println!("{}", serde_json::to_string_pretty(&previews)?);
    Ok(())
}
