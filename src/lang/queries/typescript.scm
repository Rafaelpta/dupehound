(function_declaration
  name: (identifier) @name
  body: (statement_block) @body) @func

(generator_function_declaration
  name: (identifier) @name
  body: (statement_block) @body) @func

(method_definition
  name: (_) @name
  body: (statement_block) @body) @func

(variable_declarator
  name: (identifier) @name
  value: [(arrow_function body: (statement_block) @body)
          (function_expression body: (statement_block) @body)
          (generator_function body: (statement_block) @body)]) @func

(pair
  key: (property_identifier) @name
  value: [(arrow_function body: (statement_block) @body)
          (function_expression body: (statement_block) @body)]) @func

(assignment_expression
  left: (_) @name
  right: [(arrow_function body: (statement_block) @body)
          (function_expression body: (statement_block) @body)]) @func
