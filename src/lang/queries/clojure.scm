(list_lit
  .
  (sym_lit name: (sym_name) @_kw)
  .
  (sym_lit name: (sym_name) @name)
  .
  (#any-of? @_kw "defn" "defn-" "defmacro" "deftest" "defmulti" "defmethod")) @func @body

; (def name (fn [args] ...)) -- a function value bound via def instead of
; defn. Mirrors javascript.scm's variable_declarator pattern: only fn-valued
; defs match, so a bare (def x 5) or (def config {...}) is left alone. @body
; is the inner (fn ...) form, not the whole def -- unlike defn, def actually
; has a distinct value node to point at, so there's no need to fall back to
; "the whole form is the body" here.
(list_lit
  .
  (sym_lit name: (sym_name) @_kw)
  .
  (sym_lit name: (sym_name) @name)
  .
  (list_lit
    .
    (sym_lit name: (sym_name) @_fnkw)
    .
    (#eq? @_fnkw "fn")) @body
  (#eq? @_kw "def")) @func

; Protocol method implementations inside defrecord/deftype/extend-type/
; extend-protocol/reify. No keyword of their own -- the method's own name
; is the list's head -- so the only way to distinguish one from an
; ordinary function call shaped the same way (symbol, then a vector
; argument, e.g. (zipmap [:a :b] [1 2])) is position: it must be a direct
; child of one of these five forms. Nesting in this query mirrors direct
; parent-child structure in the tree, so a call like that buried inside
; some other function's body can never match here.
(list_lit
  .
  (sym_lit name: (sym_name) @_defkw)
  .
  (#any-of? @_defkw "defrecord" "deftype" "extend-type" "extend-protocol" "reify")
  (list_lit
    .
    (sym_lit name: (sym_name) @name)
    .
    (vec_lit)) @func @body)
