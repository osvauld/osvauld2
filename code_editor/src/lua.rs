use tree_sitter::{Node, Parser, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Function,
    Statement,
    Comment,
}

impl BlockKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BlockKind::Function => "function",
            BlockKind::Statement => "statement",
            BlockKind::Comment => "comment",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub text: String,
}

/// Cut Lua `src` into blocks at top-level construct boundaries; every byte lands in exactly one
/// block so `emit(&split(src)) == src` for any input.
pub fn split(src: &str) -> Vec<Block> {
    if src.is_empty() {
        return Vec::new();
    }
    let tree = parse(src);
    let root = tree.root_node();

    let mut blocks: Vec<Block> = Vec::new();
    let mut cursor = 0usize;

    let mut walker = root.walk();
    for node in root.children(&mut walker) {
        let (start, end) = (node.start_byte(), node.end_byte());
        push(&mut blocks, BlockKind::Comment, &src[cursor..start]);

        // The grammar folds a construct's leading whitespace into the node. Peel it off into a
        // comment block so the construct block is clean.
        let slice = &src[start..end];
        let lead = slice.len() - slice.trim_start().len();
        push(&mut blocks, BlockKind::Comment, &slice[..lead]);
        push(&mut blocks, classify(&node), &slice[lead..]);
        cursor = end;
    }
    push(&mut blocks, BlockKind::Comment, &src[cursor..]);
    blocks
}

pub fn emit(blocks: &[Block]) -> String {
    blocks.iter().map(|b| b.text.as_str()).collect()
}

// Coalesce consecutive comment runs so whitespace + standalone comment + whitespace become one
// block rather than three; never crosses a construct boundary.
fn push(blocks: &mut Vec<Block>, kind: BlockKind, text: &str) {
    if text.is_empty() {
        return;
    }
    if kind == BlockKind::Comment {
        if let Some(last) = blocks.last_mut() {
            if last.kind == BlockKind::Comment {
                last.text.push_str(text);
                return;
            }
        }
    }
    blocks.push(Block { kind, text: text.to_string() });
}

// Anything unrecognised (including an ERROR node on invalid input) becomes a statement so
// byte-exact round-tripping holds regardless of parse errors.
fn classify(node: &Node) -> BlockKind {
    match node.kind() {
        "function_statement" => BlockKind::Function,
        "comment" => BlockKind::Comment,
        _ => BlockKind::Statement,
    }
}

fn parse(src: &str) -> Tree {
    let mut parser = Parser::new();
    parser
        .set_language(&inkjet::Language::Lua.config().language)
        .expect("lua grammar loads");
    parser.parse(src, None).expect("parse always returns a tree")
}

#[cfg(test)]
mod tests;
