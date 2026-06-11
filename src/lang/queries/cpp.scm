(function_definition
  declarator: (function_declarator
    declarator: (identifier) @name)
  body: (compound_statement) @body) @func

(function_definition
  declarator: (function_declarator
    declarator: (field_identifier) @name)
  body: (compound_statement) @body) @func
