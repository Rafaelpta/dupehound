(function_definition
  declarator: [
    (function_declarator declarator: (_) @name)
    (pointer_declarator (function_declarator declarator: (_) @name))
    (pointer_declarator (pointer_declarator (function_declarator declarator: (_) @name)))
  ]
  body: (compound_statement) @body) @func
