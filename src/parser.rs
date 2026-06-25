//! Parsing: `.nono` source text -> pest parse tree -> `ast::File`.

use crate::ast::*;
use anyhow::{anyhow, bail, Result};
use pest::iterators::Pair;
use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "nono.pest"]
struct NonoParser;

pub fn parse(src: &str) -> Result<File> {
    let mut pairs = NonoParser::parse(Rule::file, src)
        .map_err(|e| anyhow!("parse error:\n{}", e))?;
    let file_pair = pairs.next().ok_or_else(|| anyhow!("empty parse"))?;

    let mut items = Vec::new();
    for p in file_pair.into_inner() {
        match p.as_rule() {
            Rule::stylesheet => items.push(Item::Stylesheet(parse_stylesheet(p)?)),
            Rule::component => items.push(Item::Component(parse_component(p)?)),
            Rule::function => items.push(Item::Function(parse_function(p)?)),
            Rule::const_decl => items.push(Item::Const(parse_const(p)?)),
            Rule::EOI => {}
            other => bail!("unexpected top-level rule: {:?}", other),
        }
    }
    Ok(File { items })
}

fn parse_stylesheet(p: Pair<Rule>) -> Result<Stylesheet> {
    let mut rules = Vec::new();
    for rule in p.into_inner() {
        if rule.as_rule() != Rule::style_rule {
            continue;
        }
        let mut inner = rule.into_inner();
        let selector = inner.next().unwrap().as_str().to_string();
        let mut decls = Vec::new();
        for decl in inner {
            if decl.as_rule() != Rule::style_decl {
                continue;
            }
            let mut d = decl.into_inner();
            let prop = d.next().unwrap().as_str().to_string();
            let value = d.next().unwrap().as_str().trim().to_string();
            decls.push((prop, value));
        }
        rules.push(StyleRule { selector, decls });
    }
    Ok(Stylesheet { rules })
}

fn parse_const(p: Pair<Rule>) -> Result<ConstDecl> {
    let mut inner = p.into_inner();
    let name = inner.next().unwrap().as_str().to_string();
    let value = parse_expr(inner.next().unwrap())?;
    Ok(ConstDecl { name, value })
}

fn parse_component(p: Pair<Rule>) -> Result<Component> {
    let mut inner = p.into_inner();
    let name = inner.next().unwrap().as_str().to_string();

    let mut params = Vec::new();
    let mut body = Vec::new();

    for part in inner {
        match part.as_rule() {
            Rule::params => params = parse_params(part)?,
            Rule::block => body = parse_block(part)?,
            other => bail!("unexpected in component: {:?}", other),
        }
    }
    Ok(Component { name, params, body })
}

fn parse_function(p: Pair<Rule>) -> Result<Function> {
    let mut inner = p.into_inner();
    let name = inner.next().unwrap().as_str().to_string();

    let mut params = Vec::new();
    let mut body = None;
    for part in inner {
        match part.as_rule() {
            Rule::params => params = parse_params(part)?,
            Rule::expr => body = Some(parse_expr(part)?),
            other => bail!("unexpected in function: {:?}", other),
        }
    }
    let body = body.ok_or_else(|| anyhow!("function {} has no body", name))?;
    Ok(Function { name, params, body })
}

fn parse_params(p: Pair<Rule>) -> Result<Vec<Param>> {
    let mut out = Vec::new();
    for param in p.into_inner() {
        if param.as_rule() != Rule::param {
            continue;
        }
        let mut i = param.into_inner();
        let name = i.next().unwrap().as_str().to_string();
        let ty = i.next().unwrap().as_str().to_string();
        out.push(Param { name, ty });
    }
    Ok(out)
}

fn parse_block(p: Pair<Rule>) -> Result<Vec<Node>> {
    let mut nodes = Vec::new();
    for n in p.into_inner() {
        nodes.push(parse_node(n)?);
    }
    Ok(nodes)
}

fn parse_node(p: Pair<Rule>) -> Result<Node> {
    match p.as_rule() {
        Rule::for_node => parse_for(p),
        Rule::if_node => parse_if(p),
        Rule::match_node => parse_match(p),
        Rule::slot_node => parse_slot(p),
        Rule::named_fill => {
            let mut i = p.into_inner();
            let name = i.next().unwrap().as_str().to_string();
            let block = parse_block(i.next().unwrap())?;
            Ok(Node::NamedFill(name, block))
        }
        Rule::local_const => {
            let mut i = p.into_inner();
            let name = i.next().unwrap().as_str().to_string();
            let value = parse_expr(i.next().unwrap())?;
            Ok(Node::LocalConst(name, value))
        }
        Rule::element => parse_element(p),
        Rule::text_node => {
            let s = p.into_inner().next().unwrap();
            Ok(Node::Text(parse_string(s)?))
        }
        other => bail!("unexpected node rule: {:?}", other),
    }
}

