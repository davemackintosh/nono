//! The Nono abstract syntax tree.
//!
//! Parsing (in `parser.rs`) turns a `.nono` source file into a `File` of
//! `Item`s. Evaluation (in `eval.rs`) folds the dynamic constructs away and
//! produces a tree of HTML nodes, which `html.rs` serialises.

#[derive(Debug, Clone)]
pub struct File {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Stylesheet(Stylesheet),
    Component(Component),
    Function(Function),
    Const(ConstDecl),
}

#[derive(Debug, Clone)]
pub struct Stylesheet {
    pub rules: Vec<StyleRule>,
}

#[derive(Debug, Clone)]
pub struct StyleRule {
    pub selector: String,
    pub decls: Vec<(String, String)>, // (prop, value)
}

#[derive(Debug, Clone)]
pub struct ConstDecl {
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct Component {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Vec<Node>,
}

/// A value-returning function: `fn name(params) = expr`. Unlike a component, its
/// body is a single expression and it yields a `Value`, not markup.
#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: String,
    /// `name?: type` — the argument may be omitted, in which case it binds nil.
    pub optional: bool,
}

/// A node is anything that can appear inside a block.
#[derive(Debug, Clone)]
pub enum Node {
    /// A string literal sitting bare in a block becomes a text node.
    Text(StrTemplate),
    /// An element or component invocation: `Name(args) { children }`.
    Element(Element),
    /// A slot hole: `Slot()` or `Slot(named = "x", or = nil)`.
    Slot(Slot),
    /// A named slot fill inside a block: `sidebar = { ... }`. Pure syntax,
    /// routed into the enclosing element's named-slot channel.
    NamedFill(String, Vec<Node>),
    /// A block-local bind-time constant: `const x = expr`.
    LocalConst(String, Expr),
    /// Pre-rendered raw HTML injected by the build driver, never by the parser.
    /// Used to feed a markdown body into a layout's default `Slot()` as a fill.
    RawHtml(String),
    For(ForNode),
    If(IfNode),
    Match(MatchNode),
}

#[derive(Debug, Clone)]
pub struct Element {
    pub name: String,
    pub args: Vec<Arg>,
    /// The trailing block, if present. Fills the element's default `Slot()`.
    pub children: Vec<Node>,
}

#[derive(Debug, Clone)]
pub enum Arg {
    /// `name = value` — value is either an expression or a block.
    Named(String, ArgValue),
    /// A bare positional expression, e.g. `Heading(1)`.
    Positional(Expr),
}

#[derive(Debug, Clone)]
pub enum ArgValue {
    Expr(Expr),
    Block(Vec<Node>),
}

#[derive(Debug, Clone)]
pub struct Slot {
    /// `named = "sidebar"` if present; None is the default slot.
    pub name: Option<String>,
    /// `or = <expr>` fallback when the slot is unfilled. nil means "render nothing".
    pub or: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct ForNode {
    pub binding: String,
    pub iter: Expr,
    pub body: Vec<Node>,
}

#[derive(Debug, Clone)]
pub struct IfNode {
    pub cond: Expr,
    pub then: Vec<Node>,
    pub otherwise: Option<Vec<Node>>,
}

#[derive(Debug, Clone)]
pub struct MatchNode {
    pub scrutinee: Expr,
    pub arms: Vec<MatchArm>,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Vec<Node>,
}

#[derive(Debug, Clone)]
pub struct Pattern {
    /// The variant/tag name, e.g. `Essay`. `_` matches anything.
    pub tag: String,
    /// An optional binding capturing the matched value, e.g. `Some(x)`.
    pub binding: Option<String>,
}

// ---- expressions ----

#[derive(Debug, Clone)]
pub enum Expr {
    Number(f64),
    Bool(bool),
    Nil,
    Str(StrTemplate),
    /// A dotted path used as a value: `track.artist`, `posts`.
    Path(Vec<String>),
    /// A call: `http_get("...")`, `glob("...")`, or a user function `my_fn(x = 1)`.
    Call(Vec<String>, Vec<Arg>),
    /// A `.field` accessor applied to any expression: `http_get(url).recenttracks`.
    Field(Box<Expr>, String),
    /// A `["key"]` (or `[index]`) accessor: `track.artist["#text"]`, `xs[0]`.
    Index(Box<Expr>, Box<Expr>),
    Binary(Box<Expr>, BinOp, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
}

/// An interpolated string: a sequence of literal chunks and `{expr}` holes.
#[derive(Debug, Clone)]
pub struct StrTemplate {
    pub parts: Vec<StrPart>,
}

#[derive(Debug, Clone)]
pub enum StrPart {
    Lit(String),
    Interp(Expr),
}
