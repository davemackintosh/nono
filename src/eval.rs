//! Evaluation: AST + data -> HTML tree.
//!
//! This is the middle of the pipeline. It walks `Node`s with an environment of
//! bound variables, folds `for`/`if`/`match` away, expands user components by
//! binding their parameters and filling their `Slot()` holes, and resolves
//! expressions (including string interpolation) to `Value`s.

use crate::ast::*;
use crate::html::{known_html_tags, Html};
use crate::sources;
use crate::value::Value;
use anyhow::{anyhow, bail, Result};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

/// Slots passed into a component invocation: the default block plus any named.
/// Fills also capture the environment they were written in, so a fill that
/// references a caller's loop variable resolves correctly when the callee
/// finally renders the slot.
#[derive(Clone, Default)]
struct SlotFills {
    default: Vec<Node>,
    named: HashMap<String, Vec<Node>>,
    /// The caller's environment at the point of the invocation. Empty for the
    /// top-level page render.
    capture: Env,
    /// The slot fills that were active where these fill nodes were written, so a
    /// `Slot()` appearing inside a fill resolves to the enclosing component's
    /// slot instead of vanishing. This is what lets a layout pass its body down
    /// through another component, e.g. `Post(...) { Slot() }`. None at the top.
    capture_fills: Option<Box<SlotFills>>,
}

pub struct Evaluator {
    components: HashMap<String, Component>,
    consts: HashMap<String, Value>,
    html_tags: std::collections::BTreeSet<&'static str>,
    root: PathBuf,
}

/// A scope of local variable bindings (loop vars, match bindings, params).
type Env = HashMap<String, Value>;

impl Evaluator {
    /// Build an evaluator from parsed items. Evaluates `const`s eagerly (this
    /// is where data sources like glob/lastfm actually run).
    pub fn new(items: Vec<Item>, root: PathBuf) -> Result<Self> {
        let mut components = HashMap::new();
        let mut const_decls = Vec::new();

        for item in items {
            match item {
                Item::Component(c) => {
                    components.insert(c.name.clone(), c);
                }
                Item::Const(c) => const_decls.push(c),
                Item::Stylesheet(_) => {} // collected separately by the caller
            }
        }

        let mut ev = Evaluator {
            components,
            consts: HashMap::new(),
            html_tags: known_html_tags(),
            root,
        };

        // Evaluate consts in declaration order against an empty local env.
        let empty = Env::new();
        for c in const_decls {
            let v = ev.eval_expr(&c.value, &empty)?;
            ev.consts.insert(c.name, v);
        }

        Ok(ev)
    }

    pub fn has_component(&self, name: &str) -> bool {
        self.components.contains_key(name)
    }

    /// Render a top-level component (a page) to an HTML node list.
    pub fn render_component(&self, name: &str) -> Result<Vec<Html>> {
        let comp = self
            .components
            .get(name)
            .ok_or_else(|| anyhow!("no component named {}", name))?;
        let env = Env::new();
        let fills = SlotFills::default();
        self.eval_nodes(&comp.body, &env, &fills)
    }

    /// Render a layout for a content (`.md`) page: bind the markdown
    /// frontmatter to the layout component's declared parameters, and feed the
    /// rendered markdown body into its default `Slot()` as raw HTML. This is the
    /// mirror of `render_component` for the inverse-ownership half of the router
    /// (`.md` files that are pages in their own right).
    ///
    /// The body is injected as the default slot fill, so a layout marks where it
    /// goes with `Slot()`. That `Slot()` may sit inside raw HTML or be handed
    /// down through another component (e.g. `Post(...) { Slot() }`); both resolve.
    pub fn render_layout(
        &self,
        component: &str,
        frontmatter: &BTreeMap<String, Value>,
        body_html: String,
    ) -> Result<Vec<Html>> {
        let comp = self
            .components
            .get(component)
            .ok_or_else(|| anyhow!("no layout component named `{}`", component))?;

        // Bind the layout's declared params from frontmatter fields. A layout is
        // a function of the fields it names; anything it doesn't declare, it
        // doesn't see (same scoping rule as any other component).
        let mut scope = Env::new();
        for param in &comp.params {
            match frontmatter.get(&param.name) {
                Some(v) => {
                    scope.insert(param.name.clone(), v.clone());
                }
                None => bail!(
                    "layout `{}` declares `{}: {}`, but the frontmatter has no `{}` field",
                    component,
                    param.name,
                    param.ty,
                    param.name
                ),
            }
        }

        let fills = SlotFills {
            default: vec![Node::RawHtml(body_html)],
            named: HashMap::new(),
            capture: scope.clone(),
            capture_fills: None,
        };

        self.eval_nodes(&comp.body, &scope, &fills)
    }