fn parse_for(p: Pair<Rule>) -> Result<Node> {
    let mut i = p.into_inner();
    let binding = i.next().unwrap().as_str().to_string();
    let iter = parse_expr(i.next().unwrap())?;
    let body = parse_block(i.next().unwrap())?;
    Ok(Node::For(ForNode { binding, iter, body }))
}

fn parse_if(p: Pair<Rule>) -> Result<Node> {
    let mut i = p.into_inner();
    let cond = parse_expr(i.next().unwrap())?;
    let then = parse_block(i.next().unwrap())?;
    let otherwise = match i.next() {
        Some(else_clause) => {
            let inner = else_clause.into_inner().next().unwrap();
            match inner.as_rule() {
                Rule::if_node => Some(vec![parse_if(inner)?]),
                Rule::block => Some(parse_block(inner)?),
                other => bail!("unexpected else content: {:?}", other),
            }
        }
        None => None,
    };
    Ok(Node::If(IfNode { cond, then, otherwise }))
}

fn parse_match(p: Pair<Rule>) -> Result<Node> {
    let mut i = p.into_inner();
    let scrutinee = parse_expr(i.next().unwrap())?;
    let mut arms = Vec::new();
    for arm in i {
        if arm.as_rule() != Rule::match_arm {
            continue;
        }
        let mut a = arm.into_inner();
        let pat = a.next().unwrap();
        let pattern = parse_pattern(pat)?;
        let body_pair = a.next().unwrap();
        let body = match body_pair.as_rule() {
            Rule::block => parse_block(body_pair)?,
            _ => vec![parse_node(body_pair)?],
        };
        arms.push(MatchArm { pattern, body });
    }
    Ok(Node::Match(MatchNode { scrutinee, arms }))
}

fn parse_pattern(p: Pair<Rule>) -> Result<Pattern> {
    let mut i = p.into_inner();
    let tag = i.next().unwrap().as_str().to_string();
    let binding = i.next().map(|b| b.as_str().to_string());
    Ok(Pattern { tag, binding })
}

fn parse_slot(p: Pair<Rule>) -> Result<Node> {
    let mut name = None;
    let mut or = None;
    for arg in p.into_inner() {
        if arg.as_rule() != Rule::slot_arg {
            continue;
        }
        let mut a = arg.into_inner();
        let key = a.next().unwrap().as_str().to_string();
        let val = parse_expr(a.next().unwrap())?;
        match key.as_str() {
            "named" => {
                // value must be a string literal; pull its literal text
                if let Expr::Str(t) = &val {
                    name = Some(template_to_plain(t)?);
                } else {
                    bail!("Slot named= expects a string literal");
                }
            }
            "or" => or = Some(val),
            other => bail!("unknown Slot argument: {}", other),
        }
    }
    Ok(Node::Slot(Slot { name, or }))
}

fn parse_element(p: Pair<Rule>) -> Result<Node> {
    let mut name = String::new();
    let mut args = Vec::new();
    let mut children = Vec::new();

    for part in p.into_inner() {
        match part.as_rule() {
            Rule::ident => name = part.as_str().to_string(),
            Rule::call_args => args = parse_call_args(part)?,
            Rule::block => children = parse_block(part)?,
            other => bail!("unexpected in element: {:?}", other),
        }
    }
    Ok(Node::Element(Element { name, args, children }))
}

fn parse_call_args(p: Pair<Rule>) -> Result<Vec<Arg>> {
    let mut out = Vec::new();
    for arg in p.into_inner() {
        if arg.as_rule() != Rule::arg {
            continue;
        }
        out.push(parse_arg(arg)?);
    }
    Ok(out)
}

fn parse_arg(p: Pair<Rule>) -> Result<Arg> {
    let inner = p.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::named_arg => {
            let mut i = inner.into_inner();
            let name = i.next().unwrap().as_str().to_string();
            let val = i.next().unwrap(); // arg_value
            let vinner = val.into_inner().next().unwrap();
            let value = match vinner.as_rule() {
                Rule::block => ArgValue::Block(parse_block(vinner)?),
                Rule::expr => ArgValue::Expr(parse_expr(vinner)?),
                other => bail!("unexpected arg value: {:?}", other),
            };
            Ok(Arg::Named(name, value))
        }
        Rule::expr => Ok(Arg::Positional(parse_expr(inner)?)),
        other => bail!("unexpected arg: {:?}", other),
    }
}

// ---- expressions ----

