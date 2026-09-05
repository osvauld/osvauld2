//! Lua as a tree. Parse once at import, edit structurally, print back — the tree is the artifact
//! and text is an import format plus an export projection (docs/design/code-as-tree.md).
//!
//! full-moon rather than tree-sitter because `app_host` runs **Luau**, and because import must
//! reject malformed source rather than tolerate it (§8½).

pub mod lower;
pub mod print;
pub mod schema;

pub use print::print;
pub use schema::*;

#[derive(Debug)]
pub enum Error {
    Parse(Vec<full_moon::Error>),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Parse(es) => {
                write!(
                    f,
                    "{}",
                    es.iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<_>>()
                        .join("; ")
                )
            }
        }
    }
}

impl std::error::Error for Error {}

/// Text in, tree out. The one place a parser runs on the way in — after this, edits are
/// structural and the parser is not in the loop.
pub fn parse(src: &str) -> Result<Block, Error> {
    let ast = full_moon::parse(src).map_err(Error::Parse)?;
    Ok(lower::lower_ast(&ast))
}

#[cfg(test)]
mod tests;
