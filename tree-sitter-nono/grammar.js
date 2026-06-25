/**
 * @file Tree-sitter grammar for Nono, a spite-driven static site generator DSL.
 * @license MIT
 *
 * Mirrors src/nono.pest. A few deliberate differences fall out of tree-sitter
 * being a GLR parser rather than a PEG:
 *
 *  - Keywords (component, const, for, if, else, match, in, stylesheet, Slot,
 *    true, false, nil) are reserved, via `word`. The pest grammar technically
 *    lets some of these be identifiers in places; nobody does that, and reserving
 *    them is what makes Slot() distinct from an element invocation.
 *  - `match post.kind { ... }` works because string literals carry their own tag,
 *    same as the evaluator.
 *  - Operators have no precedence (left to right), matching the language: the
 *    binary expression is a flat left-associative fold.
 */

/* eslint-disable arrow-parens */
/* eslint-disable camelcase */

module.exports = grammar({
  name: 'nono',

  word: $ => $.identifier,

  extras: $ => [
    /\s/,
    $.comment,
  ],

  rules: {
    source_file: $ => repeat($._item),

    comment: _ => token(seq('//', /[^\n]*/)),

    _item: $ => choice(
      $.stylesheet,
      $.component,
      $.function,
      $.const_declaration,
    ),

    // ---- stylesheet ----
    stylesheet: $ => seq('stylesheet', '{', repeat($.style_rule), '}'),
    style_rule: $ => seq(field('selector', $.identifier), '{', repeat($.style_declaration), '}'),
    style_declaration: $ => seq(field('property', $.css_property), '=', field('value', $.css_value)),
    css_property: _ => token(/[A-Za-z0-9-]+/),
    // A CSS value runs to end of line; we capture it raw and trust the CSS.
    css_value: _ => token(/[^\n]+/),

    // ---- const ----
    const_declaration: $ => seq(
      'const',
      field('name', $.identifier),
      '=',
      field('value', $._expression),
    ),

    // ---- component ----
    component: $ => seq(
      'component',
      field('name', $.identifier),
      optional(field('parameters', $.parameters)),
      field('body', $.block),
    ),
    parameters: $ => seq('(', commaSep($.parameter), ')'),
    parameter: $ => seq(field('name', $.identifier), ':', field('type', $.type)),
    type: $ => $.identifier,

    // ---- function: `fn name(params) = expr` (returns a value, not markup) ----
    function: $ => seq(
      'fn',
      field('name', $.identifier),
      optional(field('parameters', $.parameters)),
      '=',
      field('body', $._expression),
    ),

    // ---- block: the body of a component, slot fill, or control structure ----
    block: $ => seq('{', repeat($._node), '}'),

    _node: $ => choice(
      $.for_statement,
      $.if_statement,
      $.match_statement,
      $.slot,
      $.local_const,
      $.named_fill,
      $.element,
      $.text,
    ),

    // `sidebar = { ... }`: fills a named slot. Pure syntax; never binds a value.
    named_fill: $ => seq(field('name', $.identifier), '=', field('value', $.block)),

    local_const: $ => seq(
      'const',
      field('name', $.identifier),
      '=',
      field('value', $._expression),
    ),

    // ---- control flow ----
    for_statement: $ => seq(
      'for',
      field('binding', $.identifier),
      'in',
      field('iterable', $._expression),
      field('body', $.block),
    ),

    if_statement: $ => seq(
      'if',
      field('condition', $._expression),
      field('consequence', $.block),
      optional($.else_clause),
    ),
    else_clause: $ => seq('else', choice($.if_statement, $.block)),

    match_statement: $ => seq('match', field('value', $._expression), '{', repeat1($.match_arm), '}'),
    match_arm: $ => seq(field('pattern', $.pattern), '=>', field('body', choice($.block, $._node))),
    // `Tag` or `Tag(binding)`; `_` is just an identifier-shaped wildcard.
    pattern: $ => seq(field('tag', $.identifier), optional(seq('(', field('binding', $.identifier), ')'))),

    // ---- slots ----
    slot: $ => seq('Slot', '(', commaSep($.slot_argument), ')'),
    slot_argument: $ => seq(field('name', $.identifier), '=', field('value', $._expression)),

    // ---- element / component invocation ----
    element: $ => seq(
      field('name', $.identifier),
      optional(field('arguments', $.arguments)),
      optional(field('body', $.block)),
    ),
    arguments: $ => seq('(', commaSep($.argument), ')'),
    argument: $ => choice($.named_argument, $._expression),
    named_argument: $ => seq(
      field('name', $._attribute_name),
      '=',
      field('value', choice($.block, $._expression)),
    ),
    // Attribute names may contain hyphens (data-level, aria-label); plain
    // identifiers may not, so `a-b` stays subtraction.
    _attribute_name: $ => choice($.identifier, $.hyphenated_name),
    hyphenated_name: _ => token(/[A-Za-z_][A-Za-z0-9_]*(-[A-Za-z0-9_]+)+/),

    // Bare text: a string literal sitting in a block.
    text: $ => $.string,

    // ---- expressions ----
    _expression: $ => choice($._primary, $.binary_expression),

    // No operator precedence: a flat left-to-right fold.
    binary_expression: $ => prec.left(seq(
      field('left', $._expression),
      field('operator', $.binary_operator),
      field('right', $._expression),
    )),
    binary_operator: _ => choice('==', '!=', '<=', '>=', '<', '>', '+', '-', '*', '/'),

    _primary: $ => choice(
      $.number,
      $.boolean,
      $.nil,
      $.string,
      $.postfix_expression,
    ),

    // A base value followed by any chain of `.field` and `["key"]` accessors,
    // mirroring the pest grammar's `postfix = base ~ accessor*`.
    postfix_expression: $ => seq(
      field('base', choice($.call, $.field_access, $.parenthesized_expression)),
      repeat($._accessor),
    ),
    _accessor: $ => choice($.member_accessor, $.subscript_accessor),
    member_accessor: $ => seq('.', field('property', $.identifier)),
    subscript_accessor: $ => seq('[', field('index', $._expression), ']'),

    parenthesized_expression: $ => seq('(', $._expression, ')'),

    // `http_get(url)`, `glob("...")`, or a user function `my_fn(x = 1)`
    call: $ => seq(field('function', $.path), '(', commaSep($.argument), ')'),
    field_access: $ => $.path,
    path: $ => prec.left(seq($.identifier, repeat(seq('.', $.identifier)))),

    // ---- literals ----
    string: $ => seq(
      '"',
      repeat(choice($.interpolation, $.escape_sequence, $.string_content)),
      '"',
    ),
    string_content: _ => token.immediate(prec(1, /[^"\\{]+/)),
    escape_sequence: _ => token.immediate(/\\["\\nt{}]/),
    interpolation: $ => seq(token.immediate('{'), $._expression, '}'),

    number: _ => token(/-?\d+(\.\d+)?/),
    boolean: _ => choice('true', 'false'),
    nil: _ => 'nil',
    identifier: _ => /[A-Za-z_][A-Za-z0-9_]*/,
  },
});

/**
 * A comma-separated list with an optional trailing comma, possibly empty.
 * @param {RuleOrLiteral} rule
 * @returns {ChoiceRule}
 */
function commaSep(rule) {
  return optional(seq(rule, repeat(seq(',', rule)), optional(',')));
}