    // ---- nodes ----

    fn eval_nodes(&self, nodes: &[Node], env: &Env, fills: &SlotFills) -> Result<Vec<Html>> {
        let mut out = Vec::new();
        // Thread a local scope so `const` bindings are visible to later siblings.
        let mut scope = env.clone();
        for n in nodes {
            match n {
                Node::LocalConst(name, expr) => {
                    let v = self.eval_expr(expr, &scope)?;
                    scope.insert(name.clone(), v);
                }
                _ => out.extend(self.eval_node(n, &scope, fills)?),
            }
        }
        Ok(out)
    }

    fn eval_node(&self, node: &Node, env: &Env, fills: &SlotFills) -> Result<Vec<Html>> {
        match node {
            Node::Text(t) => Ok(vec![Html::Text(self.eval_template(t, env)?)]),
            Node::RawHtml(s) => Ok(vec![Html::Raw(s.clone())]),
            Node::Element(el) => self.eval_element(el, env, fills),
            Node::Slot(slot) => self.eval_slot(slot, env, fills),
            Node::For(f) => self.eval_for(f, env, fills),
            Node::If(i) => self.eval_if(i, env, fills),
            Node::Match(m) => self.eval_match(m, env, fills),
            // NamedFill at render position means it appeared somewhere other than
            // directly inside a component-invocation block (where it is consumed
            // during expansion). That is a usage error.
            Node::NamedFill(name, _) => bail!(
                "named slot fill `{} = {{ ... }}` can only appear directly inside a component invocation",
                name
            ),
            // LocalConst is handled by eval_nodes' sequential pass; reaching it
            // here means it was encountered outside that pass.
            Node::LocalConst(name, _) => bail!(
                "internal: const `{}` reached eval_node directly",
                name
            ),
        }
    }

    fn eval_for(&self, f: &ForNode, env: &Env, fills: &SlotFills) -> Result<Vec<Html>> {
        let iter = self.eval_expr(&f.iter, env)?;
        let items = match iter {
            Value::List(l) => l,
            other => bail!("cannot iterate over {:?}", other),
        };
        let mut out = Vec::new();
        for item in items {
            let mut scope = env.clone();
            scope.insert(f.binding.clone(), item);
            out.extend(self.eval_nodes(&f.body, &scope, fills)?);
        }
        Ok(out)
    }

    fn eval_if(&self, i: &IfNode, env: &Env, fills: &SlotFills) -> Result<Vec<Html>> {
        let cond = self.eval_expr(&i.cond, env)?;
        if cond.truthy() {
            self.eval_nodes(&i.then, env, fills)
        } else if let Some(otherwise) = &i.otherwise {
            self.eval_nodes(otherwise, env, fills)
        } else {
            Ok(vec![])
        }
    }

    fn eval_match(&self, m: &MatchNode, env: &Env, fills: &SlotFills) -> Result<Vec<Html>> {
        let scrutinee = self.eval_expr(&m.scrutinee, env)?;
        let tag = scrutinee.tag();
        for arm in &m.arms {
            let matches = arm.pattern.tag == "_"
                || tag.as_deref() == Some(arm.pattern.tag.as_str());
            if matches {
                let mut scope = env.clone();
                if let Some(binding) = &arm.pattern.binding {
                    // bind the inner payload, or the whole value if untagged
                    let bound = match &scrutinee {
                        Value::Tagged(_, inner) => (**inner).clone(),
                        other => other.clone(),
                    };
                    scope.insert(binding.clone(), bound);
                }
                return self.eval_nodes(&arm.body, &scope, fills);
            }
        }
        // No arm matched: render nothing (could be made a hard error later).
        Ok(vec![])
    }

    fn eval_slot(&self, slot: &Slot, env: &Env, fills: &SlotFills) -> Result<Vec<Html>> {
        let filled = match &slot.name {
            None => Some(&fills.default),
            Some(n) => fills.named.get(n),
        };
        match filled {
            Some(nodes) if !nodes.is_empty() => {
                // Slot contents were captured at the call site and must evaluate
                // in the caller's environment, not the callee's. We also restore
                // the slot fills that were active there, so a `Slot()` nested
                // inside this fill resolves to the enclosing component's slot (a
                // layout handing its body through `Post(...) { Slot() }`) rather
                // than rendering nothing.
                let inner_fills = match &fills.capture_fills {
                    Some(f) => (**f).clone(),
                    None => SlotFills::default(),
                };
                self.eval_nodes(nodes, &fills.capture, &inner_fills)
            }
            _ => {
                // Unfilled: use the `or =` fallback if present.
                match &slot.or {
                    Some(expr) => {
                        let v = self.eval_expr(expr, env)?;
                        if let Value::Nil = v {
                            Ok(vec![])
                        } else {
                            Ok(vec![Html::Text(v.to_string())])
                        }
                    }
                    None => Ok(vec![]),
                }
            }
        }
    }

