use ltk_ritobin::parser::{
    real::{Tree, TreeKind, Visit, Visitor},
    tokenizer::Token,
};

pub trait CstExt {
    fn find_node(&self, byte_index: u32) -> Option<(Vec<TreeKind>, Token)>;
}

struct NodeFinder {
    stack: Vec<TreeKind>,
    offset: u32,
    found: Option<Token>,
}

impl NodeFinder {
    pub fn new(offset: u32) -> Self {
        Self {
            stack: Vec::new(),
            offset,
            found: None,
        }
    }
}

impl Visitor for NodeFinder {
    fn visit_token(&mut self, token: &Token, context: TreeKind) -> Visit {
        if token.span.contains(self.offset) {
            self.found.replace(*token);
            return Visit::Stop;
        }

        Visit::Continue
    }

    fn enter_tree(&mut self, kind: TreeKind) -> Visit {
        self.stack.push(kind);
        Visit::Continue
    }
    fn exit_tree(&mut self, kind: TreeKind) -> Visit {
        self.stack.pop();
        Visit::Continue
    }
}

impl CstExt for Tree {
    fn find_node(&self, byte_index: u32) -> Option<(Vec<TreeKind>, Token)> {
        let mut visitor = NodeFinder::new(byte_index);

        self.walk(&mut visitor);

        visitor.found.map(|tok| (visitor.stack, tok))
    }
}
