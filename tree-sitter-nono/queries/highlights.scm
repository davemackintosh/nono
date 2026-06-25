; Highlights for Nono.
; Ordered general to specific so last-wins highlighters land on the specific one.

; ---- comments ----
(comment) @comment

; ---- literals ----
(string) @string
(escape_sequence) @string.escape
(number) @number
(boolean) @constant.builtin.boolean
(nil) @constant.builtin

; ---- operators and punctuation ----
(binary_operator) @operator
[
  "="
  "=>"
  ":"
] @operator

[
  "("
  ")"
  "{"
  "}"
] @punctuation.bracket

[
  ","
  "."
] @punctuation.delimiter

(interpolation
  [
    "{"
    "}"
  ] @punctuation.special)

; ---- keywords ----
[
  "component"
  "fn"
  "const"
  "stylesheet"
] @keyword

[
  "for"
  "in"
  "if"
  "else"
  "match"
] @keyword.control

"Slot" @function.builtin

; ---- identifiers in their roles ----
; A value reference: `post`, `track.artist`.
(field_access (path (identifier) @variable))

; A data-source / function call: `glob(...)`, `lastfm.recent(...)`.
(call function: (path (identifier) @function))

; Component and function definitions, element / component invocations.
(component name: (identifier) @type)
(function name: (identifier) @function)
(element name: (identifier) @function.method)

; A `.field` accessor reads as a property.
(member_accessor property: (identifier) @property)

; Parameters and types.
(parameter name: (identifier) @variable.parameter)
(type (identifier) @type)

; Named things that read as properties / attributes.
(named_argument name: (identifier) @property)
(named_argument name: (hyphenated_name) @property)
(slot_argument name: (identifier) @property)
(named_fill name: (identifier) @property)

; match patterns: `Note`, `Essay`, `_`.
(pattern tag: (identifier) @constant)
(pattern binding: (identifier) @variable)

; loop binding.
(for_statement binding: (identifier) @variable)

; ---- stylesheet ----
(style_rule selector: (identifier) @type)
(style_declaration property: (css_property) @property)
(css_value) @string