    fn eval_element(&self, el: &Element, env: &Env, fills: &SlotFills) -> Result<Vec<Html>> {
        // User component?
        if let Some(comp) = self.components.get(&el.name) {
            return self.expand_component(comp, el, env, fills);
        }

        // Otherwise treat as a raw HTML element if known.
        let lname = el.name.to_lowercase();
        if !self.html_tags.contains(lname.as_str()) {
            bail!(
                "unknown element or component `{}` (not a defined component, not a known HTML tag)",
                el.name
            );
        }

        // Collect attributes from named args; positional args become text/attr
        // depending — for raw HTML we only support named attributes.
        let mut attrs = Vec::new();
        for arg in &el.args {
            match arg {
                Arg::Named(name, ArgValue::Expr(e)) => {
                    let v = self.eval_expr(e, env)?;
                    attrs.push((name.clone(), v.to_string()));
                }
                Arg::Named(_, ArgValue::Block(_)) => {
                    bail!("HTML element <{}> cannot take a block-valued attribute", lname)
                }
                Arg::Positional(e) => {
                    // A positional arg on a raw element: treat as text child later.
                    // We disallow to keep things predictable.
                    let _ = e;
                    bail!("HTML element <{}> does not take positional arguments", lname)
                }
            }
        }

        let children = self.eval_nodes(&el.children, env, fills)?;
        Ok(vec![Html::Element {
            tag: lname,
            attrs,
            children,
        }])
    }

    /// Expand a user component: bind params, capture slot fills, eval its body.
    fn expand_component(
        &self,
        comp: &Component,
        call: &Element,
        env: &Env,
        outer_fills: &SlotFills,
    ) -> Result<Vec<Html>> {
        // Bind parameters from named/positional args.
        let mut scope = Env::new();

        // Positional args bind to params in order; named override by name.
        let mut positional = Vec::new();
        let mut named: HashMap<String, &ArgValue> = HashMap::new();
        for arg in &call.args {
            match arg {
                Arg::Positional(e) => positional.push(e),
                Arg::Named(n, v) => {
                    named.insert(n.clone(), v);
                }
            }
        }

        let mut pos_iter = positional.into_iter();
        for param in &comp.params {
            if let Some(v) = named.get(&param.name) {
                match v {
                    ArgValue::Expr(e) => {
                        scope.insert(param.name.clone(), self.eval_expr(e, env)?);
                    }
                    ArgValue::Block(_) => {
                        bail!("parameter `{}` expects a value, got a block", param.name)
                    }
                }
            } else if let Some(e) = pos_iter.next() {
                scope.insert(param.name.clone(), self.eval_expr(e, env)?);
            } else {
                bail!("missing argument `{}` for component {}", param.name, comp.name);
            }
        }

        // Build slot fills. The trailing block is split: `name = { ... }` nodes
        // become named slots; everything else is the default slot. Named-arg
        // blocks passed in the parens (ArgValue::Block) also become named slots.
        let mut default_nodes = Vec::new();
        let mut named_slots: HashMap<String, Vec<Node>> = HashMap::new();
        for child in &call.children {
            match child {
                Node::NamedFill(name, nodes) => {
                    if named_slots.insert(name.clone(), nodes.clone()).is_some() {
                        bail!("slot `{}` filled more than once", name);
                    }
                }
                other => default_nodes.push(other.clone()),
            }
        }
        for arg in &call.args {
            if let Arg::Named(n, ArgValue::Block(nodes)) = arg {
                if named_slots.insert(n.clone(), nodes.clone()).is_some() {
                    bail!("slot `{}` filled more than once", n);
                }
            }
        }
        let fills = SlotFills {
            default: default_nodes,
            named: named_slots,
            // Capture the caller's environment so fills referencing caller
            // locals (e.g. an enclosing `for` binding) resolve correctly.
            capture: env.clone(),
            // Capture the fills active here too, so a `Slot()` inside one of
            // these fills reaches the enclosing component's slot.
            capture_fills: Some(Box::new(outer_fills.clone())),
        };

        // The component body sees only its own parameters (plus consts, which
        // the evaluator resolves globally). It deliberately does NOT see the
        // caller's locals — those reach slot fills via `fills.capture`, not the
        // body. This keeps component scoping clean: a component is a function of
        // its declared parameters.
        let body_env = scope;

        self.eval_nodes(&comp.body, &body_env, &fills)
    }

