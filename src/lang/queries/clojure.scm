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

; (def name #(...)) -- the reader-macro shorthand for the same idiom. #(...)
; parses as its own node kind (anon_fn_lit), not a list_lit headed by "fn",
; so it needs its own pattern rather than an alternation on the one above.
(list_lit
  .
  (sym_lit name: (sym_name) @_kw)
  .
  (sym_lit name: (sym_name) @name)
  .
  (anon_fn_lit) @body
  (#eq? @_kw "def")) @func

; letfn's bindings are a third shape again: each is a list_lit shaped like
; (name [args] body...), same as defn/protocol methods, but nested inside
; the binding vec_lit rather than being a direct child of the letfn form
; itself or headed by any keyword of its own.
(list_lit
  .
  (sym_lit name: (sym_name) @_kw)
  .
  (#eq? @_kw "letfn")
  (vec_lit
    (list_lit
      .
      (sym_lit name: (sym_name) @name)
      .
      (vec_lit)) @func @body))

; Anonymous/unbound fn and #(...) literals -- passed directly as a callback
; argument (map, filter, reduce, ...), never bound to a name anywhere. No
; @name capture; extract.rs already falls back to "<anonymous>" for that.
; This necessarily overlaps the def+fn and def+#(...) patterns above (the
; SAME node matches both when it's def-bound), which analyze_source resolves
; by deduping on identical span and preferring the named match.
(anon_fn_lit) @func @body

(list_lit
  .
  (sym_lit name: (sym_name) @_fnkw)
  .
  (#eq? @_fnkw "fn")) @func @body

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
