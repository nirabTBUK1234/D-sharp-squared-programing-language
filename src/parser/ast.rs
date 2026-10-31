/// How a binding may be used once declared.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VarKind {
    Let,
    Mut,
    Const,
    Static,
}

impl VarKind {
    pub fn name(self) -> &'static str {
        match self {
            VarKind::Let => "let",
            VarKind::Mut => "mut",
            VarKind::Const => "const",
            VarKind::Static => "static",
        }
    }

    pub fn can_reassign(self) -> bool {
        matches!(self, VarKind::Mut)
    }

    pub fn can_delete(self) -> bool {
        matches!(self, VarKind::Let | VarKind::Mut)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UnaryOp {
    Not,
    Neg,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BinaryOp {
    Or,
    And,
    Is,
    IsNot,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Bool(bool),
    Text(String),
    Var(String),
    HashSet(Vec<Expr>),
    Matrix(Vec<(String, Vec<(String, Expr)>)>),
    Call {
        receiver: String,
        method: String,
        args: Vec<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    Declare {
        kind: VarKind,
        name: String,
        value: Expr,
    },
    Assign {
        name: String,
        value: Expr,
    },
    Delete {
        name: String,
    },
    TypeAlias {
        name: String,
        target: String,
    },
    If {
        condition: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Option<Vec<Stmt>>,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    Loop {
        body: Vec<Stmt>,
    },
    For {
        name: String,
        iterable: Expr,
        body: Vec<Stmt>,
    },
    Break,
    Continue,
    Block(Vec<Stmt>),
    Expression(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: usize,
}
