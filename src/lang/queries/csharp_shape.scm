; Experimental "class shape" capture: a type declaration and its member list.
; The body is walked in extract.rs to build a set of member signatures
; (property/field/method signatures, bodies dropped), not winnowed.
(class_declaration
  name: (identifier) @name
  body: (declaration_list) @body) @func

(struct_declaration
  name: (identifier) @name
  body: (declaration_list) @body) @func

(record_declaration
  name: (identifier) @name
  body: (declaration_list) @body) @func