    // ---- expressions ----

    fn eval_expr(&self, expr: &Expr, env: &Env) -> Result<Value> {
        match expr {
            Expr::Number(n) => Ok(Value::Number(*n)),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Nil => Ok(Value::Nil),
            Expr::Str(t) => Ok(Value::Str(self.eval_template(t, env)?)),
            Expr::Path(path) => self.eval_path(path, env),
            Expr::Call(path, args) => self.eval_call(path, args, env),
            Expr::Binary(l, op, r) => {
                let lv = self.eval_expr(l, env)?;
                let rv = self.eval_expr(r, env)?;
                self.eval_binop(lv, *op, rv)
            }
        }
    }

    fn eval_path(&self, path: &[String], env: &Env) -> Result<Value> {
        let head = &path[0];
        let mut current = if let Some(v) = env.get(head) {
            v.clone()
        } else if let Some(v) = self.consts.get(head) {
            v.clone()
        } else {
            bail!("unknown name `{}`", head);
        };
        for field in &path[1..] {
            current = current
                .get_field(field)
                .ok_or_else(|| anyhow!("no field `{}` on value", field))?;
        }
        Ok(current)
    }

    fn eval_call(&self, path: &[String], args: &[Arg], env: &Env) -> Result<Value> {
        let joined = path.join(".");
        // Resolve named/positional args to values.
        let mut named: HashMap<String, Value> = HashMap::new();
        let mut positional: Vec<Value> = Vec::new();
        for arg in args {
            match arg {
                Arg::Named(n, ArgValue::Expr(e)) => {
                    named.insert(n.clone(), self.eval_expr(e, env)?);
                }
                Arg::Named(_, ArgValue::Block(_)) => {
                    bail!("data source call cannot take a block argument")
                }
                Arg::Positional(e) => positional.push(self.eval_expr(e, env)?),
            }
        }

        match joined.as_str() {
            "glob" => {
                let pattern = positional
                    .get(0)
                    .map(|v| v.to_string())
                    .ok_or_else(|| anyhow!("glob requires a pattern"))?;
                sources::glob(&self.root, &pattern)
            }
            "markdown" => {
                let file = named
                    .get("file")
                    .or_else(|| positional.get(0))
                    .map(|v| v.to_string())
                    .ok_or_else(|| anyhow!("markdown requires a file"))?;
                sources::read_markdown(&self.root, &file)
            }
            "lastfm.recent" => {
                let user = named
                    .get("user")
                    .map(|v| v.to_string())
                    .ok_or_else(|| anyhow!("lastfm.recent requires user="))?;
                let limit = named
                    .get("limit")
                    .map(|v| match v {
                        Value::Number(n) => *n as u32,
                        _ => 10,
                    })
                    .unwrap_or(10);
                sources::lastfm_recent(&user, limit)
            }
            other => bail!("unknown function `{}`", other),
        }
    }

    fn eval_binop(&self, l: Value, op: BinOp, r: Value) -> Result<Value> {
        use BinOp::*;
        let result = match op {
            Eq => Value::Bool(values_eq(&l, &r)),
            Ne => Value::Bool(!values_eq(&l, &r)),
            Add | Sub | Mul | Div => {
                let (a, b) = (as_num(&l)?, as_num(&r)?);
                let n = match op {
                    Add => a + b,
                    Sub => a - b,
                    Mul => a * b,
                    Div => a / b,
                    _ => unreachable!(),
                };
                Value::Number(n)
            }
            Lt | Le | Gt | Ge => {
                let (a, b) = (as_num(&l)?, as_num(&r)?);
                let bool = match op {
                    Lt => a < b,
                    Le => a <= b,
                    Gt => a > b,
                    Ge => a >= b,
                    _ => unreachable!(),
                };
                Value::Bool(bool)
            }
        };
        Ok(result)
    }

    /// Evaluate an interpolated string template into a finished String.
    fn eval_template(&self, t: &StrTemplate, env: &Env) -> Result<String> {
        let mut out = String::new();
        for part in &t.parts {
            match part {
                StrPart::Lit(s) => out.push_str(s),
                StrPart::Interp(e) => {
                    let v = self.eval_expr(e, env)?;
                    out.push_str(&v.to_string());
                }
            }
        }
        Ok(out)
    }
}

fn as_num(v: &Value) -> Result<f64> {
    match v {
        Value::Number(n) => Ok(*n),
        other => bail!("expected a number, got {:?}", other),
    }
}

fn values_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Number(x), Value::Number(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Nil, Value::Nil) => true,
        _ => false,
    }
}
