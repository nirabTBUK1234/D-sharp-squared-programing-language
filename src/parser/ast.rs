#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UnaryOp {
    Not,
    Neg,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BinaryOp {
    Or,
    And,
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    In,
    NotIn,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub name: String,
    /// Optional `name: Type` annotation. The interpreter checks it when the
    /// function is called.
    pub type_annotation: Option<String>,
    pub default: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeParameter {
    pub name: String,
    pub bound: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructField {
    pub name: String,
    pub default: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Method {
    pub name: String,
    pub params: Vec<Parameter>,
    /// Optional `-> Type` return annotation.
    pub return_type: Option<String>,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraitMethod {
    pub name: String,
    pub params: Vec<Parameter>,
    /// Optional `-> Type` return annotation.
    pub return_type: Option<String>,
    /// `None` means the trait only requires the method; implementations must
    /// provide it. `Some` is a default method.
    pub body: Option<Vec<Stmt>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Callee {
    Function(String),
    Method {
        receiver: Box<Expr>,
        method: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Bool(bool),
    None,
    Text(String),
    Var(String),
    List(Vec<Expr>),
    TypedList {
        element_type: String,
        items: Vec<Expr>,
    },
    FormatCall {
        name: String,
        args: Vec<Expr>,
    },
    Map(Vec<(Expr, Expr)>),
    Matrix(Vec<(String, Vec<(String, Expr)>)>),
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },
    Slice {
        object: Box<Expr>,
        start: Option<Box<Expr>>,
        stop: Option<Box<Expr>>,
        step: Option<Box<Expr>>,
    },
    Field {
        object: Box<Expr>,
        name: String,
    },
    Call {
        callee: Callee,
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
    /// `name = value`: creates the name, or updates the nearest existing one.
    Assign {
        name: String,
        value: Expr,
    },
    IndexAssign {
        name: String,
        index: Expr,
        value: Expr,
    },
    SliceAssign {
        name: String,
        start: Option<Expr>,
        stop: Option<Expr>,
        step: Option<Expr>,
        value: Expr,
    },
    DeleteIndex {
        name: String,
        index: Expr,
    },
    DeleteSlice {
        name: String,
        start: Option<Expr>,
        stop: Option<Expr>,
        step: Option<Expr>,
    },
    FieldAssign {
        name: String,
        field: String,
        value: Expr,
    },
    Struct {
        name: String,
        fields: Vec<StructField>,
    },
    Impl {
        trait_name: Option<String>,
        name: String,
        methods: Vec<Method>,
    },
    Trait {
        name: String,
        methods: Vec<TraitMethod>,
    },
    Delete {
        name: String,
    },
    Function {
        name: String,
        type_params: Vec<TypeParameter>,
        params: Vec<Parameter>,
        /// Optional `-> Type` return annotation.
        return_type: Option<String>,
        body: Vec<Stmt>,
    },
    Return(Option<Expr>),
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
        /// One name for a normal loop, or two names for a hashmap entry loop
        /// (`for key, value in map`).
        names: Vec<String>,
        iterable: Expr,
        body: Vec<Stmt>,
    },
    Break,
    Continue,
    Block(Vec<Stmt>),
    /// Statements produced by an `import`. The runtime executes this file at
    /// most once, and labels any error from it with `file`.
    SourceFile {
        file: String,
        key: String,
        statements: Vec<Stmt>,
    },
    Expression(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: usize,
}