fn parse_expr(p: Pair<Rule>) -> Result<Expr> {
    // expr = primary (binop primary)*
    let mut inner = p.into_inner();
    let first = inner.next().unwrap();
    let mut lhs = parse_primary(first)?;
    while let Some(op_pair) = inner.next() {
        let op = parse_binop(op_pair.as_str())?;
        let rhs_pair = inner.next().ok_or_else(|| anyhow!("binop missing rhs"))?;
        let rhs = parse_primary(rhs_pair)?;
        lhs = Expr::Binary(Box::new(lhs), op, Box::new(rhs));
    }
    Ok(lhs)
}

fn parse_binop(s: &str) -> Result<BinOp> {
    Ok(match s {
        "==" => BinOp::Eq,
        "!=" => BinOp::Ne,
        "<" => BinOp::Lt,
        "<=" => BinOp::Le,
        ">" => BinOp::Gt,
        ">=" => BinOp::Ge,
        "+" => BinOp::Add,
        "-" => BinOp::Sub,
        "*" => BinOp::Mul,
        "/" => BinOp::Div,
        other => bail!("unknown binary operator: {}", other),
    })
}

fn parse_primary(p: Pair<Rule>) -> Result<Expr> {
    match p.as_rule() {
        Rule::number => Ok(Expr::Number(p.as_str().parse()?)),
        Rule::bool => Ok(Expr::Bool(p.as_str() == "true")),
        Rule::nil => Ok(Expr::Nil),
        Rule::string => Ok(Expr::Str(parse_string(p)?)),
        Rule::postfix => parse_postfix(p),
        Rule::expr => parse_expr(p),
        other => bail!("unexpected primary: {:?}", other),
    }
}

/// A base value (call / path / parenthesised expr) followed by any chain of
/// `.field` and `["key"]` accessors, folded left to right into Field/Index.
fn parse_postfix(p: Pair<Rule>) -> Result<Expr> {
    let mut inner = p.into_inner();
    let base = inner.next().unwrap();
    let mut cur = match base.as_rule() {
        Rule::call => {
            let mut i = base.into_inner();
            let path = parse_path(i.next().unwrap());
            let mut args = Vec::new();
            for arg in i {
                if arg.as_rule() == Rule::arg {
                    args.push(parse_arg(arg)?);
                }
            }
            Expr::Call(path, args)
        }
        Rule::field_access => {
            let path_pair = base.into_inner().next().unwrap();
            Expr::Path(parse_path(path_pair))
        }
        Rule::paren => parse_expr(base.into_inner().next().unwrap())?,
        other => bail!("unexpected postfix base: {:?}", other),
    };
    for acc in inner {
        match acc.as_rule() {
            Rule::field_suffix => {
                let name = acc.into_inner().next().unwrap().as_str().to_string();
                cur = Expr::Field(Box::new(cur), name);
            }
            Rule::index => {
                let key = parse_expr(acc.into_inner().next().unwrap())?;
                cur = Expr::Index(Box::new(cur), Box::new(key));
            }
            other => bail!("unexpected accessor: {:?}", other),
        }
    }
    Ok(cur)
}

fn parse_path(p: Pair<Rule>) -> Vec<String> {
    p.as_str().split('.').map(|s| s.to_string()).collect()
}

// ---- strings + interpolation ----

fn parse_string(p: Pair<Rule>) -> Result<StrTemplate> {
    // p is Rule::string -> str_inner -> (interpolation | escape | str_char)*
    let inner = p.into_inner().next().unwrap(); // str_inner
    let mut parts: Vec<StrPart> = Vec::new();
    let mut buf = String::new();

    for piece in inner.into_inner() {
        match piece.as_rule() {
            Rule::str_char => buf.push_str(piece.as_str()),
            Rule::escape => {
                let c = match piece.as_str() {
                    "\\\"" => '"',
                    "\\\\" => '\\',
                    "\\n" => '\n',
                    "\\t" => '\t',
                    "\\{" => '{',
                    "\\}" => '}',
                    other => bail!("bad escape: {}", other),
                };
                buf.push(c);
            }
            Rule::interpolation => {
                if !buf.is_empty() {
                    parts.push(StrPart::Lit(std::mem::take(&mut buf)));
                }
                let e = piece.into_inner().next().unwrap();
                parts.push(StrPart::Interp(parse_expr(e)?));
            }
            other => bail!("unexpected string piece: {:?}", other),
        }
    }
    if !buf.is_empty() {
        parts.push(StrPart::Lit(buf));
    }
    Ok(StrTemplate { parts })
}

/// Collapse a template with no interpolations into a plain String.
fn template_to_plain(t: &StrTemplate) -> Result<String> {
    let mut s = String::new();
    for part in &t.parts {
        match part {
            StrPart::Lit(l) => s.push_str(l),
            StrPart::Interp(_) => bail!("expected a plain string, found interpolation"),
        }
    }
    Ok(s)
}
