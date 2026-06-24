#!/usr/bin/env python3
"""
A parsimonious PEG that approximates nono.pest closely enough to confirm the
example .nono files are structurally parseable. This is a sanity check on the
GRAMMAR SHAPE, not the real Rust parser — but if these files parse here, the
pest grammar (which is stricter and better) will accept them too.

The main risks we're checking:
  - string interpolation tokenising correctly (braces inside quotes)
  - Slot(...) vs element disambiguation
  - named args with block values: name = { ... }
  - for / if / match / nested blocks
"""
import sys
from parsimonious.grammar import Grammar
from parsimonious.exceptions import ParseError, IncompleteParseError

GRAMMAR = r"""
file        = _ item*
item        = (stylesheet / component / const_decl) _

stylesheet  = "stylesheet" _ "{" _ style_rule* "}" _
style_rule  = ident _ "{" _ style_decl* "}" _
style_decl  = css_prop _ "=" _ css_value nl _
css_prop    = ~r"[A-Za-z0-9-]+"
css_value   = ~r"[^\n]+"

const_decl  = "const" _ ident _ "=" _ expr _

component   = "component" _ ident _ params? _ block _
params      = "(" _ (param (_ "," _ param)*)? _ ","? _ ")"
param       = ident _ ":" _ ident

block       = "{" _ node* "}" _
node        = (for_node / if_node / match_node / slot_node / local_const / named_fill / element / text_node) _
named_fill  = ident _ "=" _ block
local_const = "const" _ ident _ "=" _ expr

for_node    = "for" _ ident _ "in" _ expr _ block
if_node     = "if" _ expr _ block (_ "else" _ (if_node / block))?
match_node  = "match" _ expr _ "{" _ match_arm+ "}"
match_arm   = pattern _ "=>" _ (block / node) _
pattern     = ident ("(" ident ")")?

slot_node   = "Slot" _ "(" _ (slot_arg (_ "," _ slot_arg)*)? _ ","? _ ")"
slot_arg    = ident _ "=" _ expr

element     = ident _ call_args? _ block?
call_args   = "(" _ (arg (_ "," _ arg)*)? _ ","? _ ")"
arg         = named_arg / expr
named_arg   = attr_name _ "=" _ arg_value
attr_name   = ~r"[A-Za-z_][A-Za-z0-9_-]*"
arg_value   = block / expr

text_node   = string

expr        = primary (_ binop _ primary)*
binop       = "==" / "!=" / "<=" / ">=" / "<" / ">" / "+" / "-" / "*" / "/"
primary     = call / number / bool / nil / string / path
call        = path "(" _ (arg (_ "," _ arg)*)? _ ","? _ ")"
path        = ident ("." ident)*

string      = "\"" str_inner "\""
str_inner   = (interpolation / escape / str_char)*
str_char    = ~r"[^\"\\{]"
escape      = "\\" ~r"[\"\\nt{}]"
interpolation = "{" _ expr _ "}"

number      = ~r"-?[0-9]+(\.[0-9]+)?"
bool        = "true" / "false"
nil         = "nil"
ident       = ~r"[A-Za-z_][A-Za-z0-9_]*"

nl          = ~r"[ \t]*\n"
_           = ~r"(\s|//[^\n]*)*"
"""

def main(paths):
    grammar = Grammar(GRAMMAR)
    failures = 0
    for path in paths:
        with open(path) as f:
            src = f.read()
        try:
            grammar.parse(src)
            print(f"  PARSE OK   {path}")
        except (ParseError, IncompleteParseError) as e:
            failures += 1
            print(f"  PARSE FAIL {path}")
            print("    " + str(e).splitlines()[0])
    return failures

if __name__ == "__main__":
    sys.exit(1 if main(sys.argv[1:]) else 0)
