use ltk_ritobin::parse::{
    Token,
    cst::{
        Cst, TreeKind,
        visitor::{Visit, Visitor},
    },
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
    fn visit_token(&mut self, token: &Token, _context: &Cst) -> Visit {
        if token.span.contains(self.offset) {
            self.found.replace(*token);
            return Visit::Stop;
        }

        Visit::Continue
    }

    fn enter_tree(&mut self, tree: &Cst) -> Visit {
        self.stack.push(tree.kind);
        Visit::Continue
    }
    fn exit_tree(&mut self, _tree: &Cst) -> Visit {
        self.stack.pop();
        Visit::Continue
    }
}

impl CstExt for Cst {
    fn find_node(&self, byte_index: u32) -> Option<(Vec<TreeKind>, Token)> {
        let mut visitor = NodeFinder::new(byte_index);

        self.walk(&mut visitor);

        visitor.found.map(|tok| (visitor.stack, tok))
    }
}
